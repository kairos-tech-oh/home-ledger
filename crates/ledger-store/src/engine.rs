//! Strict primary with fallbacks.
//!
//! One store is authoritative. Edits apply to the local cache immediately and
//! queue in the outbox; a flush lands them on the primary under a conditional
//! write. Other stores are read-only mirrors kept for when the primary is
//! gone, and promoting one is a decision a person makes, never this code.
//!
//! Divergence is therefore impossible by construction: only the primary is
//! ever written, so two stores can never both hold edits that must be merged.

use crate::outbox::{Outbox, PendingOp};
use crate::store::*;
use ledger_domain::Relation;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// How many times a flush re-reads and replays before giving up.
const MAX_ATTEMPTS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state")]
pub enum SyncState {
    /// The primary holds everything this machine has.
    Synced { version: String },
    /// Edits are waiting because the primary could not be reached.
    Behind { queued: usize, reason: String },
    /// The primary moved in a way a replay could not resolve. Needs a person.
    Blocked { queued: usize, reason: String },
    /// No primary has been configured yet.
    Unconfigured,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreStatus {
    pub id: StoreId,
    pub kind: StoreKind,
    pub role: Role,
    pub health: Health,
    pub capabilities: Capabilities,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Primary,
    Mirror,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("no primary store is configured")]
    NoPrimary,
    #[error(
        "{0} cannot lock reliably, so it is unsafe as a primary; \
         configure it as a mirror or accept the risk explicitly"
    )]
    UnsafePrimary(StoreId),
    #[error(
        "{0} edit(s) are still waiting to sync; \
         sync or discard them before replacing the ledger"
    )]
    QueuedEdits(usize),
    #[error("{0}")]
    Document(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Outbox(#[from] crate::outbox::OutboxError),
}

/// The engine moves bytes around; this is the part that understands what
/// those bytes mean. The writer crate provides the real implementation.
pub trait Document: Send + Sync {
    /// Apply queued ops to whatever the store holds now.
    fn replay(&self, document: Option<&[u8]>, ops: &[PendingOp]) -> Result<Vec<u8>, String>;

    /// Advance the document's lineage, naming the store being written to.
    /// Called on every write, so one copy can later prove it descends from
    /// another.
    fn stamp(&self, document: &[u8], writer: &str) -> Result<Vec<u8>, String>;

    /// Where in its own history a document sits, for placing a backup against
    /// the source of truth.
    fn relation(&self, ours: &[u8], theirs: &[u8]) -> Relation;
}

pub struct Engine {
    primary: Arc<dyn Store>,
    mirrors: Vec<Arc<dyn Store>>,
    outbox: Outbox,
    document: Arc<dyn Document>,
}

impl Engine {
    /// Refuses a primary that cannot promise a real compare-and-swap unless
    /// `accept_risk` says the person was told and chose it anyway.
    pub fn new(
        primary: Arc<dyn Store>,
        mirrors: Vec<Arc<dyn Store>>,
        outbox: Outbox,
        document: Arc<dyn Document>,
        accept_risk: bool,
    ) -> Result<Self, EngineError> {
        let cas = primary.capabilities().cas;
        if cas == Cas::BestEffort && !accept_risk {
            return Err(EngineError::UnsafePrimary(primary.id().clone()));
        }
        Ok(Self {
            primary,
            mirrors,
            outbox,
            document,
        })
    }

    pub fn primary(&self) -> &Arc<dyn Store> {
        &self.primary
    }

    /// Health of every configured store, for the settings screen.
    pub async fn status(&self) -> Vec<StoreStatus> {
        let mut out = Vec::with_capacity(1 + self.mirrors.len());
        out.push(StoreStatus {
            id: self.primary.id().clone(),
            kind: self.primary.kind(),
            role: Role::Primary,
            health: self.primary.health().await,
            capabilities: self.primary.capabilities(),
        });
        for mirror in &self.mirrors {
            out.push(StoreStatus {
                id: mirror.id().clone(),
                kind: mirror.kind(),
                role: Role::Mirror,
                health: mirror.health().await,
                capabilities: mirror.capabilities(),
            });
        }
        out
    }

    /// Read the document. The primary is the truth; a mirror is consulted only
    /// when the primary cannot be reached, and what comes back is explicitly
    /// marked as possibly stale so the UI can say so.
    pub async fn load(&self) -> Result<Loaded, EngineError> {
        match self.primary.load().await {
            Ok(snapshot) => Ok(Loaded {
                snapshot,
                from: self.primary.id().clone(),
                stale: false,
            }),
            Err(e) if e.is_transient() => {
                for mirror in &self.mirrors {
                    if let Ok(snapshot) = mirror.load().await {
                        return Ok(Loaded {
                            snapshot,
                            from: mirror.id().clone(),
                            stale: true,
                        });
                    }
                }
                Err(EngineError::Store(e))
            }
            Err(e) => Err(EngineError::Store(e)),
        }
    }

    /// Queue an edit. Always succeeds once the op is on disk, which is what
    /// lets the app keep working with every store unreachable.
    pub async fn enqueue(&self, op: PendingOp) -> Result<usize, EngineError> {
        Ok(self.outbox.push(op).await?)
    }

    /// Try to land the outbox on the primary.
    ///
    /// Reads the primary, replays every queued op onto what it holds, and
    /// writes back conditionally. A conflict means someone else wrote first,
    /// so it reads and replays again rather than overwriting them.
    pub async fn flush(&self) -> Result<SyncState, EngineError> {
        let queued = self.outbox.read().await?;
        if queued.is_empty() {
            let version = match self.primary.load().await {
                Ok(Some(s)) => s.version.0,
                Ok(None) => String::new(),
                Err(e) if e.is_transient() => {
                    return Ok(SyncState::Behind {
                        queued: 0,
                        reason: e.to_string(),
                    });
                }
                Err(e) => return Err(EngineError::Store(e)),
            };
            return Ok(SyncState::Synced { version });
        }

        let ids: Vec<String> = queued.iter().map(|o| o.id.clone()).collect();

        for _ in 0..MAX_ATTEMPTS {
            let current = match self.primary.load().await {
                Ok(snapshot) => snapshot,
                Err(e) if e.is_transient() => {
                    return Ok(SyncState::Behind {
                        queued: queued.len(),
                        reason: e.to_string(),
                    });
                }
                Err(e) => return Err(EngineError::Store(e)),
            };

            let expect = match &current {
                Some(s) => Expect::Version(s.version.clone()),
                None => Expect::Absent,
            };

            let merged = match self
                .document
                .replay(current.as_ref().map(|s| s.body.as_slice()), &queued)
            {
                Ok(bytes) => bytes,
                Err(reason) => {
                    return Ok(SyncState::Blocked {
                        queued: queued.len(),
                        reason,
                    });
                }
            };

            // Stamped after the replay and before the write, so the lineage
            // names the document that is actually about to be stored.
            let merged = match self.document.stamp(&merged, &self.primary.id().0) {
                Ok(stamped) => stamped,
                Err(reason) => {
                    return Ok(SyncState::Blocked {
                        queued: queued.len(),
                        reason,
                    });
                }
            };

            match self.primary.save(&merged, expect).await {
                Ok(version) => {
                    self.outbox.forget(&ids).await?;
                    self.mirror(&merged).await;
                    return Ok(SyncState::Synced { version: version.0 });
                }
                // Someone wrote between our read and our write. Read again and
                // replay onto theirs rather than clobbering it.
                Err(StoreError::Conflict) => continue,
                Err(e) if e.is_transient() => {
                    return Ok(SyncState::Behind {
                        queued: queued.len(),
                        reason: e.to_string(),
                    });
                }
                Err(e) => return Err(EngineError::Store(e)),
            }
        }

        Ok(SyncState::Blocked {
            queued: queued.len(),
            reason: "the primary kept changing underneath this write".into(),
        })
    }

    /// Replace the document outright.
    ///
    /// Not an edit and deliberately not queued: this is import, and later
    /// promoting a mirror. It refuses while the outbox holds anything, because
    /// those ops were made against a document that is about to stop existing —
    /// replaying them afterwards would apply edits to records that are gone.
    pub async fn adopt(&self, body: &[u8]) -> Result<SyncState, EngineError> {
        let waiting = self.outbox.len().await;
        if waiting > 0 {
            return Err(EngineError::QueuedEdits(waiting));
        }

        let expect = match self.primary.load().await? {
            Some(snapshot) => Expect::Version(snapshot.version),
            None => Expect::Absent,
        };
        let body = self
            .document
            .stamp(body, &self.primary.id().0)
            .map_err(EngineError::Document)?;
        let version = self.primary.save(&body, expect).await?;
        self.mirror(&body).await;
        Ok(SyncState::Synced { version: version.0 })
    }

    /// Copy what landed to each mirror, best effort. A mirror that is down is
    /// caught up by the next successful flush, so a failure here is not one.
    async fn mirror(&self, body: &[u8]) {
        for mirror in &self.mirrors {
            if let Err(e) = mirror.save(body, Expect::Force).await {
                tracing::debug!(store = %mirror.id(), error = %e, "mirror not updated");
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Loaded {
    pub snapshot: Option<Snapshot>,
    pub from: StoreId,
    /// True when this came from a mirror because the primary was unreachable.
    pub stale: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::LocalStore;

    /// Replays by concatenating op tags, which is enough to prove ordering and
    /// that a replay happens onto whatever the primary currently holds.
    struct TagReplay;
    impl Document for TagReplay {
        fn stamp(&self, document: &[u8], _: &str) -> Result<Vec<u8>, String> {
            Ok(document.to_vec())
        }

        fn relation(&self, _: &[u8], _: &[u8]) -> Relation {
            Relation::TooFarApart
        }

        fn replay(&self, document: Option<&[u8]>, ops: &[PendingOp]) -> Result<Vec<u8>, String> {
            let mut text = document
                .map(|d| String::from_utf8_lossy(d).to_string())
                .unwrap_or_default();
            for op in ops {
                if !text.is_empty() {
                    text.push(',');
                }
                text.push_str(op.op["op"].as_str().unwrap_or("?"));
            }
            Ok(text.into_bytes())
        }
    }

    /// Keeps a real lineage, so the engine's stamp call is exercised rather
    /// than stubbed away.
    struct StampingReplay;
    impl Document for StampingReplay {
        fn replay(&self, document: Option<&[u8]>, ops: &[PendingOp]) -> Result<Vec<u8>, String> {
            let mut doc: serde_json::Value = match document {
                Some(d) => serde_json::from_slice(d).map_err(|e| e.to_string())?,
                None => serde_json::json!({ "tags": [] }),
            };
            for op in ops {
                doc["tags"]
                    .as_array_mut()
                    .ok_or("tags is not a list")?
                    .push(op.op["op"].clone());
            }
            serde_json::to_vec(&doc).map_err(|e| e.to_string())
        }

        fn stamp(&self, document: &[u8], writer: &str) -> Result<Vec<u8>, String> {
            let mut doc: serde_json::Value =
                serde_json::from_slice(document).map_err(|e| e.to_string())?;
            let next = doc["lineage"]["generation"].as_u64().unwrap_or(0) + 1;
            doc["lineage"] = serde_json::json!({ "generation": next, "writer": writer });
            serde_json::to_vec(&doc).map_err(|e| e.to_string())
        }

        fn relation(&self, _: &[u8], _: &[u8]) -> Relation {
            Relation::TooFarApart
        }
    }

    struct RefuseReplay;
    impl Document for RefuseReplay {
        fn replay(&self, _: Option<&[u8]>, _: &[PendingOp]) -> Result<Vec<u8>, String> {
            Err("a rule refused this op".into())
        }

        fn stamp(&self, document: &[u8], _: &str) -> Result<Vec<u8>, String> {
            Ok(document.to_vec())
        }

        fn relation(&self, _: &[u8], _: &[u8]) -> Relation {
            Relation::TooFarApart
        }
    }

    fn op(tag: &str) -> PendingOp {
        PendingOp {
            id: uuid::Uuid::new_v4().simple().to_string(),
            at: "2026-09-21T00:00:00Z".into(),
            device: "laptop".into(),
            op: serde_json::json!({ "op": tag }),
        }
    }

    fn engine(dir: &tempfile::TempDir, document: Arc<dyn Document>) -> Engine {
        let primary = Arc::new(LocalStore::new("primary", dir.path().join("primary.json")));
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        Engine::new(primary, vec![], outbox, document, false).unwrap()
    }

    #[tokio::test]
    async fn a_flush_lands_queued_ops_and_empties_the_outbox() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(&dir, Arc::new(TagReplay));

        e.enqueue(op("one")).await.unwrap();
        e.enqueue(op("two")).await.unwrap();

        let state = e.flush().await.unwrap();
        assert!(matches!(state, SyncState::Synced { .. }));

        let stored = e.primary().load().await.unwrap().unwrap();
        assert_eq!(String::from_utf8(stored.body).unwrap(), "one,two");
        assert!(e.outbox.is_empty().await);
    }

    #[tokio::test]
    async fn edits_survive_the_primary_being_unreachable() {
        let dir = tempfile::tempdir().unwrap();
        let primary = Arc::new(LocalStore::new(
            "primary",
            "/definitely/not/here/ledger.json",
        ));
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        let e = Engine::new(primary, vec![], outbox, Arc::new(TagReplay), false).unwrap();

        e.enqueue(op("made offline")).await.unwrap();
        let state = e.flush().await.unwrap();

        match state {
            SyncState::Behind { queued, .. } => assert_eq!(queued, 1),
            other => panic!("expected to be behind, got {other:?}"),
        }
        // Still queued, not lost.
        assert_eq!(e.outbox.len().await, 1);
    }

    #[tokio::test]
    async fn a_replay_onto_someone_elses_write_keeps_both() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(&dir, Arc::new(TagReplay));

        // Another machine got there first.
        e.primary().save(b"theirs", Expect::Absent).await.unwrap();

        e.enqueue(op("ours")).await.unwrap();
        e.flush().await.unwrap();

        let stored = e.primary().load().await.unwrap().unwrap();
        assert_eq!(String::from_utf8(stored.body).unwrap(), "theirs,ours");
    }

    #[tokio::test]
    async fn a_refused_op_blocks_rather_than_dropping_the_queue() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(&dir, Arc::new(RefuseReplay));

        e.enqueue(op("bad")).await.unwrap();
        let state = e.flush().await.unwrap();

        assert!(matches!(state, SyncState::Blocked { .. }));
        assert_eq!(
            e.outbox.len().await,
            1,
            "a blocked op must not be discarded"
        );
    }

    #[tokio::test]
    async fn a_store_that_cannot_lock_is_refused_as_primary() {
        let dir = tempfile::tempdir().unwrap();
        let nas = Arc::new(LocalStore::nas("nas", dir.path().join("ledger.json")));
        let outbox = Outbox::new(dir.path().join("outbox.json"));

        let refused = Engine::new(nas.clone(), vec![], outbox, Arc::new(TagReplay), false);
        assert!(matches!(refused, Err(EngineError::UnsafePrimary(_))));

        // Allowed when the person has been told and chose it anyway.
        let outbox = Outbox::new(dir.path().join("outbox2.json"));
        assert!(Engine::new(nas, vec![], outbox, Arc::new(TagReplay), true).is_ok());
    }

    #[tokio::test]
    async fn every_write_advances_the_lineage() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(&dir, Arc::new(StampingReplay));

        e.enqueue(op("one")).await.unwrap();
        e.flush().await.unwrap();
        let first = generation_of(&e).await;

        e.enqueue(op("two")).await.unwrap();
        e.flush().await.unwrap();
        let second = generation_of(&e).await;

        assert_eq!(first, 1, "the first write should start the lineage");
        assert_eq!(second, 2, "the second write should advance it");
    }

    async fn generation_of(e: &Engine) -> u64 {
        let body = e.primary().load().await.unwrap().unwrap().body;
        let doc: serde_json::Value = serde_json::from_slice(&body).unwrap();
        doc["lineage"]["generation"].as_u64().unwrap_or(0)
    }

    #[tokio::test]
    async fn adopting_replaces_the_document_outright() {
        let dir = tempfile::tempdir().unwrap();
        let e = engine(&dir, Arc::new(TagReplay));
        e.primary().save(b"old", Expect::Absent).await.unwrap();

        e.adopt(b"imported").await.expect("adopted");

        assert_eq!(e.primary().load().await.unwrap().unwrap().body, b"imported");
    }

    #[tokio::test]
    async fn adopting_is_refused_while_edits_are_still_queued() {
        // Those ops were made against a document that is about to stop
        // existing; replaying them afterwards would hit records that are gone.
        let dir = tempfile::tempdir().unwrap();
        let e = engine(&dir, Arc::new(TagReplay));
        e.enqueue(op("unsynced")).await.unwrap();

        let refused = e.adopt(b"imported").await;
        assert!(matches!(refused, Err(EngineError::QueuedEdits(1))));
        assert_eq!(e.outbox.len().await, 1, "a refused adopt dropped the queue");
        assert!(
            e.primary().load().await.unwrap().is_none(),
            "it wrote anyway"
        );
    }

    #[tokio::test]
    async fn adopting_updates_the_mirrors_too() {
        let dir = tempfile::tempdir().unwrap();
        let mirror = Arc::new(LocalStore::new("mirror", dir.path().join("mirror.json")));
        let primary = Arc::new(LocalStore::new("primary", dir.path().join("primary.json")));
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        let e = Engine::new(
            primary,
            vec![mirror.clone()],
            outbox,
            Arc::new(TagReplay),
            false,
        )
        .unwrap();

        e.adopt(b"imported").await.expect("adopted");
        assert_eq!(mirror.load().await.unwrap().unwrap().body, b"imported");
    }

    #[tokio::test]
    async fn a_mirror_is_read_when_the_primary_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let mirror = Arc::new(LocalStore::new("mirror", dir.path().join("mirror.json")));
        mirror
            .save(b"from the mirror", Expect::Absent)
            .await
            .unwrap();

        let primary = Arc::new(LocalStore::new(
            "primary",
            "/definitely/not/here/ledger.json",
        ));
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        let e = Engine::new(primary, vec![mirror], outbox, Arc::new(TagReplay), false).unwrap();

        let loaded = e.load().await.unwrap();
        assert!(
            loaded.stale,
            "a mirror read must be flagged as possibly stale"
        );
        assert_eq!(loaded.snapshot.unwrap().body, b"from the mirror");
    }

    #[tokio::test]
    async fn a_successful_flush_updates_the_mirrors() {
        let dir = tempfile::tempdir().unwrap();
        let mirror = Arc::new(LocalStore::new("mirror", dir.path().join("mirror.json")));
        let primary = Arc::new(LocalStore::new("primary", dir.path().join("primary.json")));
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        let e = Engine::new(
            primary,
            vec![mirror.clone()],
            outbox,
            Arc::new(TagReplay),
            false,
        )
        .unwrap();

        e.enqueue(op("copied")).await.unwrap();
        e.flush().await.unwrap();

        assert_eq!(mirror.load().await.unwrap().unwrap().body, b"copied");
    }
}
