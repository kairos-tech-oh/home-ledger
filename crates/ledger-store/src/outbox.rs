//! The queue of edits made since the primary last accepted a write.
//!
//! Durable by construction: an entry is on disk before the caller is told the
//! edit succeeded, so closing the laptop mid-edit loses nothing.

use crate::sealed::Vault;
use crate::store::StoreError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// One edit, as the writer would apply it. Kept opaque here — the store layer
/// moves ops around and never interprets them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingOp {
    pub id: String,
    /// When the edit was made on this machine, not when it was pushed.
    pub at: String,
    pub device: String,
    pub op: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum OutboxError {
    #[error("outbox is unreadable: {0}")]
    Corrupt(String),
    /// Encrypted, and this machine has not been unlocked.
    #[error("the queued edits are encrypted; enter the passphrase to unlock them")]
    Locked,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A file holding the ops not yet landed on the primary, oldest first.
/// Sealed like everything else when encryption is on: a queued edit holds
/// the same amounts the ledger does.
pub struct Outbox {
    path: PathBuf,
    vault: Arc<Vault>,
}

fn sealing(e: StoreError) -> OutboxError {
    match e {
        StoreError::Locked => OutboxError::Locked,
        other => OutboxError::Corrupt(other.to_string()),
    }
}

impl Outbox {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::sealed(path, Vault::new())
    }

    pub fn sealed(path: impl Into<PathBuf>, vault: Arc<Vault>) -> Self {
        Self {
            path: path.into(),
            vault,
        }
    }

    /// Held while the queue is read, changed and written back. The desktop app
    /// and the `hl` command line can both be adding edits at once; without
    /// this, each could read the same queue and one edit would be lost.
    async fn exclusive(&self) -> Result<std::fs::File, OutboxError> {
        let mut name = self.path.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::task::spawn_blocking(move || {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)?;
            file.lock()?;
            Ok::<_, std::io::Error>(file)
        })
        .await
        .map_err(|e| OutboxError::Corrupt(e.to_string()))?
        .map_err(OutboxError::Io)
    }

    /// Writes the queue back as it is, so it is sealed (or unsealed) to match
    /// whether encryption is now on.
    pub async fn reseal(&self) -> Result<(), OutboxError> {
        let _held = self.exclusive().await?;
        match tokio::fs::metadata(&self.path).await {
            Ok(_) => {
                let ops = self.read().await?;
                self.write(&ops).await
            }
            Err(_) => Ok(()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn read(&self) -> Result<Vec<PendingOp>, OutboxError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                let plain = self.vault.open(&bytes).map_err(sealing)?;
                serde_json::from_slice(&plain).map_err(|e| OutboxError::Corrupt(e.to_string()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(OutboxError::Io(e)),
        }
    }

    pub async fn len(&self) -> usize {
        self.read().await.map(|ops| ops.len()).unwrap_or(0)
    }

    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    async fn write(&self, ops: &[PendingOp]) -> Result<(), OutboxError> {
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let plain = serde_json::to_vec(ops).map_err(|e| OutboxError::Corrupt(e.to_string()))?;
        let body = self.vault.seal(&plain).map_err(sealing)?;
        let temp = self
            .path
            .with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
        tokio::fs::write(&temp, &body).await?;
        tokio::fs::rename(&temp, &self.path).await?;
        Ok(())
    }

    /// Append an op and return how many are now waiting.
    pub async fn push(&self, op: PendingOp) -> Result<usize, OutboxError> {
        let _held = self.exclusive().await?;
        let mut ops = self.read().await?;
        ops.push(op);
        self.write(&ops).await?;
        Ok(ops.len())
    }

    /// Drop the ops that have landed, keeping anything queued since. Takes ids
    /// rather than a count because an edit can arrive while a flush is in
    /// flight, and truncating by length would silently eat it.
    pub async fn forget(&self, landed: &[String]) -> Result<usize, OutboxError> {
        let _held = self.exclusive().await?;
        let ops = self.read().await?;
        let kept: Vec<PendingOp> = ops
            .into_iter()
            .filter(|o| !landed.contains(&o.id))
            .collect();
        let remaining = kept.len();
        self.write(&kept).await?;
        Ok(remaining)
    }

    pub async fn clear(&self) -> Result<(), OutboxError> {
        let _held = self.exclusive().await?;
        self.write(&[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(tag: &str) -> PendingOp {
        PendingOp {
            id: uuid::Uuid::new_v4().simple().to_string(),
            at: "2026-09-21T00:00:00Z".into(),
            device: "laptop".into(),
            op: serde_json::json!({ "op": tag }),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn two_programs_queueing_at_once_lose_no_edit() {
        // The desktop app and a script, each with its own handle on one file.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outbox.json");
        let app = std::sync::Arc::new(Outbox::new(&path));
        let script = std::sync::Arc::new(Outbox::new(&path));
        let mut tasks = Vec::new();
        for i in 0..40 {
            let outbox = if i % 2 == 0 { app.clone() } else { script.clone() };
            tasks.push(tokio::spawn(async move { outbox.push(op("edit")).await.unwrap() }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        assert_eq!(app.read().await.unwrap().len(), 40);
    }

    #[tokio::test]
    async fn a_missing_outbox_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        assert!(outbox.is_empty().await);
    }

    #[tokio::test]
    async fn ops_come_back_in_the_order_they_were_made() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = Outbox::new(dir.path().join("outbox.json"));
        outbox.push(op("first")).await.unwrap();
        outbox.push(op("second")).await.unwrap();

        let ops = outbox.read().await.unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].op["op"], "first");
        assert_eq!(ops[1].op["op"], "second");
    }

    #[tokio::test]
    async fn an_op_queued_during_a_flush_is_not_eaten() {
        // The bug this exists to prevent: flushing two ops, then truncating by
        // length, would discard a third queued while the push was in flight.
        let dir = tempfile::tempdir().unwrap();
        let outbox = Outbox::new(dir.path().join("outbox.json"));

        let a = op("a");
        let b = op("b");
        outbox.push(a.clone()).await.unwrap();
        outbox.push(b.clone()).await.unwrap();

        let flushing: Vec<String> = vec![a.id.clone(), b.id.clone()];
        outbox.push(op("arrived late")).await.unwrap();

        let remaining = outbox.forget(&flushing).await.unwrap();
        assert_eq!(remaining, 1);
        assert_eq!(outbox.read().await.unwrap()[0].op["op"], "arrived late");
    }

    #[tokio::test]
    async fn the_queue_survives_being_reopened() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outbox.json");
        Outbox::new(&path).push(op("durable")).await.unwrap();

        let reopened = Outbox::new(&path);
        assert_eq!(reopened.len().await, 1);
    }
}
