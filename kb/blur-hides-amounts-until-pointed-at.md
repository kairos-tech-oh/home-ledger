---
id: home-ledger.blur-hides-amounts-until-pointed-at
project: home-ledger
category: privacy
severity: warn
environment: desktop
depends_on: []
---

# Blur hides every amount until the pointer is over it

## Claim
With "Blur amounts" on (Settings → Privacy), whatever holds a dollar figure
is blurred: text, chart labels, a dropdown that lists amounts, and amount
inputs. Pointing at one, or focusing an input, shows it. The figures are the
real ones; nothing is changed or sent anywhere. Elements are marked with a
`data-amount` attribute the screens never set, as they are drawn and redrawn,
so a redraw does not undo the blur. The switch is kept on this computer.

## Why
Working on the ledger where others can see the screen should not show them
the balances.

## Check
```bash
grep -q 'data-amount' ui/src/privacy.svelte.ts
grep -q ':root\[data-blur\] \[data-amount\]' ui/src/app.css
npm --prefix ui run check
```

## Depends On
None
