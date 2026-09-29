use ledger_domain::records::{AuditEntry, caps};
use ledger_store::{Expect, Store, StoreError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Entries older than this are dropped. Long enough to answer "what happened
/// to this account", short enough that the file stays small.
const KEEP_DAYS: u64 = 60;

pub struct Audit {
    path: PathBuf,
}

impl Audit {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("audit.json"),
        }
    }

    pub async fn read(&self) -> Vec<AuditEntry> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            // No history yet is not a fault; it is a ledger nobody has edited.
            Err(_) => Vec::new(),
        }
    }

    pub async fn absorb(&self, incoming: &[AuditEntry]) -> std::io::Result<usize> {
        let entries = self.read().await;
        let before = entries.len();
        let merged = union([entries.as_slice(), incoming]);
        let added = merged.len() - before;
        if added > 0 {
            self.write(&merged).await?;
        }
        Ok(added)
    }

    async fn write(&self, entries: &[AuditEntry]) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let body = serde_json::to_vec(entries)?;
        let temp = self
            .path
            .with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
        tokio::fs::write(&temp, &body).await?;
        tokio::fs::rename(&temp, &self.path).await?;
        Ok(())
    }

    /// Append one entry, oldest first, trimmed to the caps.
    ///
    /// A damaged audit file must never block a real edit — it is not the
    /// record of record — so an unreadable one is started again rather than
    /// refused.
    pub async fn append(&self, entry: AuditEntry) -> std::io::Result<()> {
        let mut entries = self.read().await;
        entries.push(entry);
        self.write(&trim(entries, &now_iso())).await
    }
}

const FOLDER: &str = "history";
const PUBLISH_ATTEMPTS: usize = 4;

#[derive(Serialize, Deserialize)]
struct Published {
    v: u32,
    install: String,
    entries: Vec<AuditEntry>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Publish {
    Landed(usize),
    UpToDate,
    NotShared,
}

pub struct Gathered {
    pub entries: Vec<AuditEntry>,
    pub problems: Vec<String>,
    pub shared: bool,
}

pub async fn publish(
    primary: &dyn Store,
    install: &str,
    local: &[AuditEntry],
) -> Result<Publish, StoreError> {
    let Some(slot) = primary.shelf(FOLDER).and_then(|shelf| shelf.slot(install)) else {
        return Ok(Publish::NotShared);
    };
    for _ in 0..PUBLISH_ATTEMPTS {
        let (remote, expect) = match slot.load().await? {
            Some(snapshot) => {
                let remote = parse(&snapshot.body).unwrap_or_else(|| {
                    tracing::warn!("this machine's published history was unreadable; replacing it");
                    Vec::new()
                });
                (remote, Expect::Version(snapshot.version))
            }
            None => (Vec::new(), Expect::Absent),
        };
        let merged = trim(union([remote.as_slice(), local]), &now_iso());
        if ids(&merged) == ids(&remote) {
            return Ok(Publish::UpToDate);
        }
        let count = merged.len();
        let body = serde_json::to_vec(&Published {
            v: 1,
            install: install.to_string(),
            entries: merged,
        })
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
        match slot.save(&body, expect).await {
            Ok(_) => return Ok(Publish::Landed(count)),
            Err(StoreError::Conflict) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(StoreError::Conflict)
}

pub async fn share(primary: Arc<dyn Store>, install: String, data_dir: PathBuf) {
    let local = Audit::new(&data_dir).read().await;
    match publish(primary.as_ref(), &install, &local).await {
        Ok(Publish::Landed(count)) => tracing::info!(count, "history published"),
        Ok(_) => {}
        Err(e) => tracing::info!(error = %e, "history not published yet; it stays queued here"),
    }
}

pub async fn gather(primary: &dyn Store, install: &str, local: &[AuditEntry]) -> Gathered {
    let mut problems = Vec::new();
    let Some(shelf) = primary.shelf(FOLDER) else {
        return Gathered {
            entries: local.to_vec(),
            problems,
            shared: false,
        };
    };
    let names = match shelf.names().await {
        Ok(names) => names,
        Err(e) => {
            problems.push(format!(
                "Only this machine's history is shown: the source of truth could not be read ({e})."
            ));
            return Gathered {
                entries: trim(union([local]), &now_iso()),
                problems,
                shared: true,
            };
        }
    };

    let mut found: Vec<Vec<AuditEntry>> = vec![local.to_vec()];
    for name in names {
        let Some(slot) = shelf.slot(&name) else {
            continue;
        };
        match slot.load().await {
            Ok(Some(snapshot)) => match parse(&snapshot.body) {
                Some(entries) => found.push(entries),
                None if name == install => {}
                None => problems.push(format!(
                    "History from one machine ({name}) could not be read and was skipped."
                )),
            },
            Ok(None) => {}
            Err(e) => problems.push(format!(
                "History from one machine ({name}) could not be read and was skipped: {e}"
            )),
        }
    }
    let lists: Vec<&[AuditEntry]> = found.iter().map(Vec::as_slice).collect();
    Gathered {
        entries: trim(union(lists), &now_iso()),
        problems,
        shared: true,
    }
}

fn parse(body: &[u8]) -> Option<Vec<AuditEntry>> {
    serde_json::from_slice::<Published>(body)
        .ok()
        .map(|p| p.entries)
}

fn union<'a>(lists: impl IntoIterator<Item = &'a [AuditEntry]>) -> Vec<AuditEntry> {
    let mut by_id: BTreeMap<&str, &AuditEntry> = BTreeMap::new();
    for list in lists {
        for entry in list {
            by_id.entry(entry.id.as_str()).or_insert(entry);
        }
    }
    let mut out: Vec<AuditEntry> = by_id.into_values().cloned().collect();
    out.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
    out
}

fn ids(entries: &[AuditEntry]) -> Vec<&str> {
    let mut ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    ids.sort();
    ids
}

/// Drop what is too old or too many, newest kept.
fn trim(mut entries: Vec<AuditEntry>, now: &str) -> Vec<AuditEntry> {
    let cutoff = days_before(now, KEEP_DAYS);
    // Lexicographic works because the timestamps are fixed-width UTC.
    entries.retain(|e| e.at.is_empty() || e.at.as_str() >= cutoff.as_str());
    if entries.len() > caps::AUDIT {
        entries.drain(0..entries.len() - caps::AUDIT);
    }
    entries
}

pub fn window_start() -> String {
    days_before(&now_iso(), KEEP_DAYS)
}

fn now_iso() -> String {
    ledger_writer::now_iso()
}

/// The same instant, a number of days earlier, as an ISO timestamp.
fn days_before(now: &str, days: u64) -> String {
    let parsed = parse_epoch(now);
    let earlier = parsed.saturating_sub(days * 86_400);
    format_epoch(earlier)
}

fn parse_epoch(iso: &str) -> u64 {
    // yyyy-mm-ddThh:mm:ssZ
    let bytes = iso.as_bytes();
    if bytes.len() < 19 {
        return 0;
    }
    let num = |from: usize, to: usize| -> i64 { iso[from..to].parse().unwrap_or(0) };
    let (y, m, d) = (num(0, 4), num(5, 7), num(8, 10));
    let (hh, mm, ss) = (num(11, 13), num(14, 16), num(17, 19));

    let days = days_from_civil(y, m, d);
    (days * 86_400 + hh * 3_600 + mm * 60 + ss).max(0) as u64
}

fn format_epoch(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: &str) -> AuditEntry {
        AuditEntry {
            id: ledger_domain::new_id(),
            at: at.into(),
            name: at.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_date_survives_a_round_trip_through_the_epoch() {
        for iso in [
            "2026-09-21T18:30:45Z",
            "2024-02-29T00:00:00Z",
            "1970-01-01T00:00:00Z",
        ] {
            assert_eq!(format_epoch(parse_epoch(iso)), iso, "{iso}");
        }
    }

    #[test]
    fn sixty_days_earlier_crosses_a_month_boundary_correctly() {
        assert_eq!(
            days_before("2026-03-01T00:00:00Z", 60),
            "2025-12-31T00:00:00Z"
        );
    }

    #[test]
    fn entries_older_than_the_window_are_dropped() {
        let kept = trim(
            vec![entry("2026-01-01T00:00:00Z"), entry("2026-09-20T00:00:00Z")],
            "2026-09-21T00:00:00Z",
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].name, "2026-09-20T00:00:00Z");
    }

    #[test]
    fn the_newest_are_kept_when_there_are_too_many() {
        let entries: Vec<AuditEntry> = (0..caps::AUDIT + 50)
            .map(|_| entry("2026-09-21T00:00:00Z"))
            .collect();
        let kept = trim(entries, "2026-09-21T00:00:00Z");
        assert_eq!(kept.len(), caps::AUDIT);
    }

    #[test]
    fn an_entry_with_no_timestamp_is_kept_rather_than_silently_dropped() {
        let kept = trim(vec![entry("")], "2026-09-21T00:00:00Z");
        assert_eq!(kept.len(), 1);
    }

    #[tokio::test]
    async fn an_unreadable_file_does_not_block_writing_history() {
        let dir = tempfile::tempdir().unwrap();
        let audit = Audit::new(dir.path());
        std::fs::write(dir.path().join("audit.json"), b"not json").unwrap();

        audit.append(entry("2026-09-21T00:00:00Z")).await.unwrap();
        assert_eq!(audit.read().await.len(), 1);
    }

    #[tokio::test]
    async fn entries_come_back_in_the_order_they_happened() {
        let dir = tempfile::tempdir().unwrap();
        let audit = Audit::new(dir.path());
        audit.append(entry("2026-09-20T00:00:00Z")).await.unwrap();
        audit.append(entry("2026-09-21T00:00:00Z")).await.unwrap();

        let entries = audit.read().await;
        assert_eq!(entries[0].name, "2026-09-20T00:00:00Z");
        assert_eq!(entries[1].name, "2026-09-21T00:00:00Z");
    }
}
