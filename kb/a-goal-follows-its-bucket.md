---
id: home-ledger.a-goal-follows-its-bucket
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# A goal's progress is read from the bucket behind it

## Claim
A goal holds no money. What it has saved is whatever its linked bucket is worth
(cash, holdings in the bucket's name and Roth basis allocated to it). A goal
linked to nothing, or to a bucket that has since been deleted, has saved
nothing. Progress is saved against target, capped at 100%, and there is none
without a target above zero. The Goals screen lists goals nearest to done first,
with untargeted goals last and ordered by name.

## Why
Money kept in two places goes out of step. A goal is a lens on a bucket, not a
second balance. Ported from `goalSaved` and `goalProgress` in the plugin's
`core/Model.js`, using the same fixture and expectations as `check-math.mjs`:
28,400 saved and 47.3% of 60,000.

## Check
```bash
cargo test -p ledger-math goals
grep -q 'ledger_math::goals_in_order(doc)' src-tauri/src/views.rs
npm --prefix ui run check
```

## Depends On
None
