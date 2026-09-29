---
id: home-ledger.core-tests-pass
project: home-ledger
category: logic
severity: critical
environment: any
depends_on: []
---

# The Rust core compiles and its tests pass

## Claim
`cargo test --workspace --exclude home-ledger` passes.

## Why
Covers the money type, the record schemas, the document's
forward-compatibility, the storage backends and the fallback engine — every
rule that protects the ledger from losing or corrupting a figure.

## Check
```bash
cargo test --workspace --exclude home-ledger
```

## Depends On
None
