---
id: home-ledger.budget-sections-roll-up
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# A budget section rolls up when its heading is clicked, in either view

## Claim
Each budget section's heading is a button that folds its lines away and back,
leaving the heading and its subtotal in place, and says which it is through
`aria-expanded`. The folded sections are held apart from the view, and the fold
is tested before the view is chosen, so a section rolled up in the table stays
rolled up in the cards and the other way round. The page still opens as a table.

## Why
A long budget is read one section at a time; rolling the others up keeps their
subtotals on screen without their lines. Checking the fold ahead of the view,
rather than inside each view, means a later change to either view cannot
quietly lose it.

Checked in a real browser against the real component: three lines in two
sections; folding one left one row, switching to cards left one card, and
unfolding brought all three back.

## Check
```bash
grep -q 'const collapsed = new SvelteSet<string>();' ui/src/Budget.svelte
grep -q 'onclick={() => fold(group.type)}' ui/src/Budget.svelte
grep -q 'aria-expanded={!collapsed.has(group.type)}' ui/src/Budget.svelte
node -e "const s = require('fs').readFileSync('ui/src/Budget.svelte', 'utf8'); if (!/\{#if collapsed\.has\(group\.type\)\}\s*<!--[^>]*-->\s*\{:else if view === \"cards\"\}/.test(s)) throw new Error('the fold is not checked ahead of the view');"
grep -q 'let view = $state<"cards" | "table">("table");' ui/src/Budget.svelte
npm --prefix ui run check
```

## Depends On
None
