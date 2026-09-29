---
id: home-ledger.net-worth-history-is-a-point-a-day
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# Net worth history is a point a day, shared and never taken from a stale copy

## Claim
The first time the app opens on a day, it records that day's figures (net,
assets, debts, savings, holdings, basis, monthly income and budget, and each
bucket's total) in `snapshots.json`, in the plugin's own shape. It records
nothing when the ledger was read from a mirror because the source of truth was
unreachable. Of two points for one day, the one taken later is kept, and at
most ten years are held. Each install publishes its points to its own slot on
the source of truth and reads every other install's back. A plugin
`snapshots.json` can be imported under Storage; one unreadable point is
skipped rather than the file refused, and importing the same file twice adds
nothing. The 30-day change is measured from the oldest point inside the window.

## Why
Yesterday's balances cannot be recomputed from today's document, so this is
the one thing that has to be stored, and losing it loses it for good. A point
taken from a copy that may be behind would record the wrong figure permanently.

## Check
```bash
cargo test -p ledger-math snapshots
cargo test -p home-ledger snapshots
grep -q 'if !loaded.stale {' src-tauri/src/snapshots.rs
npm --prefix ui run check
```

## Depends On
None
