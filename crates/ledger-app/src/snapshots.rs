//! Daily snapshots: where they live, and how every machine sees every other's.
//!
//! Kept in `snapshots.json` beside the local ledger, in the plugin's own shape
//! (`{"v":1,"points":[...]}`), so a file copied from the plugin can be read as
//! it is. Shared the way the change history is: each install publishes its
//! points to its own slot on the source of truth, and reads everyone else's
//! back into its file. Points are dated records, so the union is always safe.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use ledger_domain::Ledger;
use ledger_math::calendar::Day;
use ledger_math::snapshots::{self, Point};
use ledger_store::{Expect, Store, StoreError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const FOLDER: &str = "snapshots";
const PUBLISH_ATTEMPTS: usize = 4;
/// A plugin file can hold ten years of points; anything far past that is not one.
const READ_CAP: u64 = 8 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Stored {
    v: u32,
    points: Vec<Value>,
}

/// Points out of a stored file, one bad point dropped rather than the lot.
fn parse(body: &[u8]) -> Option<Vec<Point>> {
    let stored: Stored = serde_json::from_slice(body).ok()?;
    let points: Vec<Point> = stored
        .points
        .into_iter()
        .filter_map(|raw| serde_json::from_value(raw).ok())
        .collect();
    Some(snapshots::merge([points.as_slice()]))
}

fn body_of(points: &[Point]) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(&serde_json::json!({ "v": 1, "points": points }))
}

pub struct LocalPoints {
    path: PathBuf,
    vault: Arc<ledger_store::Vault>,
}

impl LocalPoints {
    /// Unsealed: for tests.
    pub fn new(data_dir: &Path) -> Self {
        Self::sealed(data_dir, ledger_store::Vault::new())
    }

    pub fn sealed(data_dir: &Path, vault: Arc<ledger_store::Vault>) -> Self {
        Self {
            path: data_dir.join("snapshots.json"),
            vault,
        }
    }

    /// History is valuable but it is not the record of record: a damaged or
    /// locked file reads as empty for showing.
    pub async fn read(&self) -> Vec<Point> {
        self.read_strict().await.unwrap_or_default()
    }

    /// For changing: refused when locked, so the file is never overwritten
    /// unread.
    async fn read_strict(&self) -> std::io::Result<Vec<Point>> {
        Ok(
            match crate::sealed_file::read(&self.path, &self.vault).await? {
                Some(plain) => parse(&plain).unwrap_or_default(),
                None => Vec::new(),
            },
        )
    }

    pub async fn reseal(&self) -> std::io::Result<()> {
        if tokio::fs::metadata(&self.path).await.is_ok() {
            let held = self.read_strict().await?;
            self.write(&held).await?;
        }
        Ok(())
    }

    /// Adds points, keeping the later of two for a day. Returns how many days
    /// were not there before.
    pub async fn absorb(&self, incoming: &[Point]) -> std::io::Result<usize> {
        let held = self.read_strict().await?;
        let merged = snapshots::merge([held.as_slice(), incoming]);
        let added = merged
            .iter()
            .filter(|p| !held.iter().any(|h| h.at == p.at))
            .count();
        if merged != held {
            self.write(&merged).await?;
        }
        Ok(added)
    }

    async fn write(&self, points: &[Point]) -> std::io::Result<()> {
        let body = body_of(points)?;
        crate::sealed_file::write(&self.path, &self.vault, &body).await
    }
}

/// Put this machine's points in its slot, then read every machine's back.
/// Never an error to the caller: history failing to share must not turn a
/// good launch into a bad one.
pub async fn share(
    primary: Arc<dyn Store>,
    install: String,
    data_dir: PathBuf,
    vault: Arc<ledger_store::Vault>,
) {
    let local = LocalPoints::sealed(&data_dir, vault);
    let Some(shelf) = primary.shelf(FOLDER) else {
        return;
    };
    if let Some(slot) = shelf.slot(&install) {
        match publish(slot.as_ref(), &local.read().await).await {
            Ok(()) => {}
            Err(e) => tracing::info!(error = %e, "snapshots not published yet; they stay here"),
        }
    }
    let names = match shelf.names().await {
        Ok(names) => names,
        Err(e) => {
            tracing::info!(error = %e, "other machines' snapshots could not be listed");
            return;
        }
    };
    for name in names.into_iter().filter(|n| *n != install) {
        let Some(slot) = shelf.slot(&name) else {
            continue;
        };
        match slot.load().await {
            Ok(Some(found)) => match parse(&found.body) {
                Some(points) => {
                    if let Err(e) = local.absorb(&points).await {
                        tracing::warn!(error = %e, "could not keep another machine's snapshots");
                    }
                }
                None => tracing::info!(machine = %name, "skipped unreadable snapshots"),
            },
            Ok(None) => {}
            Err(e) => tracing::info!(machine = %name, error = %e, "skipped snapshots"),
        }
    }
}

async fn publish(slot: &dyn Store, local: &[Point]) -> Result<(), StoreError> {
    for _ in 0..PUBLISH_ATTEMPTS {
        let (remote, expect) = match slot.load().await? {
            Some(found) => (
                parse(&found.body).unwrap_or_default(),
                Expect::Version(found.version),
            ),
            None => (Vec::new(), Expect::Absent),
        };
        let merged = snapshots::merge([remote.as_slice(), local]);
        if merged == remote {
            return Ok(());
        }
        let body = body_of(&merged).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        match slot.save(&body, expect).await {
            Ok(_) => return Ok(()),
            Err(StoreError::Conflict) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(StoreError::Conflict)
}

async fn share_in_background(state: &AppState) {
    let (primary, install) = state.primary_and_install().await;
    state.spawn(share(
        primary,
        install,
        state.places.data_dir.clone(),
        state.vault.clone(),
    ));
}

/// Take today's point if there is not one yet, then share. `today` is the
/// person's own calendar day, from the window.
///
/// Not from a stale copy: a mirror read while the source of truth was away
/// may be behind, and a point taken from it would say so for good.
pub async fn take_snapshot(state: &AppState, today: String) -> Answer<bool> {
    let Some(day) = Day::parse(&today) else {
        return Err(CommandError::Message("today must be yyyy-mm-dd".into()));
    };
    let local = state.points();
    let held = local.read().await;
    let mut took = false;
    if !held.iter().any(|p| p.at == day.iso()) {
        let loaded = state.live().await.engine.load().await?;
        if !loaded.stale {
            let doc = match &loaded.snapshot {
                Some(s) => ledger_writer::read(&s.body)?,
                None => Ledger::default(),
            };
            let point = snapshots::build(&doc, day, &ledger_writer::now_iso());
            local
                .absorb(&[point])
                .await
                .map_err(|e| CommandError::Message(e.to_string()))?;
            took = true;
        }
    }
    share_in_background(state).await;
    Ok(took)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotImport {
    pub path: String,
    /// Points in the file that could be read.
    pub points: usize,
    /// Days this machine did not have yet.
    pub new: usize,
    pub first: String,
    pub last: String,
    /// Days already held here, where the later of the two is kept.
    pub overlap: usize,
}

/// Where the plugin keeps its snapshots on this machine, if it ever ran here.
pub async fn plugin_snapshots_path() -> Answer<Option<String>> {
    Ok(ledger_config::plugin_file("snapshots.json")
        .filter(|p| p.is_file())
        .map(|p| p.display().to_string()))
}

fn read_file(path: &str) -> Answer<Vec<Point>> {
    let path = Path::new(path.trim());
    let size = std::fs::metadata(path)
        .map_err(|e| CommandError::Message(format!("cannot read {}: {e}", path.display())))?
        .len();
    if size > READ_CAP {
        return Err(CommandError::Message(format!(
            "{} is too large to be a snapshots file",
            path.display()
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| CommandError::Message(format!("cannot read {}: {e}", path.display())))?;
    parse(&bytes).ok_or_else(|| {
        CommandError::Message(format!(
            "{} is not a snapshots file: it should hold {{\"v\":1,\"points\":[...]}}",
            path.display()
        ))
    })
}

/// What importing a file would add, without adding it.
pub async fn snapshot_import_preview(state: &AppState, path: String) -> Answer<SnapshotImport> {
    let incoming = read_file(&path)?;
    let held = state.points().read().await;
    let overlap = incoming
        .iter()
        .filter(|p| held.iter().any(|h| h.at == p.at))
        .count();
    Ok(SnapshotImport {
        path: path.trim().to_string(),
        points: incoming.len(),
        new: incoming.len() - overlap,
        first: incoming.first().map(|p| p.at.clone()).unwrap_or_default(),
        last: incoming.last().map(|p| p.at.clone()).unwrap_or_default(),
        overlap,
    })
}

/// Adds a file's points to this machine's, then shares them. Importing the
/// same file twice adds nothing the second time.
pub async fn snapshot_import(state: &AppState, path: String) -> Answer<usize> {
    let incoming = read_file(&path)?;
    let added = state
        .points()
        .absorb(&incoming)
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?;
    share_in_background(state).await;
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::Money;

    fn point(at: &str, net: i64, taken: &str) -> Point {
        Point {
            at: at.into(),
            taken_at: taken.into(),
            net: Money::from(net),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn absorbing_keeps_one_point_a_day_and_counts_only_new_days() {
        let dir = tempfile::tempdir().unwrap();
        let local = LocalPoints::new(dir.path());
        let first = [point("2026-09-01", 1, "2026-09-01T08:00:00Z")];
        assert_eq!(local.absorb(&first).await.unwrap(), 1);
        assert_eq!(local.absorb(&first).await.unwrap(), 0);
        let later = [
            point("2026-09-01", 2, "2026-09-01T20:00:00Z"),
            point("2026-09-02", 3, "2026-09-02T08:00:00Z"),
        ];
        assert_eq!(local.absorb(&later).await.unwrap(), 1);
        let held = local.read().await;
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].net, Money::from(2));
    }

    #[test]
    fn a_plugin_file_reads_as_it_is_and_one_bad_point_does_not_spoil_it() {
        let body = br#"{"v":1,"points":[
            {"at":"2026-09-02","net":2},
            {"at":"not a date","net":9},
            {"at":"2026-09-01","net":"garbage"},
            {"at":"2026-09-01","takenAt":"2026-09-01T10:00:00Z","net":1.5,"buckets":[]}
        ]}"#;
        let points = parse(body).unwrap();
        let days: Vec<_> = points.iter().map(|p| p.at.as_str()).collect();
        assert_eq!(days, ["2026-09-01", "2026-09-02"]);
        assert!(parse(b"[1,2,3]").is_none());
    }

    #[test]
    fn what_is_written_reads_back_the_same() {
        let points = vec![point("2026-09-01", 5, "2026-09-01T08:00:00Z")];
        assert_eq!(parse(&body_of(&points).unwrap()).unwrap(), points);
    }
}
