---
id: home-ledger.spending-is-what-the-statements-itemise
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# Spending is what card statements itemise, and withdrawals are what settling took

## Claim
Spending counts only the positive lines on reconciliations, dated by their own
purchase date, then the statement date, then the settlement date. A charge
with none of those counts only in All time. Withdrawals are the bucket debits
a settlement recorded, dated by the day it was settled on the person's own
clock. An undone settlement withdrew nothing, and one settled before debits
were recorded is counted as missing rather than guessed. Names group ignoring
case and extra spaces. A statement balance the lines do not cover is reported
as unitemised and is not spending. A period of N months starts on the same day
N months back, or on that month's last day if it is shorter.

## Why
Budget lines are plans and bucket moves are transfers. Counting either as
spending would double-count every settled charge. Ported from
`core/Spending.js`, with every assertion in `check-spending.mjs` as a Rust
test: 115 across five charges in all time, 39.90 in September, 50 withdrawn,
and 29 February as a month back from 31 March 2024.

## Check
```bash
cargo test -p ledger-math spending
cargo test -p ledger-math calendar
cargo test -p home-ledger spending
cargo test -p ledger-config family_names
npm --prefix ui run check
```

## Depends On
None
