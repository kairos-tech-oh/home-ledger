---
id: home-ledger.plugin-history-imports-once
project: home-ledger
category: data
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass, home-ledger.history-reaches-every-machine]
---

# The plugin's history imports once, labelled, and never silently trimmed

## Claim
`import-plugin-history` keeps each plugin entry's id and time, labels it with
the typed machine name, publishes it to the primary, adds nothing on a re-run,
refuses rather than trims anything the retention window would drop, and a
dry run writes nothing at all.

## Why
The plugin is being retired, and this is its history's only way into the app.
An import that duplicates or quietly drops entries corrupts the record it rescues.

## Check
```bash
cargo test -p home-ledger plugin_entries_keep_their_id_and_time_and_take_the_typed_machine
cargo test -p home-ledger a_damaged_or_missing_file_is_reported
cargo test -p home-ledger an_import_publishes_once_and_a_rerun_adds_nothing
cargo test -p home-ledger entries_the_window_would_drop_are_refused_not_trimmed
cargo test -p home-ledger a_preview_writes_nothing_on_a_fresh_or_unmigrated_machine
```

## Depends On
[[core-tests-pass]]
[[history-reaches-every-machine]]
