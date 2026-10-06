---
id: home-ledger.history-records-what-changed
project: home-ledger
category: data
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Every applied edit is recorded, and a damaged log never blocks one

## Claim
`Audit::append` writes each entry atomically within sixty days and five hundred
entries, starts a fresh log rather than refusing an unreadable one, and an edit
still applies when its history cannot be published.

## Why
The audit log is what someone reads when a sync went somewhere they did not
expect, so an edit that is not recorded is a gap exactly where it matters.
It is not the record of record, though — the ledger is — so a damaged log must
never be a reason to refuse a real edit.

## Check
```bash
cargo test -p ledger-app an_unreadable_file_does_not_block_writing_history
cargo test -p ledger-app entries_older_than_the_window_are_dropped
cargo test -p ledger-app the_newest_are_kept_when_there_are_too_many
cargo test -p ledger-app a_date_survives_a_round_trip_through_the_epoch
cargo test -p ledger-app an_unreachable_primary_queues_history_and_never_writes_a_mirror
cargo test -p ledger-app a_damaged_history_from_another_machine_is_skipped_and_reported
```

## Depends On
[[core-tests-pass]]
