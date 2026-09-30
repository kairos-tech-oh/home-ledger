---
id: home-ledger.account-sections-fold
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: [home-ledger.accounts-open-as-cards-by-type]
---

# An account section hides its accounts when its heading is clicked, in either view

## Claim
Each section heading on `Accounts.svelte` is a button that hides the accounts
under it and shows them again, leaving the heading, count and subtotal in place,
and says which it is through `aria-expanded`. The hidden sections are held apart
from the view and tested before it, so a section hidden in the cards stays
hidden in the table and the other way round. The fold styles live in `app.css`,
shared with the budget page.

## Why
With many accounts, most visits are about one kind — the cards, or the savings.
Hiding the others keeps their subtotals on screen without their cards.

Checked in a real browser against the real component: three accounts in two
sections; hiding checking left one card, switching to the table left one row,
and showing it again brought all three back.

## Check
```bash
grep -q 'const collapsed = new SvelteSet<string>();' ui/src/Accounts.svelte
grep -q 'onclick={() => fold(group.key)}' ui/src/Accounts.svelte
grep -q 'aria-expanded={!collapsed.has(group.key)}' ui/src/Accounts.svelte
node -e "const s = require('fs').readFileSync('ui/src/Accounts.svelte', 'utf8'); if (!/\{#if collapsed\.has\(group\.key\)\}\s*<!--[^>]*-->\s*\{:else if view === \"cards\"\}/.test(s)) throw new Error('the fold is not checked ahead of the view');"
grep -q '^\.group-head\.fold {' ui/src/app.css
npm --prefix ui run check
```

## Depends On
- home-ledger.accounts-open-as-cards-by-type
