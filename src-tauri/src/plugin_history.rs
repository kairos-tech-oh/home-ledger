use crate::audit::{self, Audit, Publish};
use crate::state::AppState;
use ledger_domain::Money;
use ledger_domain::records::{AuditChange, AuditEntry, caps};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct PluginLog {
    entries: Vec<Value>,
}

#[derive(Debug)]
pub struct Preview {
    pub entries: Vec<AuditEntry>,
    pub skipped: usize,
    pub new: usize,
    pub first: String,
    pub last: String,
    pub too_old: usize,
    pub over_cap: usize,
}

pub fn default_path() -> Option<PathBuf> {
    ledger_config::plugin_file("audit.json")
}

pub fn read(path: &Path, machine: &str) -> Result<(Vec<AuditEntry>, usize), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let log: PluginLog = serde_json::from_slice(&bytes)
        .map_err(|e| format!("{} is not a plugin history file: {e}", path.display()))?;
    let total = log.entries.len();
    let entries: Vec<AuditEntry> = log
        .entries
        .iter()
        .filter_map(|raw| entry(raw, machine))
        .collect();
    let skipped = total - entries.len();
    Ok((entries, skipped))
}

fn entry(raw: &Value, machine: &str) -> Option<AuditEntry> {
    let text = |key: &str, limit: usize| {
        ledger_domain::plain(raw.get(key).and_then(Value::as_str).unwrap_or(""), limit)
    };
    let id = text("id", 64);
    let at = text("at", 32);
    if !ledger_store::shelf_name_ok(&id) || at.len() < 19 {
        return None;
    }
    let changes = raw
        .get("changes")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .take(caps::CHANGES)
                .map(|c| {
                    let side = |key: &str| match c.get(key) {
                        Some(Value::String(s)) => ledger_domain::plain(s, 200),
                        Some(Value::Null) | None => String::new(),
                        Some(other) => ledger_domain::plain(&other.to_string(), 200),
                    };
                    AuditChange {
                        field: side("field"),
                        from: side("from"),
                        to: side("to"),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    Some(AuditEntry {
        id,
        at,
        action: text("action", 40),
        subject: text("subject", 40),
        name: text("name", 200),
        op: text("op", 40),
        actor: machine.to_string(),
        amount: raw
            .get("amount")
            .filter(|v| !v.is_null())
            .map(|v| Money::parse(v, Money::ZERO)),
        changes,
    })
}

pub fn preview(
    entries: Vec<AuditEntry>,
    skipped: usize,
    local: &[AuditEntry],
    cutoff: &str,
) -> Preview {
    let known: std::collections::HashSet<&str> = local.iter().map(|e| e.id.as_str()).collect();
    let new = entries
        .iter()
        .filter(|e| !known.contains(e.id.as_str()))
        .count();
    let first = entries
        .iter()
        .map(|e| e.at.clone())
        .min()
        .unwrap_or_default();
    let last = entries
        .iter()
        .map(|e| e.at.clone())
        .max()
        .unwrap_or_default();
    let too_old = entries.iter().filter(|e| e.at.as_str() < cutoff).count();
    let over_cap = (local.len() + new).saturating_sub(caps::AUDIT);
    Preview {
        entries,
        skipped,
        new,
        first,
        last,
        too_old,
        over_cap,
    }
}

pub async fn import(state: &AppState, preview: &Preview) -> Result<(usize, Publish), String> {
    if preview.too_old > 0 || preview.over_cap > 0 {
        return Err(format!(
            "refused: {} entries are older than the 60-day window and {} would exceed the {}-entry cap; \
             importing would silently drop them",
            preview.too_old,
            preview.over_cap,
            caps::AUDIT
        ));
    }
    let local = state.audit();
    let added = local
        .absorb(&preview.entries)
        .await
        .map_err(|e| format!("could not write local history: {e}"))?;
    let (primary, install) = state.primary_and_install().await;
    let published = audit::publish(primary.as_ref(), &install, &local.read().await)
        .await
        .map_err(|e| {
            format!("imported locally but not published yet (it will go on the next sync): {e}")
        })?;
    Ok((added, published))
}

pub async fn survey(
    places: &ledger_config::Places,
    file: &Path,
    machine: &str,
) -> Result<(Preview, String), String> {
    let (entries, skipped) = read(file, machine)?;
    let config = places.load_config().map_err(|e| e.to_string())?;
    let local = Audit::new(&places.data_dir).read().await;
    Ok((
        preview(entries, skipped, &local, &audit::window_start()),
        config.install,
    ))
}

pub fn run_cli(args: &[String]) -> i32 {
    let mut machine = None;
    let mut file = None;
    let mut dry_run = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--machine" => machine = rest.next().cloned(),
            "--file" => file = rest.next().map(PathBuf::from),
            "--dry-run" => dry_run = true,
            other => {
                eprintln!("unknown argument: {other}");
                return 2;
            }
        }
    }
    let Some(machine) = machine.as_deref().and_then(crate::state::machine_name) else {
        eprintln!(
            "usage: home-ledger import-plugin-history --machine <name> [--file <audit.json>] [--dry-run]"
        );
        return 2;
    };
    let Some(file) = file.or_else(default_path) else {
        eprintln!("no plugin history file given and no default location on this system");
        return 2;
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    runtime.block_on(async move {
        let places = match ledger_config::Places::discover() {
            Ok(places) => places,
            Err(e) => {
                eprintln!("cannot find this machine's ledger: {e}");
                return 1;
            }
        };
        let (preview, install) = match survey(&places, &file, &machine).await {
            Ok(surveyed) => surveyed,
            Err(e) => {
                eprintln!("{e}; nothing changed");
                return 1;
            }
        };
        println!("source      {}", file.display());
        println!("machine     {machine}");
        println!(
            "entries     {} readable, {} skipped",
            preview.entries.len(),
            preview.skipped
        );
        println!("date range  {} .. {}", preview.first, preview.last);
        println!(
            "new         {} (already here: {})",
            preview.new,
            preview.entries.len() - preview.new
        );
        println!(
            "window      starts {}; {} older",
            audit::window_start(),
            preview.too_old
        );
        if install.is_empty() {
            println!("install     not assigned yet; the first real run assigns one");
        } else {
            println!("install     {install}");
        }
        if dry_run {
            println!("dry run: nothing written");
            return 0;
        }
        let state = match AppState::headless() {
            Ok(state) => state,
            Err(e) => {
                eprintln!("cannot open this machine's ledger: {e}");
                return 1;
            }
        };
        match import(&state, &preview).await {
            Ok((added, published)) => {
                println!("imported    {added}");
                println!("published   {published:?}");
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_config::{Config, Places, Settings, StoreConfig};
    use std::sync::Arc;

    fn recent(days: i64) -> String {
        let t = time::OffsetDateTime::now_utc() - time::Duration::days(days);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            t.year(),
            t.month() as u8,
            t.day(),
            t.hour(),
            t.minute(),
            t.second()
        )
    }

    fn plugin_file(dir: &Path, ats: &[String]) -> PathBuf {
        let entries: Vec<Value> = ats
            .iter()
            .enumerate()
            .map(|(i, at)| {
                serde_json::json!({
                    "action": "Add", "subject": "savings", "name": format!("entry {i}"),
                    "amount": if i == 0 { Value::from(1391.99) } else { Value::Null },
                    "changes": [{"field": "Japan", "from": "1296.25", "to": "1766.07"}],
                    "op": "bucket-set", "id": format!("{i:032x}"), "at": at, "v": 1
                })
            })
            .collect();
        let path = dir.join("plugin-audit.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({"v": 1, "entries": entries})).unwrap(),
        )
        .unwrap();
        path
    }

    fn machine(root: &Path, name: &str, primary: &Path) -> AppState {
        let places = Places {
            config_file: root.join(name).join("config.json"),
            data_dir: root.join(name).join("data"),
        };
        places
            .save_config(&Config {
                stores: vec![StoreConfig {
                    id: "s".into(),
                    label: "s".into(),
                    settings: Settings::Local {
                        path: primary.to_path_buf(),
                    },
                    accept_risk: false,
                }],
                device: name.into(),
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

    #[test]
    fn plugin_entries_keep_their_id_and_time_and_take_the_typed_machine() {
        let dir = tempfile::tempdir().unwrap();
        let path = plugin_file(dir.path(), &[recent(1), recent(2)]);
        let (entries, skipped) = read(&path, "Laptop").unwrap();
        assert_eq!((entries.len(), skipped), (2, 0));
        assert_eq!(entries[0].id, format!("{:032x}", 0));
        assert_eq!(entries[0].at, recent(1)[..19].to_string() + "Z");
        assert!(entries.iter().all(|e| e.actor == "Laptop"));
        assert_eq!(
            entries[0].amount,
            Some(Money::parse(&Value::from(1391.99), Money::ZERO))
        );
        assert_eq!(entries[1].amount, None);
        assert_eq!(entries[0].changes[0].to, "1766.07");
    }

    #[test]
    fn a_damaged_or_missing_file_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugin-audit.json");
        assert!(read(&path, "Laptop").is_err());
        std::fs::write(&path, b"{not json").unwrap();
        assert!(read(&path, "Laptop").is_err());
    }

    #[tokio::test]
    async fn an_import_publishes_once_and_a_rerun_adds_nothing() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let laptop = machine(root.path(), "a", &shared.join("ledger.json"));
        let pc = machine(root.path(), "b", &shared.join("ledger.json"));
        let path = plugin_file(root.path(), &[recent(1), recent(3), recent(5)]);
        let before = std::fs::read(&path).unwrap();

        for expected in [3, 0] {
            let (entries, skipped) = read(&path, "Laptop").unwrap();
            let local = Audit::new(&laptop.places.data_dir).read().await;
            let preview = preview(entries, skipped, &local, &audit::window_start());
            assert_eq!(preview.new, expected);
            let (added, _) = import(&laptop, &preview).await.unwrap();
            assert_eq!(added, expected);
        }

        let seen = crate::views::history_of(&pc).await;
        assert_eq!(seen.entries.len(), 3);
        assert!(seen.entries.iter().all(|e| e.actor == "Laptop"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[tokio::test]
    async fn entries_the_window_would_drop_are_refused_not_trimmed() {
        let root = tempfile::tempdir().unwrap();
        let shared = root.path().join("shared");
        std::fs::create_dir_all(&shared).unwrap();
        let laptop = machine(root.path(), "a", &shared.join("ledger.json"));
        let path = plugin_file(root.path(), &[recent(1), recent(90)]);

        let (entries, skipped) = read(&path, "Laptop").unwrap();
        let preview = preview(entries, skipped, &[], &audit::window_start());
        assert_eq!(preview.too_old, 1);
        assert!(import(&laptop, &preview).await.is_err());
        assert!(Audit::new(&laptop.places.data_dir).read().await.is_empty());
    }

    #[tokio::test]
    async fn a_preview_writes_nothing_on_a_fresh_or_unmigrated_machine() {
        let root = tempfile::tempdir().unwrap();
        let path = plugin_file(root.path(), &[recent(1)]);
        let fresh = Places {
            config_file: root.path().join("fresh/config.json"),
            data_dir: root.path().join("fresh/data"),
        };
        let (preview, install) = survey(&fresh, &path, "Laptop").await.unwrap();
        assert_eq!((preview.new, install.as_str()), (1, ""));
        assert!(!root.path().join("fresh").exists());

        let old = Places {
            config_file: root.path().join("old/config.json"),
            data_dir: root.path().join("old/data"),
        };
        std::fs::create_dir_all(root.path().join("old")).unwrap();
        let before = br#"{"v":1,"stores":[],"device":""}"#;
        std::fs::write(&old.config_file, before).unwrap();
        survey(&old, &path, "Laptop").await.unwrap();
        assert_eq!(std::fs::read(&old.config_file).unwrap(), before);
        assert!(!old.data_dir.exists());
    }
}
