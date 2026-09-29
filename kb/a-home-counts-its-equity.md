---
id: home-ledger.a-home-counts-its-equity
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# A home or car counts as an asset, and reads against its loan as equity

## Claim
Property and vehicle accounts hold what the thing is worth, counted as an
asset like any other balance, so net worth is value less the loan rather than
the loan alone. Either may name one loan or HELOC secured against it. The link
must point at a loan or HELOC that exists, one loan cannot be secured against
two things, a link is dropped when the account stops being a property or
vehicle or the loan is deleted, and an account with no link writes no link.

## Why
Recording a mortgage without the home it bought makes a household look far
poorer than it is. The link shows equity next to the value and changes no
figure, so net worth never counts anything twice.

## Check
```bash
cargo test -p ledger-writer secured
grep -q '"property",' crates/ledger-domain/src/records.rs
grep -q '{ value: "property", label: "Property" },' ui/src/ledger.ts
npm --prefix ui run check
```

## Depends On
None
