---
id: home-ledger.secrets-never-reach-the-config-file
project: home-ledger
category: security
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Credentials live in the keychain, never in a file

## Claim
The configuration types have no field capable of holding a secret, and a
machine with no usable keychain reports its own error rather than falling
back to writing a key in the clear.

## Why
Bring-your-own-storage means real cloud credentials on every user's machine.
A config file is backed up, synced and pasted into support requests; a
keychain is not. The silent-downgrade path is the dangerous one, because
nobody finds out until it matters.

## Check
```bash
cargo test -p ledger-config there_is_nowhere_in_the_config_to_put_a_secret
cargo test -p ledger-config a_missing_keychain_is_its_own_error_rather_than_a_silent_fallback
cargo test -p ledger-config an_s3_store_without_credentials_says_so_by_name
```

## Depends On
[[core-tests-pass]]
