---
id: home-ledger.history-actions-are-coloured
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# Each kind of history action has its own colour

## Claim
`actionTone` gives Create, Add, Update and Remove four different colours —
blue, green, amber and red — with Edit coloured as Update, Delete as Remove and
Move violet. An action it does not know, which imported plugin history can
carry, stays muted rather than borrowing a colour that means something else.
`History.svelte` colours each entry's pill with it.

## Why
When every pill is the same grey, the history reads as one undifferentiated
list, and a removal looks exactly like an addition. Colour lets someone looking
for what went wrong find the deletions without reading every line.

## Check
```bash
node --experimental-strip-types --no-warnings --input-type=module -e "import { actionTone as t } from './ui/src/actions.ts'; const want = { Create: 'blue', Add: 'green', Update: 'amber', Remove: 'red', Edit: 'amber', Delete: 'red', Move: 'violet', ' add ': 'green', Quote: 'muted', whatever: 'muted' }; for (const [a, c] of Object.entries(want)) if (t(a) !== c) throw new Error(a + ' is ' + t(a) + ', not ' + c); if (new Set(['Create', 'Add', 'Update', 'Remove'].map(t)).size !== 4) throw new Error('two of the four share a colour');"
grep -q 'class="pill {actionTone(entry.action)}"' ui/src/History.svelte
! grep -q '<span class="pill muted">{entry.action}</span>' ui/src/History.svelte
npm --prefix ui run check
```

## Depends On
None
