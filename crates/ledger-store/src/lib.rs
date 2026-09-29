//! Where a ledger lives, and how it gets there safely.
//!
//! One store is the primary and is the only one ever written. Everything else
//! is a mirror. Edits queue locally and land under a conditional write, so an
//! unreachable store costs availability, never data.

pub mod engine;
pub mod gdrive;
pub mod local;
pub mod oauth;
pub mod outbox;
pub mod s3;
pub mod sigv4;
pub mod store;

pub use engine::{Document, Engine, EngineError, Loaded, Role, StoreStatus, SyncState};
pub use gdrive::{DriveConfig, DriveStore};
pub use ledger_domain::Relation;
pub use local::LocalStore;
pub use outbox::{Outbox, OutboxError, PendingOp};
pub use s3::{S3Config, S3Store};
pub use sigv4::Credentials;
pub use store::{
    Capabilities, Cas, Expect, Health, Shelf, Snapshot, Store, StoreError, StoreId, StoreKind,
    Version, shelf_name_ok,
};
