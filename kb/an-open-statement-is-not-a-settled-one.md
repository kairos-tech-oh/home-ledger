---
id: home-ledger.an-open-statement-is-not-a-settled-one
project: home-ledger
category: correctness
severity: high
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Settled reconciliations are counted apart from open ones

## Claim
`recon_rollup` totals only open statements into `open_total`, counts settled
ones separately, and calls a statement ready only when its charges account for
its balance to within half a cent.

## Why
A settled statement has already moved its money; adding it back into what is
outstanding double-counts a bill that is paid. The half-cent tolerance is what
stops a statement that is right to the penny from reading as unfinished.

## Check
```bash
cargo test -p ledger-math open_reconciliations_are_counted_apart_from_settled_ones
```

## Depends On
[[core-tests-pass]]
