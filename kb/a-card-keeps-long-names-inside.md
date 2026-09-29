---
id: home-ledger.a-card-keeps-long-names-inside
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# A long name wraps inside its card instead of running over the edge

## Claim
A `.card` is held to its grid column (`min-width: 0` and a single
`minmax(0, 1fr)` column), and `.card-name` wraps (`white-space: normal`,
`overflow-wrap: anywhere`) even when it is a button, as a holding's name is.
Card notes still truncate with an ellipsis. The holdings page still opens in
the table view.

## Why
Every button is `white-space: nowrap`, and a grid item will not shrink below
its content, so a 61-character fund name pushed its card past the column and
over the next one, carrying the card's other text with it. Measured in a real
browser against this stylesheet, five elements crossed the card's edge before
the fix and none after.

## Check
```bash
node -e "const css = require('fs').readFileSync('ui/src/app.css', 'utf8'); const rule = (sel) => (css.match(new RegExp('\\\\n' + sel.replace('.', '\\\\.') + ' \\\\{([^}]*)\\\\}')) || [])[1] || ''; const need = { '.card': ['min-width: 0;', 'grid-template-columns: minmax(0, 1fr);'], '.card-name': ['white-space: normal;', 'overflow-wrap: anywhere;'], '.card-note': ['text-overflow: ellipsis;'] }; for (const [sel, decls] of Object.entries(need)) for (const d of decls) if (!rule(sel).includes(d)) throw new Error(sel + ' lacks ' + d);"
grep -q 'let view = $state<"cards" | "table">("table");' ui/src/Holdings.svelte
grep -q 'class="reveal card-name"' ui/src/Holdings.svelte
npm --prefix ui run check
```

## Depends On
None
