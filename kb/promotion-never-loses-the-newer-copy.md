---
id: home-ledger.promotion-never-loses-the-newer-copy
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.promotion-is-checked-not-guessed]
---

# Promoting a backup brings it up to date rather than discarding what it missed

## Claim
`decide` fast-forwards a backup that is merely behind, refuses when the two
copies diverged or the old store cannot be read, and never proposes a copy
in a case where the copies are unrelated.

## Why
A backup is normally behind the store it mirrors, so promoting it naively
throws away every edit it had not yet received. The refusals are the other
half: a fork means one side is discarded whichever way it goes, and an
unreadable source of truth means nobody knows what is being given up.

## Check
```bash
cargo test -p ledger-app a_backup_that_is_behind_is_brought_up_to_date_first
cargo test -p ledger-app two_copies_that_diverged_are_refused
cargo test -p ledger-app promoting_blind_is_refused_until_it_is_asked_for_twice
cargo test -p ledger-app a_fast_forward_is_only_ever_proposed_when_the_copies_are_related
```

## Depends On
[[promotion-is-checked-not-guessed]]
