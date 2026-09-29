---
id: home-ledger.import-never-silently-replaces
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Import cannot quietly destroy a ledger

## Claim
`Engine::adopt` refuses while the outbox holds anything, and `import_apply`
refuses to run over a ledger that already holds records unless it is told to
replace them.

## Why
Import is a whole-document replace, which is the one operation in this app
that can lose everything at once. Queued edits are the subtler half: they
were made against a document that is about to stop existing, so replaying
them afterwards would hit records that are gone.

## Check
```bash
cargo test -p ledger-store adopting_is_refused_while_edits_are_still_queued
cargo test -p ledger-store adopting_replaces_the_document_outright
cargo test -p ledger-writer one_bad_row_is_skipped_and_named_rather_than_failing_the_import
```

## Depends On
[[core-tests-pass]]
