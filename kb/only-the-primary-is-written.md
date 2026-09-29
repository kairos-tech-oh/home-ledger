---
id: home-ledger.only-the-primary-is-written
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Edits survive an unreachable primary and are never written elsewhere

## Claim
With the primary unreachable, `Engine::flush` reports `Behind` and leaves the
queued ops in the outbox; a blocked replay also leaves them queued, and
shared history is published to the primary only, never to a mirror.

## Why
This is the whole fallback design. Edits must not be lost when a store is
down, and must not be written to a mirror instead — two writable copies
diverge, and merging two whole documents is not solvable.

## Check
```bash
cargo test -p ledger-store edits_survive_the_primary_being_unreachable
cargo test -p ledger-store a_refused_op_blocks_rather_than_dropping_the_queue
cargo test -p ledger-store a_store_that_cannot_lock_is_refused_as_primary
cargo test -p home-ledger an_unreachable_primary_queues_history_and_never_writes_a_mirror
```

## Depends On
[[core-tests-pass]]
