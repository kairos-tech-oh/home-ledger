//! Putting the app's folder on the user's PATH, and taking it off, for the
//! Windows installer.
//!
//! This is here, not in the installer script, because NSIS cannot hold a long
//! PATH: past its string limit `ReadRegStr` gives back an empty string, and a
//! script that trusted it wrote the app's folder over everything else on the
//! PATH. Here the value is read and written whole through the registry API,
//! in its own type, so `%USERPROFILE%`-style entries keep working. A change
//! is only ever the old value with one entry added or one removed. Anything
//! else is refused before it is written, and the old value is copied aside
//! first, so it can always be put back.

// Only the Windows installer calls this; elsewhere the helpers are kept, and
// tested, but nothing uses them.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
use winreg::{RegKey, RegValue, enums::*};

/// Where the PATH lives, and where its last value before a change is kept.
pub const ENVIRONMENT: &str = "Environment";
pub const BACKUP: &str = r"Software\home-ledger\path-backup";

/// One PATH entry as Windows compares them: no case, no trailing slash.
fn same(entry: &str, dir: &str) -> bool {
    let tidy = |s: &str| s.trim().trim_end_matches(['\\', '/']).to_lowercase();
    !entry.trim().is_empty() && tidy(entry) == tidy(dir)
}

/// The PATH with `dir` added at the end, or None when it is already there.
pub fn with_dir(path: &str, dir: &str) -> Option<String> {
    if path.split(';').any(|e| same(e, dir)) {
        return None;
    }
    Some(if path.is_empty() {
        dir.to_string()
    } else if path.ends_with(';') {
        format!("{path}{dir}")
    } else {
        format!("{path};{dir}")
    })
}

/// The PATH without `dir`, every other entry exactly as it was, or None when
/// it was not there.
pub fn without_dir(path: &str, dir: &str) -> Option<String> {
    let entries: Vec<&str> = path.split(';').collect();
    if !entries.iter().any(|e| same(e, dir)) {
        return None;
    }
    Some(
        entries
            .into_iter()
            .filter(|e| !same(e, dir))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

/// The check every change passes before it is written: adding keeps the
/// whole old value as the start of the new one; removing keeps every other
/// entry, in order, and only ever shortens the value.
pub fn safe(old: &str, new: &str, dir: &str, adding: bool) -> bool {
    if adding {
        return new.starts_with(old) && new.len() > old.len() && new.ends_with(dir);
    }
    let kept: Vec<&str> = old.split(';').filter(|e| !same(e, dir)).collect();
    new.len() < old.len() && new.split(';').collect::<Vec<_>>() == kept
}

#[cfg(windows)]
fn decode(value: &RegValue) -> Result<String, String> {
    if !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) {
        return Err("PATH is not a text value; leaving it alone".into());
    }
    let wide: Vec<u16> = value
        .bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16(&wide[..end])
        .map_err(|_| "PATH is not readable text; leaving it alone".into())
}

#[cfg(windows)]
fn encode(text: &str, vtype: RegType) -> RegValue {
    let mut bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    bytes.extend_from_slice(&[0, 0]);
    RegValue { bytes, vtype }
}

/// Adds or removes `dir` on the PATH under `HKCU\<key>`. Returns whether it
/// changed anything. The key is a parameter only so tests can use their own.
#[cfg(windows)]
pub fn change(key: &str, backup: &str, dir: &str, adding: bool) -> Result<bool, String> {
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let (env, _) = user
        .create_subkey(key)
        .map_err(|e| format!("cannot open the user's environment: {e}"))?;
    let (old, vtype) = match env.get_raw_value("Path") {
        Ok(value) => (decode(&value)?, value.vtype),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), REG_EXPAND_SZ),
        Err(e) => return Err(format!("cannot read PATH: {e}")),
    };
    let new = if adding {
        with_dir(&old, dir)
    } else {
        without_dir(&old, dir)
    };
    let Some(new) = new else {
        return Ok(false);
    };
    if !safe(&old, &new, dir, adding) {
        return Err("the change to PATH did not check out; leaving it alone".into());
    }
    if !old.is_empty() {
        let (kept, _) = user
            .create_subkey(backup)
            .map_err(|e| format!("cannot keep a copy of PATH: {e}"))?;
        kept.set_raw_value("before", &encode(&old, vtype.clone()))
            .map_err(|e| format!("cannot keep a copy of PATH: {e}"))?;
    }
    env.set_raw_value("Path", &encode(&new, vtype))
        .map_err(|e| format!("cannot write PATH: {e}"))?;
    Ok(true)
}

#[cfg(not(windows))]
pub fn change(_key: &str, _backup: &str, _dir: &str, _adding: bool) -> Result<bool, String> {
    Err("only the Windows installer changes PATH this way".into())
}

/// `hl install-path add|remove <dir>`, run by the installer.
pub fn run(action: &str, dir: &str) -> std::process::ExitCode {
    let dir = dir.trim().trim_end_matches(['\\', '/']);
    if dir.is_empty() || dir.contains(';') {
        eprintln!("hl: not a folder to put on PATH: \"{dir}\"");
        return std::process::ExitCode::from(2);
    }
    match change(ENVIRONMENT, BACKUP, dir, action == "add") {
        Ok(changed) => {
            println!("{}", if changed { "changed" } else { "unchanged" });
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hl: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = r"C:\Users\me\AppData\Local\Home Ledger";

    /// A PATH past NSIS's limit, which is what the old script emptied.
    fn long_path() -> String {
        (0..80)
            .map(|i| format!(r"C:\Tools\Some Long Folder Name {i}\bin"))
            .chain(["%USERPROFILE%\\.cargo\\bin".to_string()])
            .collect::<Vec<_>>()
            .join(";")
    }

    #[test]
    fn adding_keeps_every_entry_of_a_long_path() {
        let old = long_path();
        assert!(old.len() > 2000);
        let new = with_dir(&old, APP).unwrap();
        assert_eq!(new, format!("{old};{APP}"));
        assert!(safe(&old, &new, APP, true));
        assert_eq!(with_dir(&new, APP), None, "once only");
        assert_eq!(with_dir(&new, &format!("{}\\", APP.to_lowercase())), None);
        assert_eq!(with_dir("", APP).unwrap(), APP);
        assert_eq!(with_dir(r"C:\a;", APP).unwrap(), format!(r"C:\a;{APP}"));
    }

    #[test]
    fn removing_takes_only_the_one_entry() {
        let old = format!(r"C:\a;;{APP};%USERPROFILE%\b;{APP}\");
        let new = without_dir(&old, APP).unwrap();
        assert_eq!(
            new, r"C:\a;;%USERPROFILE%\b",
            "the empty entry kept, both copies gone"
        );
        assert!(safe(&old, &new, APP, false));
        assert_eq!(without_dir(r"C:\a", APP), None);
    }

    #[test]
    fn the_check_refuses_what_wiped_the_path() {
        let old = long_path();
        assert!(
            !safe(&old, APP, APP, true),
            "the old bug: PATH replaced by the app"
        );
        assert!(!safe(&old, "", APP, false), "everything removed");
        assert!(!safe(&old, &old, APP, true));
        assert!(
            !safe("C:\\a;C:\\b", "C:\\b", APP, false),
            "another entry lost"
        );
    }

    /// Against a key of the test's own under HKCU\Software, never the real
    /// environment, and removed afterwards.
    #[cfg(windows)]
    #[test]
    fn a_change_is_written_whole_in_its_own_type_with_a_copy_kept() {
        let base = format!(
            r"Software\home-ledger-test-{}",
            uuid::Uuid::new_v4().simple()
        );
        let key = format!(r"{base}\Environment");
        let backup = format!(r"{base}\backup");
        struct Gone(String);
        impl Drop for Gone {
            fn drop(&mut self) {
                let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
            }
        }
        let _gone = Gone(base.clone());
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (env, _) = user.create_subkey(&key).unwrap();
        let old = long_path();
        env.set_raw_value("Path", &encode(&old, REG_EXPAND_SZ))
            .unwrap();

        {
            assert_eq!(change(&key, &backup, APP, true), Ok(true));
            let now = env.get_raw_value("Path").unwrap();
            assert_eq!(now.vtype, REG_EXPAND_SZ, "the type is kept");
            assert_eq!(decode(&now).unwrap(), format!("{old};{APP}"));
            let kept = user
                .open_subkey(&backup)
                .unwrap()
                .get_raw_value("before")
                .unwrap();
            assert_eq!(decode(&kept).unwrap(), old, "the old value is copied aside");

            assert_eq!(change(&key, &backup, APP, true), Ok(false), "once only");
            assert_eq!(change(&key, &backup, APP, false), Ok(true));
            assert_eq!(decode(&env.get_raw_value("Path").unwrap()).unwrap(), old);

            env.set_raw_value(
                "Path",
                &RegValue {
                    bytes: vec![1, 2, 3, 4],
                    vtype: REG_DWORD,
                },
            )
            .unwrap();
            assert!(
                change(&key, &backup, APP, true).is_err(),
                "not text: left alone"
            );
        }
    }
}
