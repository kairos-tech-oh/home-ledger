---
id: home-ledger.a-profile-is-read-not-copied
project: home-ledger
category: security
severity: high
environment: any
depends_on: [home-ledger.secrets-never-reach-the-config-file]
---

# An AWS profile is read where it lives, never copied into the keychain

## Claim
An S3 store carrying `awsProfile` reports `needs_secret() == false`, and its
credentials are read from `~/.aws/credentials` when the engine is built. A
store without one is unchanged: the field is absent from the written config
entirely, and the keychain is still the only place its key lives.

## Why
A machine that already has a working profile should not end up with a second
copy of the same secret in a second place — two copies means two things to
rotate and two things to leak. The file is refused unless it is a regular file
private to its owner, the same rule the prototype applies.

## Check
```bash
cargo test -p ledger-config a_bucket_signed_by_an_aws_profile_keeps_nothing_in_the_keychain
cargo test -p ledger-config a_bucket_with_no_profile_writes_no_profile_field_at_all
cargo test -p ledger-config a_named_profile_is_read_rather_than_the_first_one_in_the_file
cargo test -p ledger-config half_a_profile_is_an_error_rather_than_an_unsigned_request
```

## Depends On
[[secrets-never-reach-the-config-file]]
