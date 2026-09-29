//! What a storage backend must provide, and what it is allowed to promise.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;

/// Proof of which version of the document was read. Backends mint these
/// however they like — an S3 ETag, a Drive revision id, a content hash — and
/// the engine only ever compares them for equality.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version(pub String);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a write is conditional on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    /// Replace exactly this version, or fail.
    Version(Version),
    /// Create only if nothing is stored yet.
    Absent,
    /// Overwrite regardless. Only ever used when a human has been shown the
    /// conflict and picked a side.
    Force,
}

/// A document read back, with the version that produced it.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub body: Vec<u8>,
    pub version: Version,
}

/// How much a backend's compare-and-swap is actually worth.
///
/// This is the honest part of the design. A fallback chain that treats an SMB
/// share and an S3 bucket as equivalent will lose data on the share, because
/// only one of them can refuse a stale write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Cas {
    /// The service itself rejects a stale write. S3 `If-Match`, Drive
    /// `If-Match`. Safe for concurrent writers on different machines.
    Native,
    /// Emulated by reading, comparing a hash, then writing under a local lock.
    /// Safe for several processes on one machine, not across machines.
    LocalLock,
    /// The service will not refuse a stale write, but it versions every change
    /// and keeps the previous one, so a clobber is detected immediately
    /// afterwards and what it overwrote is still recoverable.
    ///
    /// Google Drive is this. Weaker than [`Cas::Native`] — a lost update is
    /// caught rather than prevented — but far stronger than a share that
    /// notices nothing.
    CheckedAfterWrite,
    /// Best effort only. Network filesystems do not reliably lock, so two
    /// machines writing at once can interleave. Fine as a mirror, and the
    /// engine refuses to make it a primary without being told twice.
    BestEffort,
    /// No concurrency to protect against, because nothing else can write here.
    Exclusive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub cas: Cas,
    /// Whether another machine can see and write this store.
    pub shared: bool,
    /// Largest document the backend will accept.
    pub max_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreKind {
    Local,
    S3,
    GoogleDrive,
    Nas,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StoreId(pub String);

impl fmt::Display for StoreId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "state", content = "detail")]
pub enum Health {
    Reachable,
    /// Reachable but refused us: bad credentials, expired token, no permission.
    Denied(String),
    Unreachable(String),
}

impl Health {
    pub fn is_reachable(&self) -> bool {
        matches!(self, Health::Reachable)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The stored version is not the one we expected. The engine replays onto
    /// the newer document rather than treating this as a failure.
    #[error("the document changed underneath this write")]
    Conflict,
    #[error("store is unreachable: {0}")]
    Unreachable(String),
    #[error("store refused the request: {0}")]
    Denied(String),
    #[error("document is {size} bytes, over this store's {limit} limit")]
    TooLarge { size: u64, limit: u64 },
    #[error("stored document is not readable: {0}")]
    Corrupt(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl StoreError {
    /// Whether retrying the same call could plausibly succeed later.
    pub fn is_transient(&self) -> bool {
        matches!(self, StoreError::Unreachable(_))
    }
}

/// One place a ledger can live.
///
/// Deliberately narrow: a ledger is a single document, so this is a versioned
/// blob slot, not a filesystem. Everything a backend needs to be useful in a
/// fallback chain is here and nothing else is.
#[async_trait]
pub trait Store: Send + Sync {
    fn id(&self) -> &StoreId;
    fn kind(&self) -> StoreKind;
    fn capabilities(&self) -> Capabilities;

    /// A cheap liveness probe. Called to choose the active store, so it must
    /// not download the document.
    async fn health(&self) -> Health;

    /// Read the document, or `None` if this store has never held one.
    async fn load(&self) -> Result<Option<Snapshot>, StoreError>;

    /// Write, subject to `expect`. Returns the version now stored.
    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError>;

    fn shelf(&self, _folder: &str) -> Option<Arc<dyn Shelf>> {
        None
    }
}

#[async_trait]
pub trait Shelf: Send + Sync {
    async fn names(&self) -> Result<Vec<String>, StoreError>;
    fn slot(&self, name: &str) -> Option<Arc<dyn Store>>;
}

pub fn shelf_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
