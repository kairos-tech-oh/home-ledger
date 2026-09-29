---
id: home-ledger.agrees-with-the-prototype
project: home-ledger
category: correctness
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# The balance sheet agrees with the implementation being replaced

## Claim
`net_worth` counts holdings, treats a liability's typed total as replacing
its debt lines rather than adding to them, and includes holdings that belong
to no account.

## Why
These three rules disagreed with `core/Model.js` by six figures on real data
when first written, and each looks correct in isolation. `tools/compare.sh`
checks both implementations over one document; these tests pin the rules so
the prototype is not needed to catch a regression.

## Check
```bash
cargo test -p ledger-math a_liability_total_replaces_its_debt_lines_rather_than_adding_to_them
cargo test -p ledger-math holdings_count_toward_the_account_that_holds_them
cargo test -p ledger-math a_holding_with_no_account_still_belongs_to_the_household
```

## Depends On
[[core-tests-pass]]
