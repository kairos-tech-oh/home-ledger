---
id: home-ledger.a-split-always-adds-back-up
project: home-ledger
category: correctness
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Dividing an amount between earners never loses or invents a penny

## Claim
`ledger_math::split` distributes the remainder rather than rounding each
share, so the shares sum exactly to the amount for every value swept, and the
same input always produces the same result.

## Why
The implementation this replaces computed each share as
`amount * percent / 100` and rounded independently. Three equal earners
splitting £10 get £3.33 each and a penny leaves the household's books —
every month, on a figure people reconcile against.

## Check
```bash
cargo test -p ledger-math the_shares_always_add_back_up_to_the_amount
cargo test -p ledger-math three_equal_earners_splitting_ten_pounds_lose_nothing
cargo test -p ledger-math a_negative_amount_splits_and_still_sums
cargo test -p ledger-math the_same_split_comes_out_the_same_way_every_time
cargo test -p ledger-domain a_four_decimal_price_displays_rounded_not_truncated
```

## Depends On
[[core-tests-pass]]
