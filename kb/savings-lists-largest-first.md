---
id: home-ledger.savings-lists-largest-first
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# Savings buckets are listed largest first

## Claim
`largestFirst` orders buckets by total, descending, comparing totals as numbers
rather than as text, and breaks a tie by name so the order is stable. The
savings page's cards, its table and the "move into" choices all use that order.

## Why
The largest buckets — Retirement above all — are the ones worth seeing first,
and the document's order is only the order they happened to be created in.
Totals arrive as decimal strings, and sorted as strings `"900.00"` would land
above `"12000.00"`, which is exactly the bug that looks right on small data.

## Check
```bash
node --experimental-strip-types --no-warnings --input-type=module -e "import { largestFirst } from './ui/src/order.ts'; const got = largestFirst([{ name: 'Gifts', total: '900.00' }, { name: 'Retirement', total: '12000.00' }, { name: 'Car', total: '900.00' }, { name: 'Taxes', total: '-5.00' }]).map((b) => b.name).join(','); if (got !== 'Retirement,Car,Gifts,Taxes') throw new Error(got);"
grep -q 'const ordered = $derived(largestFirst(buckets));' ui/src/Buckets.svelte
test "$(grep -c '{#each ordered as bucket (bucket.id)}' ui/src/Buckets.svelte)" = 2
! grep -q '{#each buckets as bucket' ui/src/Buckets.svelte
npm --prefix ui run check
```

## Depends On
None
