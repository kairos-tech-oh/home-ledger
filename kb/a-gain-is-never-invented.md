---
id: home-ledger.a-gain-is-never-invented
project: home-ledger
category: correctness
severity: high
environment: any
depends_on: [home-ledger.core-tests-pass, home-ledger.money-is-not-a-float]
---

# A holding with no recorded cost shows no gain rather than all of it

## Claim
`holding_gain` returns `None` when a holding has no cost basis, `holdings_total`
leaves the percentage out when the basis is zero, and a holding bought at its
current price reads as `0.00`, never `-0.00`.

## Why
The obvious arithmetic — value minus a missing basis — turns an unknown into a
full profit, and the Holdings tab would tell someone they had made money they
had not. The minus-zero case is the same mistake one rounding step later.

## Check
```bash
cargo test -p ledger-math an_unrecorded_cost_shows_no_gain_rather_than_all_of_it
cargo test -p ledger-math a_holding_bought_at_its_current_price_does_not_read_as_minus_zero
cargo test -p ledger-math holdings_add_up_to_a_value_a_basis_and_a_gain
```

## Depends On
[[core-tests-pass]], [[money-is-not-a-float]]
