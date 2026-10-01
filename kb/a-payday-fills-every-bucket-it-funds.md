---
id: home-ledger.a-payday-fills-every-bucket-it-funds
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# A payday adds one paycheck's worth to every bucket that earner funds

## Claim
For each earner, the Savings page offers "Add all", which adds to every bucket
their share of the budget lines feeding it, by income, divided by the
paychecks they receive in a month. It is one edit, and Undo takes the same
amounts back out. The amounts are rounded once, at the paycheck, and agree
with the plugin's `contributionDeltas` to the cent on the real ledger.

## Why
Moving each paycheck into a dozen buckets by hand is the chore the buckets
exist to remove. Rounding twice (monthly share, then paycheck) came out a cent
high on a third of the real buckets, which is why it rounds once.

## Check
```bash
cargo test -p ledger-math payday
grep -q 'label: `${undo ? "Undo " : ""}${day.owner}' ui/src/Buckets.svelte
npm --prefix ui run check
```

## Depends On
None
