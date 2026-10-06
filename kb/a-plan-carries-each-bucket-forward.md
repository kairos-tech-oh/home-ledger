---
id: home-ledger.a-plan-carries-each-bucket-forward
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# Planning carries every bucket forward to a chosen date

## Claim
For a date in the future, each bucket's projection is what it holds now, plus
the budget lines feeding it times the months to that date (on a 30.4375-day
month), less every planned draw under those lines that falls in the window,
both ends included. A recurring draw is stepped by its interval from its first
date and stops at its end date. A bucket with no line feeding it has no monthly
rate, which is different from lines that add up to nothing. Buckets the plan
does not move are left out of the rows and the totals.

## Why
This screen answers whether a bucket will be ready when it is needed, and a
premium every six months matters as much as the monthly top-up. Ported from
`monthsBetween`, `occurrences` and `project` in `core/Model.js`, with every
planning case from `check-math.mjs`: 11.99 months in a year, 13 monthly draws
over a year counted from 1 January, and 28,400 + 5,996 − 2,400 for the
emergency bucket.

## Check
```bash
cargo test -p ledger-math planning
cargo test -p ledger-app planning
npm --prefix ui run check
```

## Depends On
None
