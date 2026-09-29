---
id: home-ledger.a-projection-compounds-in-decimal
project: home-ledger
category: correctness
severity: high
environment: any
depends_on: [home-ledger.money-is-not-a-float, home-ledger.core-tests-pass]
---

# A projection compounds in decimal and agrees with a fifty-digit sum

## Claim
`project_balance` carries the running balance as a `Decimal` held at eight
places, takes the contribution at month end, and keeps a point per year plus
the horizon itself when the horizon is not a whole year.

## Why
Thirty years is three hundred and sixty multiplications by a factor that is not
exact in binary. The prototype's `float` version comes out one to four cents
low on the real ledger, growing with the rate; the port matches the same sum
computed at fifty digits. This is a recorded departure in `docs/PORT.md`, so
`compare.sh` disagreeing here is expected rather than a regression.

## Check
```bash
cargo test -p ledger-math a_balance_compounds_monthly_and_takes_its_contribution_at_month_end
cargo test -p ledger-math a_horizon_that_is_not_whole_years_keeps_its_own_last_point
cargo test -p ledger-math growth_is_what_the_rate_added_rather_than_what_was_paid_in
cargo test -p ledger-math a_target_year_already_gone_projects_nothing_rather_than_backwards
```

## Depends On
[[money-is-not-a-float]], [[core-tests-pass]]
