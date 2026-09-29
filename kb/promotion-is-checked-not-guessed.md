---
id: home-ledger.promotion-is-checked-not-guessed
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# One copy of the ledger can prove where it stands against another

## Claim
Every write advances a lineage carried in the document, and comparing two
copies returns ahead, behind, same, forked, or too far apart — never a guess.

## Why
Promoting a backup to source of truth is otherwise "hope this one was
current". Two copies that both moved on must be reported as forked and shown
to a person, because merging two whole documents has no general answer. A
bare generation counter is not enough: two forked copies reach the same
number.

## Check
```bash
cargo test -p ledger-domain lineage::
cargo test -p ledger-writer two_stores_that_both_moved_on_are_reported_as_forked
cargo test -p ledger-writer a_backup_left_behind_is_recognised_as_safe_to_fast_forward
cargo test -p ledger-store every_write_advances_the_lineage
```

## Depends On
[[core-tests-pass]]
