//! `ledger` as a second name for `hl`.
//!
//! On Windows it is `ledger.cmd` beside `hl.exe`, in the folder the installer
//! puts on PATH, and the choice is recorded where the installer reads it, so
//! an update keeps it. Elsewhere it is a link in `~/.local/bin`.

use crate::out::{Failure, Outcome, show};
use crate::session::Session;
use serde_json::json;
use std::path::PathBuf;

fn here() -> Result<PathBuf, Failure> {
    std::env::current_exe().map_err(|e| Failure::Error(format!("cannot tell where hl is: {e}")))
}

#[cfg(windows)]
fn target() -> Result<PathBuf, Failure> {
    Ok(here()?.with_file_name("ledger.cmd"))
}

#[cfg(not(windows))]
fn target() -> Result<PathBuf, Failure> {
    let home = std::env::var_os("HOME").ok_or_else(|| Failure::Error("HOME is not set".into()))?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("bin")
        .join("ledger"))
}

/// Records the choice where the installer looks, so an update keeps it.
#[cfg(windows)]
fn remember(on: bool) {
    let _ = std::process::Command::new("reg")
        .args([
            "add",
            r"HKCU\Software\home-ledger\hl",
            "/v",
            "ledgerAlias",
            "/t",
            "REG_SZ",
            "/d",
            if on { "1" } else { "0" },
            "/f",
        ])
        .output();
}

#[cfg(not(windows))]
fn remember(_on: bool) {}

fn exists() -> bool {
    target()
        .map(|t| t.exists() || t.symlink_metadata().is_ok())
        .unwrap_or(false)
}

pub fn status(s: &Session) -> Outcome {
    let path = target()?;
    let on = exists();
    show(s.json, &json!({ "alias": on, "path": path }), |_| {
        if on {
            println!("`ledger` runs hl ({})", path.display());
        } else {
            println!("`ledger` is not set up; `hl alias on` adds it");
        }
    })
}

pub fn set(s: &Session, on: bool) -> Outcome {
    let path = target()?;
    if on {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| Failure::Error(format!("cannot make {}: {e}", dir.display())))?;
        }
        create(&path)?;
    } else if exists() {
        std::fs::remove_file(&path)
            .map_err(|e| Failure::Error(format!("cannot remove {}: {e}", path.display())))?;
    }
    remember(on);
    show(s.json, &json!({ "alias": on, "path": path }), |_| {
        if on {
            println!("`ledger` now runs hl ({})", path.display());
            #[cfg(not(windows))]
            if !on_path(path.parent()) {
                println!(
                    "note: {} is not on your PATH",
                    path.parent().unwrap().display()
                );
            }
        } else {
            println!("`ledger` removed");
        }
    })
}

#[cfg(windows)]
fn create(path: &std::path::Path) -> Outcome {
    std::fs::write(path, "@\"%~dp0hl.exe\" %*\r\n")
        .map_err(|e| Failure::Error(format!("cannot write {}: {e}", path.display())))
}

#[cfg(not(windows))]
fn create(path: &std::path::Path) -> Outcome {
    let _ = std::fs::remove_file(path);
    std::os::unix::fs::symlink(here()?, path)
        .map_err(|e| Failure::Error(format!("cannot link {}: {e}", path.display())))
}

#[cfg(not(windows))]
fn on_path(dir: Option<&std::path::Path>) -> bool {
    let (Some(dir), Some(path)) = (dir, std::env::var_os("PATH")) else {
        return false;
    };
    std::env::split_paths(&path).any(|p| p == dir)
}
