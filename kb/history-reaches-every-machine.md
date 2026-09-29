---
id: home-ledger.history-reaches-every-machine
project: home-ledger
category: data
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass, home-ledger.every-edit-names-its-machine, home-ledger.only-the-primary-is-written]
---

# An edit on one machine shows in every machine's history

## Claim
Each install publishes its history to its own object on the primary under a
conditional write, and every install's History shows the union of all objects
by entry id, with no entry lost to a concurrent publish, even from a separate
store instance or process.

## Why
The person reading history on the PC needs to see what the laptop changed.
One writer per object means there is nothing to merge, so nothing can be lost.

## Check
```bash
cargo test -p home-ledger an_edit_on_one_machine_shows_in_the_others_history
cargo test -p home-ledger two_machines_publishing_at_once_lose_nothing
cargo test -p home-ledger one_install_racing_itself_keeps_every_entry
cargo test -p home-ledger separate_store_instances_keep_both_publishes
cargo test -p ledger-store two_instances_creating_at_once_have_one_winner
cargo test -p home-ledger a_write_that_lands_first_is_merged_not_overwritten
cargo test -p home-ledger existing_local_history_is_published_once
cargo test -p home-ledger published_and_merged_history_stay_within_the_caps
cargo test -p home-ledger a_store_without_a_shelf_keeps_history_local
cargo test -p ledger-store a_shelf_lists_what_its_slots_hold
cargo test -p ledger-store a_listing_keeps_only_well_formed_names_under_the_prefix
```

## Depends On
[[core-tests-pass]]
[[every-edit-names-its-machine]]
[[only-the-primary-is-written]]
