//! Credentials from `~/.aws/credentials`, so a machine that already has a
//! profile does not need a second copy of the same secret in its keychain.

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("no AWS credentials file at {0}")]
    Missing(PathBuf),
    #[error("cannot read {path}: {why}")]
    Unreadable { path: PathBuf, why: String },
    #[error("{0} is readable by other users; run chmod 600 on it")]
    TooOpen(PathBuf),
    #[error("no profile named [{0}] in the AWS credentials file")]
    NoProfile(String),
    #[error("the [{0}] profile has no aws_access_key_id and aws_secret_access_key")]
    Incomplete(String),
}

/// What one profile holds. Never logged and never sent to the interface.
pub struct Found {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

pub fn credentials_path() -> PathBuf {
    if let Some(set) = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE") {
        return PathBuf::from(set);
    }
    home().join(".aws").join("credentials")
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// Read one profile. The file is checked for being private first: a secret
/// every user on the machine can read is a secret that has already leaked.
pub fn load(profile: &str) -> Result<Found, ProfileError> {
    let path = credentials_path();
    if !path.exists() {
        return Err(ProfileError::Missing(path));
    }
    guard(&path)?;

    let body = std::fs::read_to_string(&path).map_err(|e| ProfileError::Unreadable {
        path: path.clone(),
        why: e.to_string(),
    })?;
    parse(&body, profile)
}

#[cfg(unix)]
fn guard(path: &Path) -> Result<(), ProfileError> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).map_err(|e| ProfileError::Unreadable {
        path: path.to_path_buf(),
        why: e.to_string(),
    })?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(ProfileError::Unreadable {
            path: path.to_path_buf(),
            why: "not a regular file".into(),
        });
    }
    if meta.mode() & 0o077 != 0 {
        return Err(ProfileError::TooOpen(path.to_path_buf()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn guard(_path: &Path) -> Result<(), ProfileError> {
    Ok(())
}

fn parse(body: &str, profile: &str) -> Result<Found, ProfileError> {
    let mut inside = false;
    let mut id = None;
    let mut secret = None;
    let mut token = None;

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            // A profile in the shared config file may be written "profile foo".
            let name = name.trim().strip_prefix("profile ").unwrap_or(name.trim());
            inside = name == profile;
            continue;
        }
        if !inside {
            continue;
        }
        let Some((field, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().to_string();
        match field.trim() {
            "aws_access_key_id" => id = Some(value),
            "aws_secret_access_key" => secret = Some(value),
            "aws_session_token" => token = Some(value),
            _ => {}
        }
    }

    match (id, secret) {
        (Some(access_key_id), Some(secret_access_key))
            if !access_key_id.is_empty() && !secret_access_key.is_empty() =>
        {
            Ok(Found {
                access_key_id,
                secret_access_key,
                session_token: token.filter(|t| !t.is_empty()),
            })
        }
        (None, None) => Err(ProfileError::NoProfile(profile.into())),
        _ => Err(ProfileError::Incomplete(profile.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
[default]
aws_access_key_id = DEFAULTID
aws_secret_access_key = defaultsecret

# a comment
[home-ledger]
aws_access_key_id = LEDGERID
aws_secret_access_key = ledgersecret
aws_session_token = tok

[profile spaced]
aws_access_key_id = SPACEDID
aws_secret_access_key = spacedsecret

[half]
aws_access_key_id = ONLYID
";

    #[test]
    fn a_named_profile_is_read_rather_than_the_first_one_in_the_file() {
        let found = parse(FILE, "home-ledger").expect("reads");
        assert_eq!(found.access_key_id, "LEDGERID");
        assert_eq!(found.secret_access_key, "ledgersecret");
        assert_eq!(found.session_token.as_deref(), Some("tok"));
    }

    #[test]
    fn a_profile_with_no_session_token_reads_as_none_rather_than_empty() {
        assert!(
            parse(FILE, "default")
                .expect("reads")
                .session_token
                .is_none()
        );
    }

    #[test]
    fn the_shared_config_spelling_of_a_profile_header_is_understood() {
        assert_eq!(
            parse(FILE, "spaced").expect("reads").access_key_id,
            "SPACEDID"
        );
    }

    #[test]
    fn half_a_profile_is_an_error_rather_than_an_unsigned_request() {
        assert!(matches!(
            parse(FILE, "half"),
            Err(ProfileError::Incomplete(_))
        ));
        assert!(matches!(
            parse(FILE, "absent"),
            Err(ProfileError::NoProfile(_))
        ));
    }
}
