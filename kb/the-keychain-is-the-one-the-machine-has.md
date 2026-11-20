---
id: home-ledger.the-keychain-is-the-one-the-machine-has
project: home-ledger
category: security
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# A secret written to the keychain is there when it is read back

## Claim
On every platform the app builds for, a secret written to the keychain can be
read back as the same secret. The keyring backend for the host platform is
compiled in, so `Keychain::available()` reporting true means a write will
actually be found again.

## Why
A keychain that accepts a write and returns nothing on the read is worse than
one that refuses, because the app reports success and the person believes
their credentials are saved. They then find out at the moment it matters: the
S3 keys are gone, or the data key that opens an encrypted ledger is gone, and
an encrypted ledger with no key is unreadable.

This is not hypothetical. `keyring` selects its backend by Cargo feature, and
with no feature for the host platform it silently falls back to an in-memory
store that is discarded when the process exits. Building on macOS without
`apple-native` produced exactly that: `available()` true, `set()` ok, `get()`
empty. The sibling claim
[[secrets-never-reach-the-config-file]] promises a secret never lands in a
file; this one promises it lands somewhere at all.

## Check
```bash
cargo run -q -p ledger-config --example keychain-check 2>/dev/null | grep -qx "read back: matches"
```

The example writes a throwaway entry, reads it back, and removes it. It
reports `read back: matches` only when the round trip worked, and says what
went wrong otherwise — `nothing there, which should not happen` for the
missing-backend case. The grep is what makes this a gate: the example itself
exits zero either way, because it is also meant to be run by hand on a
machine being diagnosed.

## Depends On
[[core-tests-pass]]
