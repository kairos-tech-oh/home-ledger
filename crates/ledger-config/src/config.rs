//! What stores are configured, in what order.
//!
//! Everything here is safe to read: bucket names, regions, endpoints, paths.
//! Secrets never appear, and there is no field for one — see [`crate::secrets`].

use ledger_store::StoreKind;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0}")]
    Invalid(String),
    #[error("the configuration file is not readable: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// How to reach one store. Non-secret by construction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Settings {
    Local {
        path: PathBuf,
    },
    /// Also every S3-compatible service; `endpoint` is what selects one.
    S3 {
        bucket: String,
        key: String,
        region: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint: Option<String>,
        /// A profile in `~/.aws/credentials` to sign with. When set, nothing
        /// is kept in the keychain for this store.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        aws_profile: Option<String>,
    },
    /// A file on a mounted share. Same shape as local, different guarantees.
    Nas {
        path: PathBuf,
    },
    /// Google Drive. The client id comes from a Google Cloud OAuth client of
    /// type "Desktop app"; it is not a secret, which is why it lives here.
    GoogleDrive {
        file_name: String,
        client_id: String,
    },
}

impl Settings {
    pub fn store_kind(&self) -> StoreKind {
        match self {
            Settings::Local { .. } => StoreKind::Local,
            Settings::S3 { .. } => StoreKind::S3,
            Settings::Nas { .. } => StoreKind::Nas,
            Settings::GoogleDrive { .. } => StoreKind::GoogleDrive,
        }
    }

    /// Whether this kind keeps a secret in the keychain at all.
    pub fn needs_secret(&self) -> bool {
        match self {
            Settings::S3 { aws_profile, .. } => aws_profile.is_none(),
            Settings::GoogleDrive { .. } => true,
            _ => false,
        }
    }

    /// What is wrong with these settings, if anything.
    pub fn problem(&self) -> Option<String> {
        match self {
            Settings::Local { path } | Settings::Nas { path } => {
                if path.as_os_str().is_empty() {
                    Some("a file path is required".into())
                } else {
                    None
                }
            }
            Settings::S3 {
                bucket,
                key,
                region,
                ..
            } => {
                if bucket.trim().is_empty() {
                    Some("a bucket name is required".into())
                } else if key.trim().is_empty() {
                    Some("an object key is required".into())
                } else if region.trim().is_empty() {
                    Some("a region is required".into())
                } else {
                    None
                }
            }
            Settings::GoogleDrive {
                file_name,
                client_id,
            } => {
                if client_id.trim().is_empty() {
                    Some("a Google OAuth client id is required".into())
                } else if file_name.trim().is_empty() {
                    Some("a file name is required".into())
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreConfig {
    /// Stable, and the name this store's secret is filed under in the
    /// keychain. Never re-used, so removing and re-adding does not inherit an
    /// old key.
    pub id: String,
    /// What this is called on screen.
    pub label: String,
    pub settings: Settings,
    /// Set only when a store that cannot lock reliably was chosen as the
    /// source of truth anyway, after being told.
    #[serde(default)]
    pub accept_risk: bool,
}

// Not Eq: opacity is a float, and comparing appearance settings for exact
// equality is not something anything here needs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_version")]
    pub v: u32,
    /// Ordered. The first is the source of truth; the rest are backups, tried
    /// in order when it cannot be reached.
    #[serde(default)]
    pub stores: Vec<StoreConfig>,
    /// What this machine is called in the audit log. Never the hostname.
    #[serde(default)]
    pub device: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub install: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retirement_target_year: Option<i32>,
    #[serde(default)]
    pub retirement_target_year_seeded: bool,
    /// Whether first-run setup has been through. False means the app offers
    /// the setup flow rather than assuming a deliberate local-only choice.
    #[serde(default)]
    pub setup_complete: bool,
    /// How opaque the window is, 0.30..1.0. An appearance setting for this
    /// machine, so it lives here rather than in the shared ledger.
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// Names offered when a charge is put against a family member. Kept per
    /// machine, as the plugin keeps them, alongside the names already in use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family_members: Vec<String>,
    #[serde(default)]
    pub family_members_seeded: bool,
    /// Set while encryption is on: the id of the data key everything is
    /// sealed under. The key itself is in the keychain, never here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encryption_key_id: Option<String>,
    /// Anything a newer build added, kept so an older one cannot delete it.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

fn default_version() -> u32 {
    CONFIG_VERSION
}

/// The same default the plugin uses: translucent enough to read the wallpaper
/// through, opaque enough to read the figures.
fn default_opacity() -> f64 {
    0.94
}

/// Below this the text stops being legible over a busy wallpaper.
pub const MIN_OPACITY: f64 = 0.30;

impl Default for Config {
    fn default() -> Self {
        Self {
            v: CONFIG_VERSION,
            stores: Vec::new(),
            device: String::new(),
            install: String::new(),
            retirement_target_year: None,
            retirement_target_year_seeded: false,
            setup_complete: false,
            opacity: default_opacity(),
            family_members: Vec::new(),
            family_members_seeded: false,
            encryption_key_id: None,
            unknown: BTreeMap::new(),
        }
    }
}

impl Config {
    /// The store everything is written to.
    pub fn source_of_truth(&self) -> Option<&StoreConfig> {
        self.stores.first()
    }

    /// The stores read from when the source of truth cannot be reached.
    pub fn backups(&self) -> &[StoreConfig] {
        self.stores.get(1..).unwrap_or(&[])
    }

    pub fn find(&self, id: &str) -> Option<&StoreConfig> {
        self.stores.iter().find(|s| s.id == id)
    }

    /// Everything wrong with this configuration, so a settings screen can show
    /// all of it at once rather than one problem per attempt.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();

        let mut seen: Vec<&str> = Vec::new();
        for store in &self.stores {
            if store.id.trim().is_empty() {
                out.push("a store has no id".into());
            } else if seen.contains(&store.id.as_str()) {
                out.push(format!("two stores share the id {}", store.id));
            } else {
                seen.push(&store.id);
            }
            if let Some(problem) = store.settings.problem() {
                out.push(format!("{}: {problem}", store.label));
            }
        }

        // A store that cannot lock reliably must not silently become the one
        // everything is written to.
        if let Some(first) = self.source_of_truth()
            && first.settings.store_kind() == StoreKind::Nas
            && !first.accept_risk
        {
            out.push(format!(
                "{} cannot lock reliably, so it is unsafe as the source of truth",
                first.label
            ));
        }

        out
    }

    /// Clamped rather than refused: an out-of-range figure in a hand-edited
    /// config should not stop the app opening.
    pub fn window_opacity(&self) -> f64 {
        if self.opacity.is_finite() {
            self.opacity.clamp(MIN_OPACITY, 1.0)
        } else {
            default_opacity()
        }
    }

    pub fn is_valid(&self) -> bool {
        self.problems().is_empty()
    }

    /// Move a store to the front, making it the source of truth. Roles change;
    /// no data moves, which is the caller's job.
    pub fn promote(&mut self, id: &str) -> Result<(), ConfigError> {
        let Some(index) = self.stores.iter().position(|s| s.id == id) else {
            return Err(ConfigError::Invalid(format!("no store called {id}")));
        };
        let store = self.stores.remove(index);
        self.stores.insert(0, store);
        Ok(())
    }

    /// Forget a store. The source of truth cannot be removed while it is the
    /// source of truth — there is always exactly one.
    pub fn remove(&mut self, id: &str) -> Result<StoreConfig, ConfigError> {
        let Some(index) = self.stores.iter().position(|s| s.id == id) else {
            return Err(ConfigError::Invalid(format!("no store called {id}")));
        };
        if index == 0 && self.stores.len() > 1 {
            return Err(ConfigError::Invalid(
                "promote another store before removing the source of truth".into(),
            ));
        }
        Ok(self.stores.remove(index))
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ConfigError> {
        serde_json::to_vec_pretty(self).map_err(|e| ConfigError::Corrupt(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ConfigError> {
        serde_json::from_slice(bytes).map_err(|e| ConfigError::Corrupt(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s3(id: &str) -> StoreConfig {
        StoreConfig {
            id: id.into(),
            label: format!("bucket {id}"),
            settings: Settings::S3 {
                bucket: "b".into(),
                key: "ledger/ledger.json".into(),
                region: "us-east-2".into(),
                endpoint: None,
                aws_profile: None,
            },
            accept_risk: false,
        }
    }

    fn nas(id: &str) -> StoreConfig {
        StoreConfig {
            id: id.into(),
            label: "the NAS".into(),
            settings: Settings::Nas {
                path: "/mnt/nas/ledger.json".into(),
            },
            accept_risk: false,
        }
    }

    #[test]
    fn a_fresh_config_has_no_stores_and_has_not_been_set_up() {
        let config = Config::default();
        assert!(config.source_of_truth().is_none());
        assert!(!config.setup_complete);
    }

    #[test]
    fn the_first_store_is_the_source_of_truth_and_the_rest_are_backups() {
        let config = Config {
            stores: vec![s3("a"), s3("b"), s3("c")],
            ..Config::default()
        };

        assert_eq!(config.source_of_truth().unwrap().id, "a");
        assert_eq!(config.backups().len(), 2);
        assert_eq!(config.backups()[0].id, "b");
    }

    #[test]
    fn promoting_a_backup_makes_it_the_source_of_truth() {
        let mut config = Config {
            stores: vec![s3("a"), s3("b")],
            ..Config::default()
        };

        config.promote("b").expect("promoted");
        assert_eq!(config.source_of_truth().unwrap().id, "b");
        assert_eq!(config.backups()[0].id, "a");
    }

    #[test]
    fn the_source_of_truth_cannot_be_removed_while_another_store_exists() {
        // There is always exactly one, so removing it is promotion first.
        let mut config = Config {
            stores: vec![s3("a"), s3("b")],
            ..Config::default()
        };

        assert!(config.remove("a").is_err());
        config.remove("b").expect("a backup can go");
        config.remove("a").expect("the last store can go");
        assert!(config.stores.is_empty());
    }

    #[test]
    fn a_nas_cannot_quietly_become_the_source_of_truth() {
        let mut config = Config {
            stores: vec![nas("n")],
            ..Config::default()
        };
        assert!(!config.is_valid());
        assert!(config.problems()[0].contains("cannot lock reliably"));

        // Allowed once the person has been told and chosen it anyway.
        config.stores[0].accept_risk = true;
        assert!(config.is_valid());
    }

    #[test]
    fn a_nas_is_an_unremarkable_backup() {
        let config = Config {
            stores: vec![s3("a"), nas("n")],
            ..Config::default()
        };
        assert!(config.is_valid(), "{:?}", config.problems());
    }

    #[test]
    fn incomplete_settings_are_reported_all_at_once() {
        let mut config = Config::default();
        let mut broken = s3("a");
        broken.settings = Settings::S3 {
            bucket: String::new(),
            key: "k".into(),
            region: "r".into(),
            endpoint: None,
            aws_profile: None,
        };
        let mut second = s3("b");
        second.settings = Settings::Local {
            path: PathBuf::new(),
        };
        config.stores = vec![broken, second];

        assert_eq!(config.problems().len(), 2);
    }

    #[test]
    fn two_stores_cannot_share_an_id() {
        let config = Config {
            stores: vec![s3("same"), s3("same")],
            ..Config::default()
        };
        assert!(config.problems().iter().any(|p| p.contains("share the id")));
    }

    #[test]
    fn a_setting_a_newer_build_added_survives_a_round_trip() {
        // Same rule as the ledger: an older build must not delete what a newer
        // one wrote.
        let raw = br#"{ "v": 1, "stores": [], "somethingNewer": { "kept": true } }"#;
        let config = Config::from_bytes(raw).expect("parses");
        let out = String::from_utf8(config.to_bytes().unwrap()).unwrap();
        assert!(out.contains("somethingNewer"), "{out}");
    }

    #[test]
    fn there_is_nowhere_in_the_config_to_put_a_secret() {
        // Structural: the settings types have no field for one, so a secret
        // cannot reach this file even by mistake.
        let config = Config {
            stores: vec![s3("a")],
            ..Config::default()
        };
        let out = String::from_utf8(config.to_bytes().unwrap()).unwrap();

        for word in ["secretAccessKey", "accessKeyId", "password", "token"] {
            assert!(!out.contains(word), "config mentions {word}: {out}");
        }
    }

    #[test]
    fn a_bucket_signed_by_an_aws_profile_keeps_nothing_in_the_keychain() {
        let mut store = s3("a");
        assert!(store.settings.needs_secret());

        let Settings::S3 { aws_profile, .. } = &mut store.settings else {
            unreachable!()
        };
        *aws_profile = Some("home-ledger".into());
        assert!(!store.settings.needs_secret());
        assert!(store.settings.problem().is_none());
    }

    #[test]
    fn a_bucket_with_no_profile_writes_no_profile_field_at_all() {
        let store = s3("a");
        let json = serde_json::to_string(&store.settings).expect("serialises");
        assert!(!json.contains("aws_profile"), "{json}");

        let back: Settings = serde_json::from_str(&json).expect("reads back");
        assert_eq!(back, store.settings);
    }
}
