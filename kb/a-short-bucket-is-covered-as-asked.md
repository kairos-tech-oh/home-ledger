---
id: home-ledger.a-short-bucket-is-covered-as-asked
project: home-ledger
category: correctness
severity: error
environment: any
depends_on: []
---

# A bucket that cannot cover a statement is covered as asked, and undo returns it all

## Claim
Before settling, the Reconcile page shows each bucket that the open statements
together need more from than it holds, and each statement that is short on
its own. Settling such a statement covers the rest one of three ways: from
another bucket, which must be unlocked and hold enough; as everyday spending,
from the spending account; or by letting the bucket go below zero. The choice
comes from the settle itself, or from the bucket's remembered default. With
neither, the settle is refused and nothing moves. Every move is recorded, so
undo restores every bucket and account exactly. A bucket below zero cannot be
spent from, and money added to it fills it back first.

## Why
Two open statements drawing on one bucket used to leave the second
unsettleable, with no warning until the attempt. Shortfalls are ordinary, but
how one is covered says something about the budget, so the person chooses,
once if they like.

## Check
```bash
cargo test -p ledger-writer cover_tests
cargo test -p ledger-math shortfall
cargo test -p ledger-writer reconcile::tests
npm --prefix ui run check
```

## Depends On
None
