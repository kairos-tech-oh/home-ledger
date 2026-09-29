---
id: home-ledger.a-refused-edit-changes-nothing
project: home-ledger
category: correctness
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# A refused edit leaves the ledger exactly as it was

## Claim
An op that breaks a rule returns an error and changes no balance and no
record — including a multi-bucket adjustment where only one target is bad.

## Why
A half-applied edit is worse than a refused one: one bucket credited and
another not leaves a ledger that reconciles against nothing, and the person
has no way to tell it happened.

## Check
```bash
cargo test -p ledger-writer a_record_with_no_name_is_refused_and_nothing_is_stored
cargo test -p ledger-writer an_adjustment_naming_a_missing_bucket_moves_nothing
cargo test -p ledger-writer funds_cannot_be_moved_out_of_a_locked_bucket
cargo test -p ledger-writer a_bucket_cannot_pay_out_more_than_it_holds
```

## Depends On
[[core-tests-pass]]
