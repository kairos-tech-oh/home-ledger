---
id: home-ledger.savings-opens-as-cards
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# The savings page opens in the card view

## Claim
`Buckets.svelte` starts its view as `"cards"`, and the table stays one click
away on the same toggle.

## Why
Cards show each bucket's progress toward its target at a glance, which is what
the page is opened to see. The table is for reading every figure at once.

## Check
```bash
grep -q 'let view = $state<"cards" | "table">("cards");' ui/src/Buckets.svelte
grep -q 'onclick={() => (view = "table")}' ui/src/Buckets.svelte
npm --prefix ui run check
```

## Depends On
None
