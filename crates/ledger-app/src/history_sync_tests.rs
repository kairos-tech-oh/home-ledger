use crate::audit::{self, Audit, Publish};
use crate::commands::{apply_value, sync};
use crate::state::AppState;
use crate::views::history_of;
use ledger_config::{Config, Places, Settings, StoreConfig};
use ledger_domain::records::AuditEntry;
use ledger_store::{
    Capabilities, Expect, Health, LocalStore, Snapshot, Store, StoreError, StoreId, StoreKind,
    SyncState, Version,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn local(id: &str, path: PathBuf) -> StoreConfig {
    StoreConfig {
        id: id.into(),
        label: id.into(),
        settings: Settings::Local { path },
        accept_risk: false,
    }
}

fn machine(root: &Path, name: &str, device: &str, stores: Vec<StoreConfig>) -> AppState {
    let places = Places {
        config_file: root.join(name).join("config.json"),
        data_dir: root.join(name).join("data"),
    };
    places
        .save_config(&Config {
            stores,
            device: device.into(),
            setup_complete: true,
            ..Config::default()
        })
        .unwrap();
    AppState::open(
        places,
        Arc::new(ledger_config::secrets::InMemory::default()),
        false,
        || None,
    )
    .unwrap()
}

fn shared(root: &Path) -> PathBuf {
    let dir = root.join("shared");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("ledger.json")
}

async fn add_type(state: &AppState, name: &str) {
    apply_value(
        state,
        json!({"op": "type-add", "list": "budget", "name": name}),
    )
    .await
    .unwrap()
    .expect("the edit applied");
}

fn entry(id: &str, at: &str) -> AuditEntry {
    AuditEntry {
        id: id.into(),
        at: at.into(),
        name: id.into(),
        ..Default::default()
    }
}

fn now_minus_days(days: u64) -> String {
    let now = time::OffsetDateTime::now_utc() - time::Duration::days(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        now.year(),
        now.month() as u8,
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

#[tokio::test]
async fn an_edit_on_one_machine_shows_in_the_others_history() {
    let root = tempfile::tempdir().unwrap();
    let primary = shared(root.path());
    let laptop = machine(
        root.path(),
        "a",
        "Laptop",
        vec![local("s", primary.clone())],
    );
    let pc = machine(root.path(), "b", "Windows PC", vec![local("s", primary)]);

    add_type(&laptop, "Pets").await;
    sync(&laptop).await.unwrap();

    let seen = history_of(&pc).await;
    assert!(seen.problems.is_empty(), "{:?}", seen.problems);
    let pets = seen
        .entries
        .iter()
        .find(|e| e.name.contains("Pets"))
        .expect("the laptop's edit reached the pc");
    assert_eq!(pets.actor, "Laptop");
}

#[tokio::test]
async fn an_unreachable_primary_queues_history_and_never_writes_a_mirror() {
    let root = tempfile::tempdir().unwrap();
    let away = root.path().join("away");
    let mirror = root.path().join("mirror");
    std::fs::create_dir_all(&mirror).unwrap();
    let laptop = machine(
        root.path(),
        "a",
        "Laptop",
        vec![
            local("s", away.join("ledger.json")),
            local("m", mirror.join("ledger.json")),
        ],
    );

    let applied = apply_value(
        &laptop,
        json!({"op": "type-add", "list": "budget", "name": "Pets"}),
    )
    .await
    .unwrap()
    .expect("the edit still applied");
    assert!(matches!(applied.sync, SyncState::Behind { .. }));
    assert_eq!(Audit::new(&laptop.places.data_dir).read().await.len(), 1);
    assert!(!away.exists());

    std::fs::create_dir_all(&away).unwrap();
    sync(&laptop).await.unwrap();

    let landed = std::fs::read_dir(away.join("history"))
        .unwrap()
        .filter(|e| e.as_ref().unwrap().path().extension() == Some("json".as_ref()))
        .count();
    assert_eq!(landed, 1);
    assert!(!mirror.join("history").exists());
}

#[tokio::test]
async fn two_machines_publishing_at_once_lose_nothing() {
    let root = tempfile::tempdir().unwrap();
    let primary = shared(root.path());
    let laptop = machine(
        root.path(),
        "a",
        "Laptop",
        vec![local("s", primary.clone())],
    );
    let pc = machine(root.path(), "b", "Windows PC", vec![local("s", primary)]);

    add_type(&laptop, "Pets").await;
    add_type(&pc, "Garden").await;
    let (a, b) = tokio::join!(sync(&laptop), sync(&pc));
    a.unwrap();
    b.unwrap();

    for state in [&laptop, &pc] {
        let names: Vec<String> = history_of(state)
            .await
            .entries
            .into_iter()
            .map(|e| format!("{} {}", e.actor, e.name))
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("Laptop") && n.contains("Pets")),
            "{names:?}"
        );
        assert!(
            names
                .iter()
                .any(|n| n.starts_with("Windows PC") && n.contains("Garden")),
            "{names:?}"
        );
    }
}

#[tokio::test]
async fn one_install_racing_itself_keeps_every_entry() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new("s", shared(root.path()));
    let at = now_minus_days(0);
    audit::publish(&store, "abc", &[entry("x", &at)])
        .await
        .unwrap();

    let (ys, zs) = ([entry("y", &at)], [entry("z", &at)]);
    let (y, z) = tokio::join!(
        audit::publish(&store, "abc", &ys),
        audit::publish(&store, "abc", &zs),
    );
    y.unwrap();
    z.unwrap();

    let all = audit::gather(&store, "abc", &[]).await.entries;
    let mut ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
    ids.sort();
    assert_eq!(ids, vec!["x", "y", "z"]);
}

#[tokio::test]
async fn a_damaged_history_from_another_machine_is_skipped_and_reported() {
    let root = tempfile::tempdir().unwrap();
    let primary = shared(root.path());
    let laptop = machine(
        root.path(),
        "a",
        "Laptop",
        vec![local("s", primary.clone())],
    );
    let history = primary.parent().unwrap().join("history");
    std::fs::create_dir_all(&history).unwrap();
    std::fs::write(history.join("deadbeef.json"), b"not json").unwrap();

    add_type(&laptop, "Pets").await;
    sync(&laptop).await.unwrap();

    let seen = history_of(&laptop).await;
    assert_eq!(seen.entries.len(), 1);
    assert_eq!(seen.problems.len(), 1);
    assert!(seen.problems[0].contains("deadbeef"));
}

#[tokio::test]
async fn existing_local_history_is_published_once() {
    let root = tempfile::tempdir().unwrap();
    let primary = shared(root.path());
    let places_dir = root.path().join("a").join("data");
    let audit = Audit::new(&places_dir);
    for id in ["old-1", "old-2"] {
        audit.append(entry(id, &now_minus_days(3))).await.unwrap();
    }
    let laptop = machine(
        root.path(),
        "a",
        "Laptop",
        vec![local("s", primary.clone())],
    );
    let install = laptop.config().await.install;
    let store = LocalStore::new("s", primary);

    sync(&laptop).await.unwrap();
    sync(&laptop).await.unwrap();
    let local_entries = audit.read().await;
    assert_eq!(
        audit::publish(&store, &install, &local_entries)
            .await
            .unwrap(),
        Publish::UpToDate
    );
    assert_eq!(audit::gather(&store, "other", &[]).await.entries.len(), 2);
}

#[tokio::test]
async fn published_and_merged_history_stay_within_the_caps() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalStore::new("s", shared(root.path()));
    let recent = now_minus_days(1);
    let many = |prefix: &str| -> Vec<AuditEntry> {
        let mut out: Vec<AuditEntry> = (0..400)
            .map(|i| entry(&format!("{prefix}{i}"), &recent))
            .collect();
        out.push(entry(&format!("{prefix}-stale"), &now_minus_days(90)));
        out
    };

    audit::publish(&store, "aaa", &many("a")).await.unwrap();
    audit::publish(&store, "bbb", &many("b")).await.unwrap();

    let slot = std::fs::read(root.path().join("shared/history/aaa.json")).unwrap();
    let published: serde_json::Value = serde_json::from_slice(&slot).unwrap();
    assert_eq!(published["entries"].as_array().unwrap().len(), 400);

    let merged = audit::gather(&store, "aaa", &[]).await.entries;
    assert_eq!(merged.len(), ledger_domain::records::caps::AUDIT);
    assert!(merged.iter().all(|e| !e.id.ends_with("-stale")));
}

struct NoShelf(LocalStore);

#[async_trait::async_trait]
impl Store for NoShelf {
    fn id(&self) -> &StoreId {
        self.0.id()
    }
    fn kind(&self) -> StoreKind {
        self.0.kind()
    }
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
    async fn health(&self) -> Health {
        self.0.health().await
    }
    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        self.0.load().await
    }
    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        self.0.save(body, expect).await
    }
}

#[tokio::test]
async fn a_store_without_a_shelf_keeps_history_local() {
    let root = tempfile::tempdir().unwrap();
    let store = NoShelf(LocalStore::new("s", shared(root.path())));
    let mine = [entry("x", &now_minus_days(0))];

    assert_eq!(
        audit::publish(&store, "abc", &mine).await.unwrap(),
        Publish::NotShared
    );
    let seen = audit::gather(&store, "abc", &mine).await;
    assert!(!seen.shared);
    assert_eq!(seen.entries.len(), 1);
}

struct Intruding {
    inner: LocalStore,
}

struct IntrudingShelf {
    inner: Arc<dyn ledger_store::Shelf>,
}

struct IntrudingSlot {
    inner: Arc<dyn Store>,
    struck: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl Store for Intruding {
    fn id(&self) -> &StoreId {
        self.inner.id()
    }
    fn kind(&self) -> StoreKind {
        self.inner.kind()
    }
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    async fn health(&self) -> Health {
        self.inner.health().await
    }
    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        self.inner.load().await
    }
    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        self.inner.save(body, expect).await
    }
    fn shelf(&self, folder: &str) -> Option<Arc<dyn ledger_store::Shelf>> {
        let inner = self.inner.shelf(folder)?;
        Some(Arc::new(IntrudingShelf { inner }))
    }
}

#[async_trait::async_trait]
impl ledger_store::Shelf for IntrudingShelf {
    async fn names(&self) -> Result<Vec<String>, StoreError> {
        self.inner.names().await
    }
    fn slot(&self, name: &str) -> Option<Arc<dyn Store>> {
        Some(Arc::new(IntrudingSlot {
            inner: self.inner.slot(name)?,
            struck: false.into(),
        }))
    }
}

#[async_trait::async_trait]
impl Store for IntrudingSlot {
    fn id(&self) -> &StoreId {
        self.inner.id()
    }
    fn kind(&self) -> StoreKind {
        self.inner.kind()
    }
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    async fn health(&self) -> Health {
        self.inner.health().await
    }
    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        self.inner.load().await
    }
    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        if !self.struck.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let intruder =
                json!({"v": 1, "install": "abc", "entries": [entry("w", &now_minus_days(0))]});
            self.inner
                .save(&serde_json::to_vec(&intruder).unwrap(), Expect::Force)
                .await?;
        }
        self.inner.save(body, expect).await
    }
}

#[tokio::test]
async fn a_write_that_lands_first_is_merged_not_overwritten() {
    let root = tempfile::tempdir().unwrap();
    let store = Intruding {
        inner: LocalStore::new("s", shared(root.path())),
    };
    let mine = [entry("y", &now_minus_days(0))];

    assert_eq!(
        audit::publish(&store, "abc", &mine).await.unwrap(),
        Publish::Landed(2)
    );
    let mut ids: Vec<String> = audit::gather(&store.inner, "abc", &[])
        .await
        .entries
        .into_iter()
        .map(|e| e.id)
        .collect();
    ids.sort();
    assert_eq!(ids, vec!["w", "y"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn separate_store_instances_keep_both_publishes() {
    let root = tempfile::tempdir().unwrap();
    let path = shared(root.path());
    let one = LocalStore::new("s", path.clone());
    let two = LocalStore::new("s", path);
    let at = now_minus_days(0);
    for i in 0..50 {
        let install = format!("install-{i}");
        let (a, b) = ([entry("a", &at)], [entry("b", &at)]);
        let (left, right) = tokio::join!(
            audit::publish(&one, &install, &a),
            audit::publish(&two, &install, &b)
        );
        left.unwrap();
        right.unwrap();
        let seen = audit::gather(&one, &install, &[]).await.entries;
        let ids: Vec<&str> = seen
            .iter()
            .filter(|e| e.id == "a" || e.id == "b")
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids.len(), 2, "iteration {i} lost an entry: {ids:?}");
    }
}
