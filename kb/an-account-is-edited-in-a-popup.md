---
id: home-ledger.an-account-is-edited-in-a-popup
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# An account is added and edited in a popup, not at the top of the page

## Claim
`Accounts.svelte` opens `AccountForm` inside `Modal.svelte`, a native `<dialog>`
shown with `showModal()`, for both adding and editing; the form is never placed
inline on the page. The popup closes on its `close` event, so Escape, Cancel and
any other way the dialog is shut all clear the editing state. A save that fails
leaves the popup open with the error inside it; one that succeeds closes it.

## Why
An inline form at the top of the page sends the reader away from the card they
clicked, and on a long page they lose their place. A popup keeps the list where
it was. Closing on `close` rather than `cancel` matters because an engine may
close a dialog without firing `cancel`, which would leave the page believing the
form was still open.

Checked in a real browser against the real component: Edit opened a modal
dialog holding the account's values; a refused save kept it open with the error
inside and none on the page; a good save sent the set and closed it; a `close`
event unmounted it and it reopened on the next Edit.

## Check
```bash
grep -q 'dialog.showModal();' ui/src/Modal.svelte
grep -q '<dialog bind:this={dialog} class="modal" {onclose}>' ui/src/Modal.svelte
test "$(grep -c '<AccountForm' ui/src/Accounts.svelte)" = 2
node -e "const s = require('fs').readFileSync('ui/src/Accounts.svelte', 'utf8'); const m = s.indexOf('<Modal onclose={close}>'), e = s.indexOf('</Modal>'); if (m < 0 || e < 0) throw new Error('no popup'); for (const i of [...s.matchAll(/<AccountForm/g)].map((x) => x.index)) if (i < m || i > e) throw new Error('a form sits outside the popup');"
grep -q '^\.modal::backdrop {' ui/src/app.css
npm --prefix ui run check
```

## Depends On
None
