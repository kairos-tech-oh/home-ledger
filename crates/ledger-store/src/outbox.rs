//! The queue of edits made since the primary last accepted a write.
//!
//! Durable by construction: an entry is on disk before the caller is told the
//! edit succeeded, so closing the laptop mid-edit loses nothing.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

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
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A file holding the ops not yet landed on the primary, oldest first.
pub struct Outbox {
    path: PathBuf,
}

impl Outbox {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn read(&self) -> Result<Vec<PendingOp>, OutboxError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| OutboxError::Corrupt(e.to_string()))
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
        let body = serde_json::to_vec(ops).map_err(|e| OutboxError::Corrupt(e.to_string()))?;
        let temp = self
            .path
            .with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
        tokio::fs::write(&temp, &body).await?;
        tokio::fs::rename(&temp, &self.path).await?;
        Ok(())
    }

    /// Append an op and return how many are now waiting.
    pub async fn push(&self, op: PendingOp) -> Result<usize, OutboxError> {
        let mut ops = self.read().await?;
        ops.push(op);
        self.write(&ops).await?;
        Ok(ops.len())
    }

    /// Drop the ops that have landed, keeping anything queued since. Takes ids
    /// rather than a count because an edit can arrive while a flush is in
    /// flight, and truncating by length would silently eat it.
    pub async fn forget(&self, landed: &[String]) -> Result<usize, OutboxError> {
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
