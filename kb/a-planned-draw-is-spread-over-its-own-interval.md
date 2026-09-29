---
id: home-ledger.a-planned-draw-is-spread-over-its-own-interval
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# A repeating bill is spread over the interval it actually repeats in

## Claim
`expense_monthly` divides a recurring draw by its own cadence, and returns
nothing for a one-off.

## Why
A premium of 1,590 every six months is 265 a month, which is what makes it
comparable to the budget line it sits under. Two failure modes matter: using
a flat twelve understates anything that repeats more often, and averaging a
one-off over the year inflates every month's budget with a bill that happens
once.

## Check
```bash
cargo test -p ledger-math a_premium_every_six_months_reads_as_a_monthly_figure
cargo test -p ledger-math each_interval_spreads_over_the_year_it_actually_repeats_in
cargo test -p ledger-math a_one_off_draw_has_no_monthly_figure
```

## Depends On
[[core-tests-pass]]
