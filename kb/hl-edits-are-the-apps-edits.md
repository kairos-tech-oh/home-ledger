---
id: home-ledger.hl-edits-are-the-apps-edits
project: home-ledger
category: correctness
severity: error
environment: any
depends_on: []
---

# The command line changes the ledger only the way the app does, and says so

## Claim
Every `hl` edit is built as the same op the app sends and applied by the same
writer, so the same rules refuse it, and with `--dry-run` it is applied to a
copy and nothing is queued or written. Each one is recorded in the change
history with the machine's name, `client: cli`, the install's id, the
version, and any `--via` label. `hl` shares the app's setup on a machine
that has it; `HOME_LEDGER_DIR` points it at a separate one. It exits 3 when
locked, 4 when an edit is queued unsynced, 5 when a rule refuses, and stops
quietly when its output pipe closes. The outbox it shares with the app is
locked across processes.

## Why
A script that could do what the app cannot, or that changed the ledger without
saying who, would make the history untrustworthy. One edit path, one set of
rules, one record.

## Check
```bash
cargo test -p hl
cargo test -p ledger-store two_programs_queueing_at_once_lose_no_edit
grep -q 'ledger_app::commands::apply_value(&self.state, op)' crates/hl/src/session.rs
grep -q 'ledger_app::commands::dry_run(&self.state, op)' crates/hl/src/session.rs
grep -q 'client: client.name().into(),' crates/ledger-app/src/state.rs
grep -q 'HOME_LEDGER_DIR' crates/ledger-config/src/lib.rs
