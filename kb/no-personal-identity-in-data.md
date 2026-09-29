---
id: home-ledger.no-personal-identity-in-data
project: home-ledger
category: privacy
severity: warn
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Nothing the app records is taken from the machine's identity

## Claim
`device_name()` does not read `COMPUTERNAME` or `HOSTNAME`, and no tracked
source file contains a personal name.

## Why
The device label is written into audit entries and into the document itself,
then synced to whatever store is configured. Machine names routinely contain
a person's real name, so defaulting to the hostname publishes it.

## Check
```bash
! sed '/#\[cfg(test)\]/,$d' src-tauri/src/state.rs | grep -qE 'COMPUTERNAME|HOSTNAME'
cargo test -p home-ledger the_device_name_never_comes_from_the_environment
```

## Depends On
[[core-tests-pass]]
