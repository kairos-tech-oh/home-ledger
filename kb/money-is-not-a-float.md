---
id: home-ledger.money-is-not-a-float
project: home-ledger
category: correctness
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Money is decimal and never a binary float

## Claim
No amount-bearing type in `ledger-domain` or `ledger-math` is `f32` or `f64`,
and `Money` is backed by `rust_decimal`.

## Why
`0.1 + 0.2 != 0.3` in binary floating point, and a bucket adjusted thousands
of times accumulates the error into a balance a person is relying on. The
prototype used floats; this port exists partly to stop that.

## Check
```bash
grep -q "rust_decimal::Decimal" crates/ledger-domain/src/money.rs
! grep -rnE ':\s*f(32|64)\b' crates/ledger-domain/src crates/ledger-math/src \
  --include=*.rs | grep -v 'to_f64' | grep -q .
```

## Depends On
[[core-tests-pass]]
