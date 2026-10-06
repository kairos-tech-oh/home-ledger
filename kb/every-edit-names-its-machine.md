---
id: home-ledger.every-edit-names-its-machine
project: home-ledger
category: data
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass, home-ledger.no-personal-identity-in-data]
---

# Every history entry names the machine that made it

## Claim
Each new entry's `actor` is the settings "Machine name" from the next edit
on, with no restart, and a blank name is refused in the UI and in Rust.

## Why
History is read to answer "which machine did this". A blank name answers
nothing, and a stale one answers wrongly until the app restarts.

## Check
```bash
cargo test -p ledger-app a_blank_machine_name_is_refused
cargo test -p ledger-app a_machine_name_is_trimmed_and_capped
cargo test -p ledger-app a_renamed_machine_signs_the_next_edit
grep -q 'Machine name' ui/src/Storage.svelte
grep -q 'entry.actor' ui/src/History.svelte
```

## Depends On
[[core-tests-pass]]
[[no-personal-identity-in-data]]
