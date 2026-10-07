---
id: home-ledger.a-bucket-spend-leaves-its-account-too
project: home-ledger
category: correctness
severity: error
environment: any
depends_on: []
---

# Money spent from a bucket leaves the account the bucket is kept in

## Claim
Every bucket says which account its money is kept in: one that holds money,
not a card, a loan, a home or a car. Spending from a bucket is its own edit,
`bucket-spend`. It takes the amount out of the bucket and out of that
account together, in one history entry. The account loses the whole amount
even when the bucket held less, because that is what left the real account.

A move between buckets kept in two different accounts moves the money
between the accounts. Within one account, the accounts are unchanged.

A payday, its undo, adding to a bucket and setting a bucket's balance by
hand move only buckets: they allocate or correct, and no money leaves.

A bucket with no account changes alone. The Buckets screen and `hl` say so,
and the screen offers to give every such bucket an account in one step.

## Why
Before this, a spend changed only the bucket. A mortgage payment taken from
a bucket left the savings account's balance where it was, so net worth
counted money that had already gone.

## Check
```bash
cargo test -p ledger-writer a_spend_leaves_the_bucket_and_its_account_together
cargo test -p ledger-writer the_account_loses_all_of_a_spend_the_bucket_could_not_cover
cargo test -p ledger-writer a_payday_and_its_undo_leave_accounts_alone
cargo test -p ledger-writer a_move_between_accounts_moves_the_money_between_them
cargo test -p ledger-writer a_spend_from_a_bucket_kept_on_a_card_is_owed
grep -q 'op: "bucket-spend"' ui/src/Buckets.svelte
grep -q '"op": "bucket-spend"' crates/hl/src/write.rs
test "$(grep -c 'op: "bucket-adjust"' ui/src/Buckets.svelte)" = 2
```
