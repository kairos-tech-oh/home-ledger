---
id: home-ledger.the-dashboard-is-the-plugins
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# The dashboard is stored in the ledger, and reads the same as the plugin's

## Claim
The layout is saved whole by `dashboard-set`, into the document, so every
machine shows the same one. It is cleaned the way the helper cleans it: at
most 40 widgets and 50 picks each, a bad widget dropped rather than the layout
refused, and a kind this build does not know kept rather than deleted. A
spending widget's options keep only the figures and breakdowns that exist, in
the plugin's order. Deleting an account, bucket or goal takes it off every
widget. With nothing saved, the default is built from the ledger with the
plugin's fixed ids, so an edit aimed at a default widget lands. The figures
each widget draws match the plugin's `widgetData` and `spendingWidgetData` on
the real ledger.

## Why
Every client has shown the same dashboard since the plugin stored it in the
ledger. A client that dropped a widget it did not recognise, or re-keyed the
default every time it was built, would silently undo edits made elsewhere.

## Check
```bash
cargo test -p ledger-domain dashboard
cargo test -p ledger-writer dashboard
cargo test -p ledger-math dashboard
cargo test -p ledger-app dashboard
npm --prefix ui run check
```

## Depends On
None
