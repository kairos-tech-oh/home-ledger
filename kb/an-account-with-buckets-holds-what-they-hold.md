---
id: home-ledger.an-account-with-buckets-holds-what-they-hold
project: home-ledger
category: correctness
severity: error
environment: any
depends_on: []
---

# An account that keeps buckets holds what they hold

## Claim
Every bucket says which account its money is kept in: one that holds money,
not a card, a loan, a home or a car. Once any bucket is kept in an account,
that account's balance is the cash in its buckets, and nothing else. It is
worked out on every read and after every edit, so it cannot drift from them.
Holdings and Roth contributions credited to a bucket are not counted: they
sit in other accounts. An account with no buckets keeps the balance typed for
it.

So anything that moves a bucket moves its account: a spend, a payday and its
undo, an add, a balance set by hand, a move between buckets in two accounts,
a statement settle and its undo. The history entry names the account too.

Typing a different balance for such an account is refused; the Accounts
popup shows it read-only. A bank balance from Plaid is shown against it, with
the gap, and never written over it. A settle takes only everyday spending from
a chosen account, and refuses one that holds only buckets.

A spend larger than its bucket is covered the way a statement covers it: as
asked, or by the bucket's own "when short": from another unlocked bucket that
holds enough, as everyday money (only what the bucket held leaves savings),
or by letting the bucket go below zero. With neither, it is refused and
nothing moves.

## Why
Kept as two figures, an account and its buckets drifted apart: some edits
moved one, some the other, and a settle took bucket money from whichever
account the statement named. Fidelity Savings ended up $1,159.09 below its
own buckets. With the buckets as the record, there is nothing to drift.

## Check
```bash
cargo test -p ledger-writer an_account_with_buckets_holds_what_they_hold
cargo test -p ledger-writer an_account_with_no_buckets_keeps_its_typed_balance
cargo test -p ledger-writer a_spend_leaves_the_bucket_and_its_account_together
cargo test -p ledger-writer a_short_spend_is_covered_as_the_bucket_says
cargo test -p ledger-writer a_cover_bucket_that_is_locked_or_too_small_refuses
cargo test -p ledger-writer an_adjustment_moves_the_account_with_the_bucket
cargo test -p ledger-writer a_move_between_accounts_moves_the_money_between_them
cargo test -p ledger-writer a_payday_puts_the_money_in_the_buckets_and_their_account
cargo test -p ledger-writer spending_cannot_be_taken_from_an_account_that_holds_only_buckets
cargo test -p ledger-app an_account_kept_as_its_buckets_is_shown_the_bank_figure_but_not_given_it
grep -q 'op: "bucket-deposit"' ui/src/Buckets.svelte
grep -q '"op": "bucket-spend"' crates/hl/src/write.rs
! grep -q bucketSourceId ui/src/StatementForm.svelte
npm --prefix ui run check
```

## Depends On
None
