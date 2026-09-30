---
id: home-ledger.account-sections-keep-their-order
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: [home-ledger.accounts-open-as-cards-by-type]
---

# Account sections open most-accounts-first, and keep the order they are given

## Claim
With nothing arranged, the Accounts page shows the sections with the most
accounts first, equal counts in the order the kinds are offered, so sections
of one account sit at the bottom. Arrange moves a section up or down, and the
order is saved in the ledger by `account-order-set`: only known kinds, once
each. It therefore survives an update, a crash or a restart, and is the same
on every machine. A kind not in the saved order, such as a new one, follows
the arranged ones by the default rule. "Most accounts first" removes the
saved order and leaves no key behind.

## Why
Single-account sections at the top pushed the sections that matter below
the fold. The order is a choice about the household's accounts, so it is
kept with them rather than in one window.

## Check
```bash
cargo test -p ledger-domain layout
cargo test -p ledger-writer layout::
node --experimental-strip-types --no-warnings --input-type=module -e "import { sectionOrder } from './ui/src/order.ts'; const k=['checking','savings','credit','loan','other']; const c={checking:4,credit:6,loan:2,savings:1,other:1}; const a=sectionOrder(c,[],k).join(); const b=sectionOrder(c,['gone','loan'],k).join(); if (a!=='credit,checking,loan,savings,other') throw new Error(a); if (b!=='loan,credit,checking,savings,other') throw new Error(b);"
grep -q 'sections(accounts, (a) => a.kind, shown)' ui/src/Accounts.svelte
npm --prefix ui run check
```

## Depends On
- home-ledger.accounts-open-as-cards-by-type
