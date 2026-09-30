---
id: home-ledger.accounts-open-as-cards-by-type
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# The accounts page opens as cards, one section per kind of account

## Claim
`Accounts.svelte` starts in the card view and lists accounts under one heading
per kind — checking, savings, investment, the two retirement kinds, credit,
HELOC, loan, property, vehicle, other — each with its count and net subtotal,
in the order `sectionOrder` gives (see account-sections-keep-their-order).
`sections` keeps every account, and a kind it does not know gets its own
section after the known ones rather than being dropped. The table view is
sectioned the same way.

## Why
One long list mixes what is owned with what is owed, and a credit card sits
between two savings accounts. Grouped by kind, each section answers one
question — how much cash, how much invested, how much owed — at a glance.

## Check
```bash
node --experimental-strip-types --no-warnings --input-type=module -e "import { sections } from './ui/src/order.ts'; const got = sections([{ k: 'credit', n: 'Visa' }, { k: 'checking', n: 'Main' }, { k: 'barter', n: 'Goats' }, { k: 'credit', n: 'Amex' }], (a) => a.k, ['checking', 'savings', 'credit']).map((s) => s.key + ':' + s.items.map((a) => a.n).join('+')).join(','); if (got !== 'checking:Main,credit:Visa+Amex,barter:Goats') throw new Error(got);"
grep -q 'let view = $state<"cards" | "table">("cards");' ui/src/Accounts.svelte
grep -q 'sections(accounts, (a) => a.kind, shown)' ui/src/Accounts.svelte
test "$(grep -c '{#each group.items as account (account.id)}' ui/src/Accounts.svelte)" = 2
! grep -q '{#each accounts as account' ui/src/Accounts.svelte
npm --prefix ui run check
```

## Depends On
None
