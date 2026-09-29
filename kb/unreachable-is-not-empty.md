---
id: home-ledger.unreachable-is-not-empty
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# An unreachable store never reads as an empty one

## Claim
`LocalStore::load` returns `StoreError::Unreachable` when the containing
directory is absent, and `Ok(None)` only when the directory exists.

## Why
An unmounted NAS share is indistinguishable from an empty one by file
existence alone. Reading it as empty would let the app offer to start a fresh
ledger over the top of real data.

## Check
```bash
cargo test -p ledger-store a_missing_directory_is_unreachable_not_empty
cargo test -p ledger-store an_unmounted_share_refuses_the_write_rather_than_faking_it
```

## Depends On
[[core-tests-pass]]
