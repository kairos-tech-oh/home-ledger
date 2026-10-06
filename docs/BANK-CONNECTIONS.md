# Bank connections

Home Ledger can fetch transactions and balances straight from a bank through
[Plaid](https://plaid.com), using **your own Plaid keys**. Nothing passes
through a server of ours, because there is none.

## The shape of it

**A source, not a second ledger.** A bank connection feeds the same paths the
app already has. Transactions arrive as an import preview on a statement,
exactly like a CSV export: duplicates marked, charges after the statement
date set aside, and attribution defaulting to All, until the person chooses
what to add. Balances arrive as a list of proposed balance changes to accept.
Nothing is written without being shown first. Every change is an ordinary
edit, in the history as "Office PC · desktop · plaid", and undoes like any
other.

The web app's Plaid integration felt clunky in use. This one deliberately
builds nothing around the bank data: no separate transaction store to
browse, no categories to correct, no syncing out of sight.

**Bring your own keys.** Plaid authenticates an app with a client id and a
secret, and expects them to stay on a server. An open-source desktop app
cannot ship a secret, so each person signs up for their own Plaid developer
account and enters their keys in Settings. They go in the OS keychain with
every other credential. Sandbox keys work for trying it out with Plaid's test
banks. Real banks need Production access from Plaid, on Plaid's terms.

**Per machine.** The keys, and the access token for each connected bank, live
in this machine's keychain. They are not written into the ledger, so a
ledger file shared through S3 never holds a working bank credential, even
when it is not encrypted. One machine, the desktop or a Pi running `hl` on a
schedule, does the fetching. Every machine sees the results.

## How it works

1. **Connecting a bank** uses Plaid's
   [Hosted Link](https://plaid.com/docs/link/hosted-link/). The app asks Plaid
   for a link token with a `hosted_link` object, opens the returned
   `hosted_link_url` in the system browser, and polls `/link/token/get` until
   the session finishes. Then it exchanges
   `link_sessions[].results.item_add_results[].public_token` for an access
   token. Plaid's own page handles the bank's sign-in, including OAuth banks,
   so the app embeds no third-party script and needs no redirect address of
   its own.
2. **Linking accounts.** Each Plaid account (name, mask, type) is linked to a
   ledger account by the person. An unlinked account is ignored.
3. **Transactions** come from `/transactions/sync`. Its cursor means each
   fetch asks only for what changed. Added, modified and removed transactions
   are kept in a local cache, sealed like the other local files when
   encryption is on. A statement's "From bank" import reads that cache for
   the statement's card, over the statement's dates.
   - Posted transactions only. A pending one changes its id when it posts,
     so importing it would make a duplicate later.
   - Plaid's sign is positive for money leaving the account, which on a card
     is a purchase. Payments into the card are left out, as the CSV import
     leaves them out.
   - A line carries Plaid's `transaction_id`, so a re-fetch never adds it
     twice. Lines already on a statement from a CSV are matched by date,
     amount and description, as now.
4. **Balances** come from `/accounts/get`: Plaid's latest balance, at no
   extra charge. A card's balance is a debt and is stored as one.
5. **Disconnecting** calls `/item/remove` and deletes the access token and
   the cache.

The same operations are on the command line: `hl bank status`, `hl bank
connect`, `hl bank fetch`, `hl bank balances`, and `hl statement import
<statement> --from-bank`.

## To prove in Sandbox before relying on it

- Hosted Link with an OAuth test institution completes and returns a public
  token by polling, with no `redirect_uri`.
- `/transactions/sync` on a new Item can return nothing at first, while Plaid
  is still pulling history. The app must say "still gathering" rather than
  "no transactions".
- Production base URL: `https://production.plaid.com`; Sandbox:
  `https://sandbox.plaid.com`.

## Considered, not chosen: SimpleFIN

[SimpleFIN Bridge](https://www.simplefin.org) was the other serious option.
You pay SimpleFIN a small yearly fee, connect your banks on their site, and
paste one setup token into the app. There is no developer account, no secret
for the app to keep, and no server. That suits an open-source app well, which
is why other open-source budgeting tools such as Actual Budget use it.

It was not the route chosen. The owner already knows Plaid from the web app
and wants to build on that. Plaid also covers more institutions, and its
transaction data is richer. The cost of that is setup: each person needs
their own Plaid keys. If that turns out to stop people using bank
connections, SimpleFIN is the alternative to add. The design above keeps the
bank as a source feeding the existing import, so a second provider would be
another source, not a rework.
