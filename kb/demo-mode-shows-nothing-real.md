---
id: home-ledger.demo-mode-shows-nothing-real
project: home-ledger
category: privacy
severity: error
environment: desktop
depends_on: []
---

# Demo mode shows nothing real and changes nothing real

## Claim
With demo mode on (Settings → Privacy), every screen reads a copy of the
ledger in which each record's amounts are scaled by its own factor, between
about a third and two and a half times, never within 15% of the real figure.
Figures worked out from others are worked out again, so an account still
equals its buckets, a statement still adds up and every total agrees. Market
prices stay real; holding quantities are scaled. The history's amounts and
the net-worth history are scaled to match. The same seed makes up the same
figures; "New figures" picks another.

Edits in demo mode change only that copy: nothing is queued, synced, saved
or written to the history, and turning demo mode off drops them. Bank
fetches, bank balances and charges, and imports are refused until it is off.
The daily net-worth point is still taken from the real ledger. Demo mode
belongs to the desktop window's session; `hl` always sees the real ledger.
Names are not changed.

## Why
Showing the app to someone, or taking screenshots of it, should not show the
family's money, and a demo that edits the real ledger is not a demo.

## Check
```bash
cargo test -p ledger-app demo
grep -q 'demo_set' src-tauri/src/lib.rs
npm --prefix ui run check
```

## Depends On
None
