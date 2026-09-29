//! What is configured, where the secrets are, and how the two become a
//! running store engine.

pub mod aws;
pub mod config;
pub mod secrets;

pub use config::{Config, ConfigError, Settings, StoreConfig};
pub use secrets::{Keychain, Secret, SecretError, Secrets};

use directories::ProjectDirs;
use ledger_store::{
    Credentials, Document, DriveConfig, DriveStore, Engine, LocalStore, Outbox, S3Config, S3Store,
    Store,
};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("no store is configured yet")]
    NotConfigured,
    #[error("{0}")]
    Invalid(String),
    #[error("{label} needs credentials, and none are stored for it")]
    MissingSecret { label: String },
    #[error("{label} was given credentials of the wrong kind")]
    WrongSecret { label: String },
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Secret(#[from] SecretError),
    #[error("{label}: {source}")]
    Profile {
        label: String,
        #[source]
        source: aws::ProfileError,
    },
    #[error(transparent)]
    Store(#[from] ledger_store::StoreError),
    #[error(transparent)]
    Engine(#[from] ledger_store::EngineError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Where the app keeps its own files.
pub struct Places {
    pub config_file: PathBuf,
    pub data_dir: PathBuf,
}

impl Places {
    pub fn discover() -> Result<Self, SetupError> {
        let dirs = ProjectDirs::from("net", "kairos", "home-ledger")
            .ok_or_else(|| SetupError::Invalid("no home directory to store data in".into()))?;
        Ok(Self {
            config_file: dirs.config_dir().join("config.json"),
            data_dir: dirs.data_dir().to_path_buf(),
        })
    }

    /// The local file a fresh install uses until something else is set up.
    pub fn default_local_ledger(&self) -> PathBuf {
        self.data_dir.join("ledger.json")
    }

    pub fn outbox(&self) -> PathBuf {
        self.data_dir.join("outbox.json")
    }

    pub fn load_config(&self) -> Result<Config, SetupError> {
        match std::fs::read(&self.config_file) {
            Ok(bytes) => Ok(Config::from_bytes(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(SetupError::Io(e)),
        }
    }

    /// Write via a temporary file and rename, so a crash mid-write leaves the
    /// previous configuration rather than half of a new one.
    pub fn save_config(&self, config: &Config) -> Result<(), SetupError> {
        if let Some(dir) = self.config_file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temp = self.config_file.with_extension("tmp");
        std::fs::write(&temp, config.to_bytes()?)?;
        std::fs::rename(&temp, &self.config_file)?;
        Ok(())
    }
}

/// Build one store from its configuration and whatever secret it needs.
pub fn build_store(
    store: &StoreConfig,
    secrets: &dyn Secrets,
) -> Result<Arc<dyn Store>, SetupError> {
    if let Some(problem) = store.settings.problem() {
        return Err(SetupError::Invalid(format!("{}: {problem}", store.label)));
    }

    match &store.settings {
        config::Settings::Local { path } => Ok(Arc::new(LocalStore::new(&store.label, path))),
        config::Settings::Nas { path } => Ok(Arc::new(LocalStore::nas(&store.label, path))),
        config::Settings::GoogleDrive {
            file_name,
            client_id,
        } => {
            let secret = secrets
                .get(&store.id)?
                .ok_or_else(|| SetupError::MissingSecret {
                    label: store.label.clone(),
                })?;
            let Secret::OAuth { refresh_token } = secret else {
                return Err(SetupError::WrongSecret {
                    label: store.label.clone(),
                });
            };
            Ok(Arc::new(DriveStore::new(
                &store.label,
                DriveConfig {
                    file_name: file_name.clone(),
                    client_id: client_id.clone(),
                },
                refresh_token,
            )?))
        }
        config::Settings::S3 {
            bucket,
            key,
            region,
            endpoint,
            aws_profile,
        } => {
            let (access_key_id, secret_access_key, session_token) = match aws_profile {
                Some(profile) => {
                    let found = aws::load(profile).map_err(|source| SetupError::Profile {
                        label: store.label.clone(),
                        source,
                    })?;
                    (
                        found.access_key_id,
                        found.secret_access_key,
                        found.session_token,
                    )
                }
                None => {
                    let secret =
                        secrets
                            .get(&store.id)?
                            .ok_or_else(|| SetupError::MissingSecret {
                                label: store.label.clone(),
                            })?;
                    let Secret::AccessKey {
                        access_key_id,
                        secret_access_key,
                        session_token,
                    } = secret
                    else {
                        return Err(SetupError::WrongSecret {
                            label: store.label.clone(),
                        });
                    };
                    (access_key_id, secret_access_key, session_token)
                }
            };

            Ok(Arc::new(S3Store::new(
                &store.label,
                S3Config {
                    bucket: bucket.clone(),
                    key: key.clone(),
                    region: region.clone(),
                    endpoint: endpoint.clone(),
                },
                Credentials {
                    access_key_id,
                    secret_access_key,
                    session_token,
                },
            )?))
        }
    }
}

/// Build the whole engine: the source of truth, its backups, and the outbox.
pub fn build_engine(
    config: &Config,
    places: &Places,
    secrets: &dyn Secrets,
    document: Arc<dyn Document>,
) -> Result<Engine, SetupError> {
    let problems = config.problems();
    if !problems.is_empty() {
        return Err(SetupError::Invalid(problems.join("; ")));
    }
    let Some(first) = config.source_of_truth() else {
        return Err(SetupError::NotConfigured);
    };

    let primary = build_store(first, secrets)?;
    let mut mirrors = Vec::new();
    for backup in config.backups() {
        // One unreachable backup must not stop the app starting; it shows as
        // unhealthy in settings instead.
        match build_store(backup, secrets) {
            Ok(store) => mirrors.push(store),
            Err(e) => tracing::warn!(store = %backup.label, error = %e, "backup not configured"),
        }
    }

    std::fs::create_dir_all(&places.data_dir)?;
    let outbox = Outbox::new(places.outbox());

    Ok(Engine::new(
        primary,
        mirrors,
        outbox,
        document,
        first.accept_risk,
    )?)
}

/// The configuration a fresh install starts with: one local file, and setup
/// not yet done, so the app knows to offer the choice rather than assume it.
pub fn plugin_file(name: &str) -> Option<PathBuf> {
    let dirs = directories::BaseDirs::new()?;
    Some(dirs.state_dir()?.join("kairos.home-ledger").join(name))
}

pub fn plugin_target_year() -> Option<i32> {
    let path = plugin_file("prefs.json")?;
    let prefs: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    let year = prefs.get("retirementTargetYear")?.as_f64()?;
    (year.is_finite() && year > 0.0).then_some(year as i32)
}

/// The family names the plugin kept in its own preferences, if it ran here.
pub fn plugin_family_members() -> Vec<String> {
    let Some(path) = plugin_file("prefs.json") else {
        return Vec::new();
    };
    let Some(prefs) = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    else {
        return Vec::new();
    };
    let names: Vec<String> = prefs
        .get("familyMembers")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    clean_family_members(&names)
}

/// At most this many names, as the plugin allows.
pub const FAMILY_MEMBERS_MAX: usize = 50;
/// The ledger's own limit on a name.
const NAME_MAX: usize = 120;

/// Trimmed, one of each ignoring case, the first spelling kept, in the order
/// given. Blank names are dropped.
pub fn clean_family_members(names: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    names
        .iter()
        .map(|n| {
            let one_line: String = n.chars().filter(|c| !c.is_control()).collect();
            let name = one_line.split_whitespace().collect::<Vec<_>>().join(" ");
            name.chars().take(NAME_MAX).collect::<String>()
        })
        .filter(|n| !n.is_empty() && seen.insert(n.to_lowercase()))
        .take(FAMILY_MEMBERS_MAX)
        .collect()
}

pub fn starting_config(places: &Places, device: &str) -> Config {
    Config {
        stores: vec![StoreConfig {
            id: "local".into(),
            label: "This computer".into(),
            settings: config::Settings::Local {
                path: places.default_local_ledger(),
            },
            accept_risk: false,
        }],
        device: device.to_string(),
        setup_complete: false,
        ..Config::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_store::{Cas, PendingOp, Relation, StoreKind};

    #[test]
    fn family_names_are_one_of_each_in_the_order_given() {
        // check-spending.py: ["Chris", "Pat", "Chris", "", 1] keeps Chris and Pat.
        let names: Vec<String> = ["Chris", " Pat ", "chris", "", "  "]
            .map(String::from)
            .to_vec();
        assert_eq!(clean_family_members(&names), ["Chris", "Pat"]);
        let many: Vec<String> = (0..80).map(|i| format!("Name {i}")).collect();
        assert_eq!(clean_family_members(&many).len(), FAMILY_MEMBERS_MAX);
    }

    struct NoDocument;
    impl Document for NoDocument {
        fn replay(&self, _: Option<&[u8]>, _: &[PendingOp]) -> Result<Vec<u8>, String> {
            Ok(Vec::new())
        }
        fn stamp(&self, document: &[u8], _: &str) -> Result<Vec<u8>, String> {
            Ok(document.to_vec())
        }
        fn relation(&self, _: &[u8], _: &[u8]) -> Relation {
            Relation::TooFarApart
        }
    }

    /// `unwrap_err` needs Debug on the success type, and neither a boxed
    /// Store nor the Engine has it.
    fn error_from<T>(result: Result<T, SetupError>) -> SetupError {
        match result {
            Err(e) => e,
            Ok(_) => panic!("expected this to be refused"),
        }
    }

    fn places(dir: &tempfile::TempDir) -> Places {
        Places {
            config_file: dir.path().join("config.json"),
            data_dir: dir.path().join("data"),
        }
    }

    fn s3_store(id: &str) -> StoreConfig {
        StoreConfig {
            id: id.into(),
            label: "My bucket".into(),
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

    fn access_key() -> Secret {
        Secret::AccessKey {
            access_key_id: "AKIAEXAMPLE".into(),
            secret_access_key: "verysecret".into(),
            session_token: None,
        }
    }

    #[test]
    fn a_missing_config_file_reads_as_a_fresh_one() {
        let dir = tempfile::tempdir().unwrap();
        let config = places(&dir).load_config().expect("loaded");
        assert!(config.stores.is_empty());
        assert!(!config.setup_complete);
    }

    #[test]
    fn config_survives_being_written_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let places = places(&dir);

        let mut config = starting_config(&places, "Linux PC");
        config.stores.push(s3_store("bucket"));
        places.save_config(&config).expect("saved");

        assert_eq!(places.load_config().unwrap(), config);
    }

    #[test]
    fn a_fresh_install_starts_local_and_unconfigured() {
        let dir = tempfile::tempdir().unwrap();
        let places = places(&dir);
        let config = starting_config(&places, "Windows PC");

        assert_eq!(config.stores.len(), 1);
        assert_eq!(
            config.source_of_truth().unwrap().settings.store_kind(),
            StoreKind::Local
        );
        assert!(!config.setup_complete, "must still offer the choice");
        assert!(config.is_valid());
    }

    #[test]
    fn an_s3_store_without_credentials_says_so_by_name() {
        let secrets = secrets::InMemory::default();
        let error = error_from(build_store(&s3_store("bucket"), &secrets));
        assert!(matches!(error, SetupError::MissingSecret { .. }));
        assert!(error.to_string().contains("My bucket"));
    }

    #[test]
    fn an_s3_store_with_the_wrong_kind_of_secret_is_refused() {
        let secrets = secrets::InMemory::default();
        secrets
            .set(
                "bucket",
                &Secret::OAuth {
                    refresh_token: "nope".into(),
                },
            )
            .unwrap();
        let error = error_from(build_store(&s3_store("bucket"), &secrets));
        assert!(matches!(error, SetupError::WrongSecret { .. }));
    }

    #[test]
    fn an_s3_store_builds_once_its_key_is_stored() {
        let secrets = secrets::InMemory::default();
        secrets.set("bucket", &access_key()).unwrap();

        let store = build_store(&s3_store("bucket"), &secrets).expect("built");
        assert_eq!(store.kind(), StoreKind::S3);
        assert_eq!(store.capabilities().cas, Cas::Native);
    }

    #[test]
    fn the_engine_is_built_from_the_configured_order() {
        let dir = tempfile::tempdir().unwrap();
        let places = places(&dir);
        let secrets = secrets::InMemory::default();
        secrets.set("bucket", &access_key()).unwrap();

        let mut config = starting_config(&places, "Linux PC");
        config.stores.insert(0, s3_store("bucket"));

        let engine = build_engine(&config, &places, &secrets, Arc::new(NoDocument)).expect("built");
        assert_eq!(engine.primary().kind(), StoreKind::S3);
    }

    #[tokio::test]
    async fn a_backup_that_cannot_be_built_does_not_stop_the_app_starting() {
        // An unreachable or unconfigured backup shows as unhealthy in
        // settings; it is not a reason to refuse to open the ledger.
        let dir = tempfile::tempdir().unwrap();
        let places = places(&dir);
        let secrets = secrets::InMemory::default();

        let mut config = starting_config(&places, "Linux PC");
        config.stores.push(s3_store("no-key-for-this"));

        let engine = build_engine(&config, &places, &secrets, Arc::new(NoDocument)).expect("built");
        assert_eq!(engine.primary().kind(), StoreKind::Local);
        assert_eq!(
            engine.status().await.len(),
            1,
            "the broken backup was left out"
        );
    }

    #[test]
    fn nothing_is_built_from_an_invalid_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let places = places(&dir);
        let secrets = secrets::InMemory::default();

        let config = Config {
            stores: vec![StoreConfig {
                id: "n".into(),
                label: "the NAS".into(),
                settings: Settings::Nas {
                    path: "/mnt/nas/ledger.json".into(),
                },
                accept_risk: false,
            }],
            ..Config::default()
        };

        let error = error_from(build_engine(
            &config,
            &places,
            &secrets,
            Arc::new(NoDocument),
        ));
        assert!(error.to_string().contains("cannot lock reliably"));
    }

    #[test]
    fn an_empty_configuration_says_so_rather_than_guessing() {
        let dir = tempfile::tempdir().unwrap();
        let places = places(&dir);
        let secrets = secrets::InMemory::default();

        let error = error_from(build_engine(
            &Config::default(),
            &places,
            &secrets,
            Arc::new(NoDocument),
        ));
        assert!(matches!(error, SetupError::NotConfigured));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// A real bucket, named by the environment so no one's bucket lives here:
    /// `LEDGER_LIVE_BUCKET`, `LEDGER_LIVE_REGION` and `LEDGER_LIVE_PROFILE`.
    fn live(name: &str) -> String {
        std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run the live tests"))
    }

    #[tokio::test]
    #[ignore = "reads a real bucket named by LEDGER_LIVE_BUCKET, LEDGER_LIVE_REGION and LEDGER_LIVE_PROFILE"]
    async fn the_real_bucket_lists_and_reads_its_history_shelf() {
        let store = StoreConfig {
            id: "live".into(),
            label: "live".into(),
            settings: config::Settings::S3 {
                bucket: live("LEDGER_LIVE_BUCKET"),
                key: "ledger/ledger.json".into(),
                region: live("LEDGER_LIVE_REGION"),
                endpoint: None,
                aws_profile: Some(live("LEDGER_LIVE_PROFILE")),
            },
            accept_risk: false,
        };
        let built = build_store(&store, &secrets::InMemory::default()).unwrap();
        let shelf = built.shelf("history").unwrap();
        let names = shelf.names().await.unwrap();
        println!("history objects: {names:?}");
        let missing = shelf.slot("0000-no-such-install").unwrap().load().await;
        assert!(matches!(missing, Ok(None)), "{missing:?}");
    }
}
