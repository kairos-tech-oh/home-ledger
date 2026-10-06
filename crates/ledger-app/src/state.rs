//! What the app holds while it runs, and where it keeps its files.
//!
//! The engine is rebuilt whenever the configuration changes, so it lives
//! behind a lock rather than being fixed at startup: changing where the
//! ledger lives is an ordinary thing to do, not a restart.

use ledger_config::{Config, Keychain, Places, Secrets, SetupError, build_engine, starting_config};
use ledger_store::{Engine, Vault};
use ledger_writer::Writer;
use std::sync::Arc;
use tokio::sync::{RwLock, RwLockReadGuard};

/// Everything that is replaced together when the configuration changes.
pub struct Live {
    pub engine: Engine,
    pub writer: Arc<Writer>,
    pub config: Config,
}

pub struct AppState {
    live: RwLock<Live>,
    pub places: Places,
    pub secrets: Arc<dyn Secrets>,
    /// False when no keychain could be reached. Credentials still work for
    /// this session, but nothing is kept, and the UI has to say so rather
    /// than letting someone believe a key was saved.
    pub keychain_available: bool,
    /// Seals and opens everything stored. One for the life of the app, so a
    /// reconfiguration keeps the unlock.
    pub vault: Arc<Vault>,
    /// Which program this is, stamped on every change it makes.
    pub client: Client,
    background: std::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

/// Which program is making changes, for the change history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Client {
    Desktop,
    /// The `hl` command line, with the label a script gave itself, if any.
    Cli {
        via: String,
    },
    Mobile,
}

impl Client {
    pub fn name(&self) -> &'static str {
        match self {
            Client::Desktop => "desktop",
            Client::Cli { .. } => "cli",
            Client::Mobile => "mobile",
        }
    }

    fn via(&self) -> String {
        match self {
            Client::Cli { via } => ledger_domain::plain(via, 60),
            _ => String::new(),
        }
    }
}

/// Where the data key is kept in the keychain.
pub const DATA_KEY: &str = "data-key";

/// The key this machine kept for the ledger sealed under `key_id`, if any.
pub fn kept_key(secrets: &dyn Secrets, key_id: &str) -> Option<ledger_store::sealed::Key> {
    match secrets.get(DATA_KEY).ok().flatten()? {
        ledger_config::Secret::DataKey {
            key_id: kept,
            key,
            envelope,
        } if kept == key_id => {
            let envelope = serde_json::from_str(&envelope).ok()?;
            ledger_store::sealed::Key::import(&envelope, &key)
        }
        _ => None,
    }
}

impl AppState {
    /// Waits for the work handed to the background, such as publishing
    /// history after an edit. The desktop app never needs to; the command
    /// line does before it exits, so a script never quits mid-upload.
    pub async fn finish_background(&self) {
        let handles: Vec<_> = std::mem::take(&mut *self.background.lock().unwrap());
        for handle in handles {
            let _ = handle.await;
        }
    }

    /// Runs work in the background, kept so [`Self::finish_background`] can
    /// wait for it.
    pub fn spawn(&self, work: impl std::future::Future<Output = ()> + Send + 'static) {
        let handle = tokio::spawn(work);
        let mut held = self.background.lock().unwrap();
        held.retain(|h| !h.is_finished());
        held.push(handle);
    }

    /// The installed app's own setup, as the given client.
    pub fn headless(client: Client) -> Result<Self, Box<dyn std::error::Error>> {
        let keychain = Keychain::new();
        let keychain_available = keychain.available();
        if !keychain_available {
            tracing::warn!("no keychain on this system; credentials will not be kept");
        }
        let secrets: Arc<dyn Secrets> = if keychain_available {
            Arc::new(keychain)
        } else {
            Arc::new(ledger_config::secrets::InMemory::default())
        };
        Self::open_as(
            client,
            Places::discover()?,
            secrets,
            keychain_available,
            ledger_config::plugin_target_year,
        )
    }

    /// As the desktop app: what the tests use.
    pub fn open(
        places: Places,
        secrets: Arc<dyn Secrets>,
        keychain_available: bool,
        plugin_year: impl FnOnce() -> Option<i32>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_as(
            Client::Desktop,
            places,
            secrets,
            keychain_available,
            plugin_year,
        )
    }

    pub fn open_as(
        client: Client,
        places: Places,
        secrets: Arc<dyn Secrets>,
        keychain_available: bool,
        plugin_year: impl FnOnce() -> Option<i32>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(&places.data_dir)?;

        let mut config = places.load_config()?;
        if config.stores.is_empty() {
            config = starting_config(&places, &device_name());
            places.save_config(&config)?;
        }
        config.device = machine_name(&config.device).unwrap_or_else(device_name);
        if !ledger_store::shelf_name_ok(&config.install) {
            config.install = uuid::Uuid::new_v4().simple().to_string();
            places.save_config(&config)?;
        }

        if !config.family_members_seeded {
            config.family_members = ledger_config::plugin_family_members();
            config.family_members_seeded = true;
            places.save_config(&config)?;
        }

        if !config.retirement_target_year_seeded {
            let this_year = crate::clock::year_and_month().0;
            config.retirement_target_year = target_year(plugin_year(), this_year);
            config.retirement_target_year_seeded = true;
            places.save_config(&config)?;
        }

        // Encryption on: sealed from the start, with the kept key if there is
        // one. Without it everything stays locked until the passphrase is given.
        let vault = Vault::new();
        if let Some(id) = &config.encryption_key_id {
            vault.set(true, kept_key(secrets.as_ref(), id));
        }

        let live = build_live(&config, &places, secrets.as_ref(), &vault, &client)?;
        tracing::info!(
            dir = %places.data_dir.display(),
            stores = config.stores.len(),
            "ledger ready"
        );

        Ok(Self {
            live: RwLock::new(live),
            places,
            secrets,
            keychain_available,
            vault,
            client,
            background: std::sync::Mutex::new(Vec::new()),
        })
    }

    pub fn audit(&self) -> crate::audit::Audit {
        crate::audit::Audit::sealed(&self.places.data_dir, self.vault.clone())
    }

    pub fn points(&self) -> crate::snapshots::LocalPoints {
        crate::snapshots::LocalPoints::sealed(&self.places.data_dir, self.vault.clone())
    }

    pub async fn live(&self) -> RwLockReadGuard<'_, Live> {
        self.live.read().await
    }

    pub async fn primary_and_install(&self) -> (Arc<dyn ledger_store::Store>, String) {
        let live = self.live.read().await;
        (live.engine.primary().clone(), live.config.install.clone())
    }

    pub async fn share_history(&self) {
        let (primary, install) = self.primary_and_install().await;
        crate::audit::share(
            primary,
            install,
            self.places.data_dir.clone(),
            self.vault.clone(),
        )
        .await;
    }

    pub async fn retirement_target_year(&self) -> Option<i32> {
        self.live.read().await.config.retirement_target_year
    }

    pub async fn set_retirement_target_year(&self, year: Option<i32>) -> Result<(), SetupError> {
        let mut config = self.config().await;
        config.retirement_target_year = year;
        self.reconfigure(config).await
    }

    pub async fn family_members(&self) -> Vec<String> {
        self.live.read().await.config.family_members.clone()
    }

    pub async fn set_family_members(&self, names: &[String]) -> Result<Vec<String>, SetupError> {
        let mut config = self.config().await;
        config.family_members = ledger_config::clean_family_members(names);
        let kept = config.family_members.clone();
        self.reconfigure(config).await?;
        Ok(kept)
    }

    pub async fn config(&self) -> Config {
        self.live.read().await.config.clone()
    }

    /// Adopt a new configuration: validate it, build from it, and only then
    /// write it to disk and swap it in.
    ///
    /// Ordered that way on purpose. A configuration that cannot be built must
    /// not be saved, or the app fails to start next time and the person has to
    /// edit JSON to recover.
    pub async fn reconfigure(&self, config: Config) -> Result<(), SetupError> {
        let rebuilt = build_live(
            &config,
            &self.places,
            self.secrets.as_ref(),
            &self.vault,
            &self.client,
        )?;
        self.places.save_config(&config)?;
        *self.live.write().await = rebuilt;
        tracing::info!(stores = config.stores.len(), "configuration replaced");
        Ok(())
    }
}

fn build_live(
    config: &Config,
    places: &Places,
    secrets: &dyn Secrets,
    vault: &Arc<Vault>,
    client: &Client,
) -> Result<Live, SetupError> {
    let writer = Arc::new(Writer::attributed(ledger_writer::Attribution {
        device: config.device.clone(),
        client: client.name().into(),
        install: config.install.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
        via: client.via(),
    }));
    let engine = build_engine(config, places, secrets, writer.clone(), vault.clone())?;
    Ok(Live {
        engine,
        writer,
        config: config.clone(),
    })
}

pub fn target_year(year: Option<i32>, this_year: i32) -> Option<i32> {
    year.filter(|y| (this_year..=this_year + 70).contains(y))
}

pub const MACHINE_NAME_MAX: usize = 60;

pub fn machine_name(raw: &str) -> Option<String> {
    let name = ledger_domain::plain(raw, MACHINE_NAME_MAX);
    (!name.is_empty()).then_some(name)
}

/// Names the machine in the audit log.
///
/// Deliberately not the hostname: machine names very often contain a person's
/// real name, and this value is written into the document and synced to
/// whatever store is configured. A neutral default is renamed in settings.
fn device_name() -> String {
    if cfg!(target_os = "windows") {
        "Windows PC".into()
    } else if cfg!(target_os = "macos") {
        "Mac".into()
    } else {
        "Linux PC".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_name_never_comes_from_the_environment() {
        // A hostname commonly carries someone's real name, and this value is
        // written into the ledger and synced off the machine.
        unsafe {
            std::env::set_var("COMPUTERNAME", "janedoe-pc");
            std::env::set_var("HOSTNAME", "janedoe");
        }
        let name = device_name();
        assert!(
            !name.contains("janedoe"),
            "device name leaked the hostname: {name}"
        );
    }

    #[test]
    fn a_target_year_is_kept_only_within_seventy_years_from_now() {
        assert_eq!(target_year(Some(2026), 2026), Some(2026));
        assert_eq!(target_year(Some(2096), 2026), Some(2096));
        assert_eq!(target_year(Some(2097), 2026), None);
        assert_eq!(target_year(Some(2025), 2026), None);
        assert_eq!(target_year(None, 2026), None);
    }

    #[test]
    fn a_blank_machine_name_is_refused() {
        for raw in ["", "   ", "\t\n", "\u{7}"] {
            assert_eq!(machine_name(raw), None, "{raw:?}");
        }
    }

    #[test]
    fn a_machine_name_is_trimmed_and_capped() {
        assert_eq!(machine_name("  Laptop  ").as_deref(), Some("Laptop"));
        let long = machine_name(&"x".repeat(200)).unwrap();
        assert_eq!(long.chars().count(), MACHINE_NAME_MAX);
    }

    #[test]
    fn a_renamed_machine_signs_the_next_edit() {
        let dir = tempfile::tempdir().unwrap();
        let places = Places {
            config_file: dir.path().join("config.json"),
            data_dir: dir.path().join("data"),
        };
        let secrets = ledger_config::secrets::InMemory::default();
        let mut config = starting_config(&places, &device_name());
        let before =
            build_live(&config, &places, &secrets, &Vault::new(), &Client::Desktop).unwrap();
        config.device = machine_name(" Laptop ").unwrap();
        let after =
            build_live(&config, &places, &secrets, &Vault::new(), &Client::Desktop).unwrap();

        let op = ledger_writer::Op::TypeAdd {
            list: ledger_writer::TypeList::Budget,
            name: "Pets".into(),
        };
        let mut ledger = ledger_domain::Ledger::default();
        let first = before.writer.apply(&mut ledger.clone(), &op).unwrap();
        let second = after.writer.apply(&mut ledger, &op).unwrap();
        assert_eq!(first.actor, device_name());
        assert_eq!(second.actor, "Laptop");
    }

    #[tokio::test]
    async fn the_target_year_is_seeded_from_the_plugin_once() {
        let root = tempfile::tempdir().unwrap();
        let places = || Places {
            config_file: root.path().join("config.json"),
            data_dir: root.path().join("data"),
        };
        let secrets = || -> Arc<dyn ledger_config::Secrets> {
            Arc::new(ledger_config::secrets::InMemory::default())
        };
        let year = crate::clock::year_and_month().0 + 20;

        let first = AppState::open(places(), secrets(), false, || Some(year)).unwrap();
        assert_eq!(first.retirement_target_year().await, Some(year));
        first.set_retirement_target_year(None).await.unwrap();

        let again = AppState::open(places(), secrets(), false, || Some(year)).unwrap();
        assert_eq!(again.retirement_target_year().await, None);
    }
}
