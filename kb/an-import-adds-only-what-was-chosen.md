---
id: home-ledger.an-import-adds-only-what-was-chosen
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# A bank export adds only the purchases chosen, once

## Claim
A card export is read by its column names (Chase, American Express, Discover,
Capital One and Citi headers), or guessed when it has none (Wells Fargo). A
purchase is told apart from a payment, return or credit by the bank's own word
for it when there is one, by debit and credit columns, or otherwise by the
sign most rows carry, and each of those can be corrected on screen. A charge
already on the statement (same day, amount and description, counted) is not
suggested again. Importing appends cleaned lines to an open statement and
never edits a line, the balance, or a settled statement. Imported charges are
attributed to "All" unless the person chooses otherwise.

## Why
Re-importing an updated export is the normal way to catch up, so it must add
only what is new. A payment read as a purchase would double what the statement
seems to owe.

## Check
```bash
cargo test -p ledger-writer bank_csv
cargo test -p ledger-writer import_tests
cargo test -p home-ledger transactions
grep -q 'let member = $state("All");' ui/src/ImportPanel.svelte
npm --prefix ui run check
```

## Depends On
None
