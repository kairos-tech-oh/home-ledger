//! Where credentials live: the operating system's keychain, never a file.
//!
//! Windows Credential Manager, and the Secret Service — GNOME Keyring or
//! KWallet — on Linux. If neither is available the app says so rather than
//! writing a key in the clear, because a silent downgrade to plaintext is the
//! kind of thing nobody finds out about until it matters.

use serde::{Deserialize, Serialize};

/// The service name every entry is filed under.
const SERVICE: &str = "net.kairos.home-ledger";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// No keychain on this machine. The caller must ask what to do rather
    /// than falling back to a file on its own.
    #[error(
        "this system has no keychain available, so there is nowhere safe to \
         keep the credentials"
    )]
    NoKeychain,
    #[error("the keychain refused: {0}")]
    Refused(String),
    #[error("the stored credentials are not readable: {0}")]
    Corrupt(String),
}

/// What a store needs to authenticate. One shape per kind of secret, so
/// adding a provider does not mean stringly-typed blobs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Secret {
    /// S3 and everything that speaks its API.
    AccessKey {
        access_key_id: String,
        secret_access_key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_token: Option<String>,
    },
    /// For the providers that use OAuth. Only the refresh token is kept; an
    /// access token is short-lived and fetched when needed.
    OAuth { refresh_token: String },
    /// A bare key for a service that is not a store, such as a quote feed.
    ApiKey { key: String },
    /// The data key that opens the encrypted ledger, kept so this machine is
    /// not asked for the passphrase again. Named by its id, so a key for an
    /// older ledger is never used on a newer one.
    DataKey {
        key_id: String,
        key: String,
        /// The key wrapped under the passphrase and recovery code. Not secret,
        /// but kept with the key so this machine can seal what it writes.
        envelope: String,
    },
    /// The person's own Plaid keys, for bank connections. `environment` is
    /// "sandbox" or "production".
    PlaidKeys {
        client_id: String,
        secret: String,
        environment: String,
    },
    /// The access token for one connected bank, which with the keys reads
    /// that bank's accounts and transactions.
    BankToken { access_token: String },
}

/// Somewhere credentials can be kept and fetched by store id.
pub trait Secrets: Send + Sync {
    fn get(&self, store_id: &str) -> Result<Option<Secret>, SecretError>;
    fn set(&self, store_id: &str, secret: &Secret) -> Result<(), SecretError>;
    fn forget(&self, store_id: &str) -> Result<(), SecretError>;
}

/// The real thing.
pub struct Keychain {
    service: String,
}

impl Default for Keychain {
    fn default() -> Self {
        Self::new()
    }
}

impl Keychain {
    pub fn new() -> Self {
        Self {
            service: SERVICE.to_string(),
        }
    }

    /// Whether a keychain is actually usable here, which is not the same as
    /// the library being compiled in. Checked by writing and removing a
    /// throwaway entry, because on Linux the Secret Service can be present but
    /// locked or unavailable over D-Bus.
    pub fn available(&self) -> bool {
        let probe = Secret::OAuth {
            refresh_token: "probe".into(),
        };
        match self.set("__probe__", &probe) {
            Ok(()) => {
                let _ = self.forget("__probe__");
                true
            }
            Err(_) => false,
        }
    }

    fn entry(&self, store_id: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, store_id).map_err(map_error)
    }
}

fn map_error(error: keyring::Error) -> SecretError {
    match error {
        keyring::Error::NoStorageAccess(e) => SecretError::Refused(e.to_string()),
        keyring::Error::PlatformFailure(e) => SecretError::NoKeychain.tagged(e.to_string()),
        other => SecretError::Refused(other.to_string()),
    }
}

impl SecretError {
    /// Keep the underlying reason without losing which variant it is.
    fn tagged(self, detail: String) -> Self {
        tracing::debug!(%detail, "keychain unavailable");
        self
    }
}

impl Secrets for Keychain {
    fn get(&self, store_id: &str) -> Result<Option<Secret>, SecretError> {
        match self.entry(store_id)?.get_password() {
            Ok(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|e| SecretError::Corrupt(e.to_string())),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_error(e)),
        }
    }

    fn set(&self, store_id: &str, secret: &Secret) -> Result<(), SecretError> {
        let raw = serde_json::to_string(secret).map_err(|e| SecretError::Corrupt(e.to_string()))?;
        self.entry(store_id)?.set_password(&raw).map_err(map_error)
    }

    fn forget(&self, store_id: &str) -> Result<(), SecretError> {
        match self.entry(store_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_error(e)),
        }
    }
}

/// For tests, and for a session that has declined to store anything. Holds
/// secrets in memory only, so they are gone when the app closes.
#[derive(Default)]
pub struct InMemory {
    held: std::sync::Mutex<std::collections::BTreeMap<String, Secret>>,
}

impl Secrets for InMemory {
    fn get(&self, store_id: &str) -> Result<Option<Secret>, SecretError> {
        Ok(self
            .held
            .lock()
            .expect("not poisoned")
            .get(store_id)
            .cloned())
    }

    fn set(&self, store_id: &str, secret: &Secret) -> Result<(), SecretError> {
        self.held
            .lock()
            .expect("not poisoned")
            .insert(store_id.to_string(), secret.clone());
        Ok(())
    }

    fn forget(&self, store_id: &str) -> Result<(), SecretError> {
        self.held.lock().expect("not poisoned").remove(store_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Secret {
        Secret::AccessKey {
            access_key_id: "AKIAEXAMPLE".into(),
            secret_access_key: "verysecret".into(),
            session_token: None,
        }
    }

    #[test]
    fn a_secret_round_trips() {
        let store = InMemory::default();
        store.set("s3", &key()).unwrap();
        assert_eq!(store.get("s3").unwrap(), Some(key()));
    }

    #[test]
    fn a_store_with_no_secret_reads_as_nothing_rather_than_failing() {
        let store = InMemory::default();
        assert_eq!(store.get("never-set").unwrap(), None);
    }

    #[test]
    fn forgetting_is_idempotent() {
        let store = InMemory::default();
        store.set("s3", &key()).unwrap();
        store.forget("s3").unwrap();
        store.forget("s3").unwrap();
        assert_eq!(store.get("s3").unwrap(), None);
    }

    #[test]
    fn secrets_are_kept_apart_by_store() {
        let store = InMemory::default();
        store.set("a", &key()).unwrap();
        assert_eq!(store.get("b").unwrap(), None);
    }

    #[test]
    fn an_oauth_secret_keeps_only_the_refresh_token() {
        let secret = Secret::OAuth {
            refresh_token: "refresh".into(),
        };
        let raw = serde_json::to_string(&secret).unwrap();
        assert!(raw.contains("refresh"));
        assert!(!raw.contains("access_token"));
    }

    #[test]
    fn a_missing_keychain_is_its_own_error_rather_than_a_silent_fallback() {
        // The caller has to decide what to do; it must not become a plaintext
        // file without anyone saying so.
        let error = SecretError::NoKeychain;
        assert!(error.to_string().contains("nowhere safe"));
    }
}
