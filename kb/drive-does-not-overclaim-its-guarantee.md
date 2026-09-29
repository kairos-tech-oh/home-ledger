---
id: home-ledger.drive-does-not-overclaim-its-guarantee
project: home-ledger
category: data
severity: critical
environment: any
depends_on: [home-ledger.core-tests-pass]
---

# Google Drive reports the guarantee it can actually keep

## Claim
`DriveStore` declares `Cas::CheckedAfterWrite`, never `Cas::Native`, and
reports a conflict when the file's version moved by more than this write.

## Why
Drive v3 has no `If-Match` on `files.update`, so a stale write cannot be
refused the way S3 refuses one. Claiming otherwise would tell someone their
data was safe from a concurrent write when it is not. Checking the version
before and after catches a clobber instead of preventing it, and Drive's own
revision history still holds what was overwritten.

## Check
```bash
cargo test -p ledger-store it_does_not_claim_a_guarantee_drive_cannot_keep
cargo test -p ledger-store a_version_that_jumped_means_someone_wrote_in_between
cargo test -p ledger-store an_unreadable_version_does_not_invent_a_conflict
cargo test -p ledger-store oauth::
```

## Depends On
[[core-tests-pass]]
