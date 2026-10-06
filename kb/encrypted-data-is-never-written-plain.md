---
id: home-ledger.encrypted-data-is-never-written-plain
project: home-ledger
category: security
severity: error
environment: any
depends_on: []
---

# With encryption on, nothing that holds balances is written readable

## Claim
Turning encryption on seals the ledger on every store, this machine's local
history, snapshots and outbox, and its shared history and snapshot objects,
with XChaCha20-Poly1305 under a random data key. A sealed file that has been
altered fails to open. The key is wrapped under the passphrase and a recovery
code (Argon2id), so another machine unlocks with either, typed carelessly,
and keeps the key in its keychain so it is not asked again. A locked machine
refuses every write rather than writing plain, and refuses to treat a sealed
local file as empty. Turning it off needs the passphrase and leaves
everything plain.

## Why
A ledger is the most sensitive file a household has. Encryption is only
worth having if one slip (a plain outbox, a history file rewritten unread)
cannot undo it.

## Check
```bash
cargo test -p ledger-store sealed
cargo test -p ledger-app encryption_tests
grep -q 'ledger_store::Sealed::wrap(build_store(first, secrets)?, vault.clone())' crates/ledger-config/src/lib.rs
grep -q 'Outbox::sealed(places.outbox(), vault)' crates/ledger-config/src/lib.rs
npm --prefix ui run check
```

## Depends On
None
