//! One run of `hl`: the app's state, the flags every command honours, and the
//! one path every edit takes, so a dry run, `--via` and exit codes behave the
//! same for all of them.

use crate::out::{Failure, Outcome};
use ledger_app::views::LedgerView;
use ledger_app::{AppState, Client};
use serde_json::Value;

pub struct Session {
    pub state: AppState,
    pub json: bool,
    pub dry_run: bool,
}

impl Session {
    pub fn open(via: Option<String>, json: bool, dry_run: bool) -> Result<Session, Failure> {
        let state = AppState::headless(Client::Cli {
            via: via.unwrap_or_default(),
        })
        .map_err(|e| Failure::Error(format!("could not open the ledger's settings: {e}")))?;
        Ok(Session {
            state,
            json,
            dry_run,
        })
    }

    /// Refuses to work on a machine that has not been set up, rather than
    /// quietly using an empty local ledger.
    pub async fn ready(&self) -> Outcome {
        if !self.state.config().await.setup_complete {
            return Err(Failure::Usage(
                "this machine is not set up yet: run `hl init`, or set up the Home Ledger app"
                    .into(),
            ));
        }
        Ok(())
    }

    pub async fn view(&self) -> Result<LedgerView, Failure> {
        ledger_app::views::ledger(&self.state)
            .await
            .map_err(Failure::from_app)
    }

    /// Today on this machine's calendar, and how far its clock is from UTC.
    pub fn today() -> (String, i64) {
        let now = chrono::Local::now();
        (
            now.format("%Y-%m-%d").to_string(),
            i64::from(now.offset().local_minus_utc()) / 60,
        )
    }

    /// Makes one edit, or with `--dry-run` shows what it would do. Every edit
    /// `hl` makes comes through here, so it is recorded in the history the
    /// same way the app's are, signed with this machine, `cli` and `--via`.
    pub async fn edit(&self, op: Value) -> Outcome {
        if self.dry_run {
            let entry = ledger_app::commands::dry_run(&self.state, op)
                .await
                .map_err(refusal)?;
            match entry {
                None => self.say("nothing to change"),
                Some(entry) => {
                    if self.json {
                        println!("{}", serde_json::to_string_pretty(&entry).unwrap());
                    } else {
                        println!("would {}", describe(&entry));
                    }
                }
            }
            return Ok(());
        }
        let applied = ledger_app::commands::apply_value(&self.state, op)
            .await
            .map_err(refusal)?;
        self.state.finish_background().await;
        let Some(applied) = applied else {
            self.say("nothing to change");
            return Ok(());
        };
        if self.json {
            println!("{}", serde_json::to_string_pretty(&applied).unwrap());
        } else {
            println!("{}", describe(&applied.entry));
        }
        match applied.sync {
            ledger_store::SyncState::Behind { reason, .. } => Err(Failure::Queued(reason)),
            ledger_store::SyncState::Blocked { reason, .. } => Err(Failure::Queued(reason)),
            _ => Ok(()),
        }
    }

    fn say(&self, text: &str) {
        if self.json {
            println!(
                "{}",
                serde_json::json!({ "changed": false, "message": text })
            );
        } else {
            println!("{text}");
        }
    }
}

/// A refused edit is told apart from a locked ledger and from other failures.
fn refusal(e: impl std::fmt::Display) -> Failure {
    match Failure::from_app(&e) {
        Failure::Locked => Failure::Locked,
        _ => Failure::Refused(e.to_string()),
    }
}

/// "Update bucket Groceries: currentTotal 120.00 → 0.00".
pub fn describe(entry: &ledger_domain::records::AuditEntry) -> String {
    let mut out = format!("{} {} {}", entry.action, entry.subject, entry.name);
    if let Some(amount) = entry.amount {
        out.push_str(&format!(" ({})", crate::out::money(&amount.to_string())));
    }
    for c in &entry.changes {
        out.push_str(&format!("\n  {}: {} → {}", c.field, c.from, c.to));
    }
    out
}

/// One thing by its name (any case, but only if one matches) or its id.
pub fn find<'a, T>(
    items: &'a [T],
    wanted: &str,
    what: &str,
    id: impl Fn(&T) -> &str,
    name: impl Fn(&T) -> &str,
) -> Result<&'a T, Failure> {
    if let Some(found) = items.iter().find(|i| id(i) == wanted) {
        return Ok(found);
    }
    let matches: Vec<&T> = items
        .iter()
        .filter(|i| name(i).trim().eq_ignore_ascii_case(wanted.trim()))
        .collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(Failure::Usage(format!("no {what} called \"{wanted}\""))),
        many => Err(Failure::Usage(format!(
            "more than one {what} is called \"{wanted}\"; use its id: {}",
            many.iter().map(|i| id(i)).collect::<Vec<_>>().join(", ")
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Thing(&'static str, &'static str);

    fn id(t: &Thing) -> &str {
        t.0
    }

    fn name(t: &Thing) -> &str {
        t.1
    }

    #[test]
    fn things_are_found_by_id_or_by_one_name_in_any_case() {
        let things = [
            Thing("a1", "Groceries"),
            Thing("b2", "Travel"),
            Thing("c3", "travel"),
        ];
        assert_eq!(
            find(&things, "groceries", "bucket", id, name).unwrap().0,
            "a1"
        );
        assert_eq!(find(&things, "b2", "bucket", id, name).unwrap().0, "b2");
        let two = find(&things, "TRAVEL", "bucket", id, name)
            .unwrap_err()
            .to_string();
        assert!(two.contains("b2") && two.contains("c3"), "{two}");
        assert!(find(&things, "Gifts", "bucket", id, name).is_err());
    }
}
