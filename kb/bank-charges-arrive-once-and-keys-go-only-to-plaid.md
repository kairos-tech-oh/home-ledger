---
id: home-ledger.bank-charges-arrive-once-and-keys-go-only-to-plaid
project: home-ledger
category: correctness
severity: error
environment: any
depends_on: []
---

# Bank charges arrive once, through the import, and the keys go only to Plaid

## Claim
A bank connection writes nothing by itself. Its charges come into a
statement's import preview, and its balances come as proposals. Either is
applied through the ordinary edit path, labelled "plaid" in the history.

A bank transaction is added once, on whichever statement took it first. The
writer enforces this, not only the preview, and a line keeps its bank link
through edits from clients that know nothing of it.

The Plaid keys and each bank's access token live in the keychain, and the
fetched transactions live in a local file, sealed when encryption is on.
None of it goes into the ledger. Requests go only to Plaid's two hosts: no
setting or environment variable can point them anywhere else, and only tests
may use a local address.

## Why
A bank feed that wrote on its own, or added a charge twice, would make every
statement suspect. Keys that could be pointed at another server could be
stolen by anyone able to set an environment variable.

## Check
```bash
cargo test -p ledger-app bank::
cargo test -p ledger-writer a_bank_transaction_is_added_once_wherever_it_already_is
cargo test -p ledger-writer editing_a_statement_keeps_its_charges_bank_links
grep -q '"https://sandbox.plaid.com"' crates/ledger-app/src/bank/plaid.rs
grep -q '"https://production.plaid.com"' crates/ledger-app/src/bank/plaid.rs
test "$(grep -c 'std::env::var' crates/ledger-app/src/bank/plaid.rs)" = 0
grep -B1 'if environment.starts_with("http://127.0.0.1:")' crates/ledger-app/src/bank/plaid.rs | grep -qF '#[cfg(test)]'
grep -qF 'crate::bank::banks(state)' crates/ledger-app/src/encryption.rs
```
