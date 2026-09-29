//! A ledger in a file on this machine. Also the shape a NAS mount takes,
//! which is why the guarantees it reports are configurable.

use crate::store::*;
use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct LocalStore {
    id: StoreId,
    path: PathBuf,
    kind: StoreKind,
    capabilities: Capabilities,
    /// Whether a missing directory may be created. False for a mounted share:
    /// creating the mount point would write to the local disk underneath it
    /// and look like a successful save that never reached the NAS.
    create_dirs: bool,
    anchor: Option<PathBuf>,
    /// Serialises this process's own writes. Cross-process safety comes from
    /// the atomic rename, not from this.
    gate: Arc<Mutex<()>>,
}

impl LocalStore {
    /// A file only this app writes.
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: StoreId(id.into()),
            path: path.into(),
            kind: StoreKind::Local,
            capabilities: Capabilities {
                cas: Cas::LocalLock,
                shared: false,
                max_bytes: 64 * 1024 * 1024,
            },
            create_dirs: true,
            anchor: None,
            gate: Arc::new(Mutex::new(())),
        }
    }

    /// A file on a mounted share. Locking over SMB/NFS is not dependable, so
    /// this says so rather than pretending otherwise.
    pub fn nas(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        let mut store = Self::new(id, path);
        store.kind = StoreKind::Nas;
        store.capabilities = Capabilities {
            cas: Cas::BestEffort,
            shared: true,
            max_bytes: 64 * 1024 * 1024,
        };
        store.create_dirs = false;
        store
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The version of a body is its content hash, so a file edited outside
    /// this app still compares unequal and is caught as a conflict.
    fn version_of(body: &[u8]) -> Version {
        let mut hasher = DefaultHasher::new();
        body.hash(&mut hasher);
        Version(format!("{:016x}", hasher.finish()))
    }

    async fn read_raw(&self) -> Result<Option<Vec<u8>>, StoreError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => Ok(Some(bytes)),
            // A missing file in a directory that exists is a first run. A
            // missing directory is a store we cannot reach — an unmounted
            // share looks exactly like an empty one otherwise, and adopting
            // "empty" would propose wiping the ledger.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if self.dir_exists().await {
                    Ok(None)
                } else {
                    Err(StoreError::Unreachable(format!(
                        "{} is not there",
                        self.path.parent().unwrap_or(&self.path).display()
                    )))
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(StoreError::Denied(e.to_string()))
            }
            Err(e) => Err(StoreError::Unreachable(e.to_string())),
        }
    }

    fn slot_in(&self, dir: &Path, name: &str) -> Self {
        Self {
            id: StoreId(format!("{}/{name}", self.id)),
            path: dir.join(format!("{name}.json")),
            kind: self.kind,
            capabilities: self.capabilities,
            create_dirs: self.create_dirs,
            anchor: self.path.parent().map(Path::to_path_buf),
            gate: self.gate.clone(),
        }
    }

    async fn prepare_dir(&self) -> Result<(), StoreError> {
        let Some(dir) = self.path.parent() else {
            return Ok(());
        };
        let present = tokio::fs::metadata(dir)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false);
        if present {
            return Ok(());
        }
        if !self.create_dirs && (self.anchor.is_none() || !self.dir_exists().await) {
            return Err(StoreError::Unreachable(format!(
                "{} is not mounted",
                dir.display()
            )));
        }
        tokio::fs::create_dir_all(dir).await?;
        Ok(())
    }

    async fn lock_file(&self) -> Result<Option<std::fs::File>, StoreError> {
        let mut name = self.path.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        let locked = tokio::task::spawn_blocking(move || {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)?;
            file.lock()?;
            Ok::<_, std::io::Error>(file)
        })
        .await
        .map_err(|e| StoreError::Unreachable(e.to_string()))?;
        match locked {
            Ok(file) => Ok(Some(file)),
            Err(_) if self.capabilities.cas == Cas::BestEffort => Ok(None),
            Err(e) => Err(StoreError::Io(e)),
        }
    }

    async fn dir_exists(&self) -> bool {
        match self.anchor.as_deref().or(self.path.parent()) {
            Some(dir) => tokio::fs::metadata(dir)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false),
            None => false,
        }
    }
}

#[async_trait]
impl Store for LocalStore {
    fn id(&self) -> &StoreId {
        &self.id
    }

    fn kind(&self) -> StoreKind {
        self.kind
    }

    fn capabilities(&self) -> Capabilities {
        self.capabilities
    }

    async fn health(&self) -> Health {
        // The directory has to exist and be writable; the file itself need not.
        let dir = match self.path.parent() {
            Some(d) => d,
            None => return Health::Unreachable("path has no parent".into()),
        };
        match tokio::fs::metadata(dir).await {
            Ok(meta) if meta.is_dir() => {
                if meta.permissions().readonly() {
                    Health::Denied(format!("{} is read only", dir.display()))
                } else {
                    Health::Reachable
                }
            }
            Ok(_) => Health::Unreachable(format!("{} is not a directory", dir.display())),
            Err(e) => Health::Unreachable(e.to_string()),
        }
    }

    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        Ok(self.read_raw().await?.map(|body| Snapshot {
            version: Self::version_of(&body),
            body,
        }))
    }

    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        let limit = self.capabilities.max_bytes;
        if body.len() as u64 > limit {
            return Err(StoreError::TooLarge {
                size: body.len() as u64,
                limit,
            });
        }

        let _held = self.gate.lock().await;
        self.prepare_dir().await?;
        let _locked = self.lock_file().await?;

        let current = self.read_raw().await?;
        match (&expect, &current) {
            (Expect::Absent, Some(_)) => return Err(StoreError::Conflict),
            (Expect::Version(_), None) => return Err(StoreError::Conflict),
            (Expect::Version(want), Some(bytes)) if &Self::version_of(bytes) != want => {
                return Err(StoreError::Conflict);
            }
            _ => {}
        }

        // Write beside the target and rename: a crash mid-write leaves the
        // previous document intact rather than a truncated one.
        let temp = self
            .path
            .with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
        tokio::fs::write(&temp, body).await?;
        if let Err(e) = tokio::fs::rename(&temp, &self.path).await {
            let _ = tokio::fs::remove_file(&temp).await;
            return Err(StoreError::Io(e));
        }

        Ok(Self::version_of(body))
    }

    fn shelf(&self, folder: &str) -> Option<Arc<dyn Shelf>> {
        if !shelf_name_ok(folder) || self.anchor.is_some() {
            return None;
        }
        let owner = self.path.parent()?.to_path_buf();
        Some(Arc::new(LocalShelf {
            dir: owner.join(folder),
            owner,
            template: Self {
                id: self.id.clone(),
                path: self.path.clone(),
                kind: self.kind,
                capabilities: self.capabilities,
                create_dirs: self.create_dirs,
                anchor: None,
                gate: self.gate.clone(),
            },
        }))
    }
}

struct LocalShelf {
    dir: PathBuf,
    owner: PathBuf,
    template: LocalStore,
}

#[async_trait]
impl Shelf for LocalShelf {
    async fn names(&self) -> Result<Vec<String>, StoreError> {
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return if tokio::fs::metadata(&self.owner).await.is_ok() {
                    Ok(Vec::new())
                } else {
                    Err(StoreError::Unreachable(format!(
                        "{} is not there",
                        self.owner.display()
                    )))
                };
            }
            Err(e) => return Err(StoreError::Unreachable(e.to_string())),
        };
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let file = entry.file_name();
            if let Some(name) = file.to_str().and_then(|f| f.strip_suffix(".json"))
                && shelf_name_ok(name)
            {
                names.push(name.to_string());
            }
        }
        names.sort();
        Ok(names)
    }

    fn slot(&self, name: &str) -> Option<Arc<dyn Store>> {
        shelf_name_ok(name)
            .then(|| Arc::new(self.template.slot_in(&self.dir, name)) as Arc<dyn Store>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &tempfile::TempDir) -> LocalStore {
        LocalStore::new("local", dir.path().join("ledger.json"))
    }

    #[tokio::test]
    async fn an_empty_store_loads_as_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(store(&dir).load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_document_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let version = s.save(b"{\"v\":3}", Expect::Absent).await.unwrap();

        let got = s.load().await.unwrap().expect("something stored");
        assert_eq!(got.body, b"{\"v\":3}");
        assert_eq!(got.version, version);
    }

    #[tokio::test]
    async fn creating_twice_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.save(b"first", Expect::Absent).await.unwrap();

        let again = s.save(b"second", Expect::Absent).await;
        assert!(matches!(again, Err(StoreError::Conflict)));
        // The first write survived, which is the whole point.
        assert_eq!(s.load().await.unwrap().unwrap().body, b"first");
    }

    #[tokio::test]
    async fn a_stale_version_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let first = s.save(b"one", Expect::Absent).await.unwrap();
        s.save(b"two", Expect::Version(first.clone()))
            .await
            .unwrap();

        // Someone still holding the first version must not win.
        let stale = s.save(b"three", Expect::Version(first)).await;
        assert!(matches!(stale, Err(StoreError::Conflict)));
        assert_eq!(s.load().await.unwrap().unwrap().body, b"two");
    }

    #[tokio::test]
    async fn a_file_changed_outside_the_app_is_caught() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        let version = s.save(b"ours", Expect::Absent).await.unwrap();

        std::fs::write(s.path(), b"edited by hand").unwrap();

        let write = s.save(b"ours again", Expect::Version(version)).await;
        assert!(matches!(write, Err(StoreError::Conflict)));
    }

    #[tokio::test]
    async fn force_overwrites_whatever_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.save(b"one", Expect::Absent).await.unwrap();
        s.save(b"forced", Expect::Force).await.unwrap();
        assert_eq!(s.load().await.unwrap().unwrap().body, b"forced");
    }

    #[tokio::test]
    async fn an_oversized_document_is_refused_before_it_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = store(&dir);
        s.capabilities.max_bytes = 8;

        let write = s.save(b"far too long to fit", Expect::Absent).await;
        assert!(matches!(write, Err(StoreError::TooLarge { .. })));
        assert!(s.load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_nas_store_admits_it_cannot_lock() {
        let dir = tempfile::tempdir().unwrap();
        let s = LocalStore::nas("nas", dir.path().join("ledger.json"));
        assert_eq!(s.capabilities().cas, Cas::BestEffort);
        assert!(s.capabilities().shared);
    }

    #[tokio::test]
    async fn health_reports_a_missing_directory() {
        let s = LocalStore::new("gone", "/definitely/not/here/ledger.json");
        assert!(!s.health().await.is_reachable());
    }

    #[tokio::test]
    async fn a_missing_directory_is_unreachable_not_empty() {
        // The distinction that matters: an unmounted share must not read as an
        // empty ledger, or the app would offer to start one from scratch.
        let s = LocalStore::new("gone", "/definitely/not/here/ledger.json");
        let read = s.load().await;
        assert!(matches!(read, Err(StoreError::Unreachable(_))));
    }

    #[tokio::test]
    async fn a_missing_file_in_a_real_directory_is_just_a_first_run() {
        let dir = tempfile::tempdir().unwrap();
        assert!(store(&dir).load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn an_unmounted_share_refuses_the_write_rather_than_faking_it() {
        // Writing here would create the mount point on the local disk and
        // report success, with the data nowhere near the NAS.
        let s = LocalStore::nas("nas", "/definitely/not/mounted/ledger.json");
        let write = s.save(b"{}", Expect::Absent).await;
        assert!(matches!(write, Err(StoreError::Unreachable(_))));
        assert!(!std::path::Path::new("/definitely/not/mounted").exists());
    }

    #[tokio::test]
    async fn a_shelf_lists_what_its_slots_hold() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = store(&dir).shelf("history").unwrap();
        assert!(shelf.names().await.unwrap().is_empty());

        let slot = shelf.slot("abc-1").unwrap();
        assert!(slot.load().await.unwrap().is_none());
        slot.save(b"[]", Expect::Absent).await.unwrap();
        std::fs::write(dir.path().join("history/not ok.json"), b"[]").unwrap();

        assert_eq!(shelf.names().await.unwrap(), vec!["abc-1"]);
        assert!(shelf.slot("../ledger").is_none());
    }

    #[tokio::test]
    async fn a_shelf_on_a_missing_directory_is_unreachable() {
        let s = LocalStore::new("gone", "/definitely/not/here/ledger.json");
        let shelf = s.shelf("history").unwrap();
        assert!(matches!(
            shelf.names().await,
            Err(StoreError::Unreachable(_))
        ));
        let slot = shelf.slot("abc").unwrap();
        assert!(matches!(slot.load().await, Err(StoreError::Unreachable(_))));
    }

    #[tokio::test]
    async fn a_shelf_on_a_mounted_share_may_create_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let share = LocalStore::nas("nas", dir.path().join("ledger.json"));
        let slot = share.shelf("history").unwrap().slot("abc").unwrap();
        slot.save(b"[]", Expect::Absent).await.unwrap();
        assert!(dir.path().join("history/abc.json").exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_instances_creating_at_once_have_one_winner() {
        for _ in 0..50 {
            let dir = tempfile::tempdir().unwrap();
            let one = store(&dir);
            let two = store(&dir);
            let (a, b) = tokio::join!(
                one.save(b"one", Expect::Absent),
                two.save(b"two", Expect::Absent)
            );
            let wins = [a.is_ok(), b.is_ok()].iter().filter(|w| **w).count();
            assert_eq!(wins, 1);
            assert!(
                matches!(a, Err(StoreError::Conflict)) || matches!(b, Err(StoreError::Conflict))
            );
        }
    }

    #[tokio::test]
    async fn a_lock_file_is_not_listed_on_the_shelf() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = store(&dir).shelf("history").unwrap();
        shelf
            .slot("abc")
            .unwrap()
            .save(b"[]", Expect::Absent)
            .await
            .unwrap();
        assert!(dir.path().join("history/abc.json.lock").exists());
        assert_eq!(shelf.names().await.unwrap(), vec!["abc"]);
    }
}
