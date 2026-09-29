---
id: home-ledger.a-budget-line-reaches-retirement-three-ways
project: home-ledger
category: correctness
severity: high
environment: any
depends_on: [home-ledger.agrees-with-the-prototype]
---

# Money reaches a retirement account by more than a direct pointer

## Claim
`retirement_standing` counts a budget line toward an account when the line
names the account, when the line's bucket is linked to it, or when the line
sits in a bucket called "Retirement" and the account name starts with the
line's name.

## Why
Only the first of those was ported at first, and on real data the monthly figure
going in came out nearly a quarter low — one budget line reaching a Roth
through its bucket. Every projection is built on that monthly figure, so the
whole retirement screen was wrong by the same proportion.

## Check
```bash
cargo test -p ledger-math a_budget_line_reaches_retirement_through_a_linked_bucket
```

## Depends On
[[agrees-with-the-prototype]]
