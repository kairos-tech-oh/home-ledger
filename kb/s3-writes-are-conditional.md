---
id: home-ledger.s3-writes-are-conditional
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# The S3 store signs correctly and refuses a stale write

## Claim
SigV4 signing matches known vectors, every header sent is covered by the
signature, and a `PUT` carrying a stale version is refused rather than
overwriting.

## Why
Hand-rolled signing fails as an opaque 403 that says nothing about which step
was wrong, so the chain is checked against RFC 4231 and a published AWS
scope. The conditional write matters more: without it two machines writing at
once silently lose one of the edits, and no care elsewhere in the app can
recover it.

## Check
```bash
cargo test -p ledger-store sigv4::
cargo test -p ledger-store a_refused_key_is_reported_as_denied_not_as_a_network_problem
cargo test -p ledger-store a_service_outage_is_transient_so_the_outbox_holds
cargo test -p ledger-store the_clock_conversion_matches_known_instants
```

## Depends On
[[core-tests-pass]]
