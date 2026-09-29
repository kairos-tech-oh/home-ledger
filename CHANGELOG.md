# Changelog

What changed in each release, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Before 1.0, a minor version may
change anything.

The public history begins after 0.1.1, so the two releases before it are
described here but have no tags of their own.

## [Unreleased]

### Added
- Licensed under the Apache License 2.0.
- The savings page lists buckets largest first, in both views and in the
  "move into" choices.
- Accounts are grouped into one section per kind of account, each with its
  count and subtotal.
- Budget sections roll up and back when their heading is clicked, in either
  view.
- History actions are coloured by kind: create, add, update, remove and move
  each have their own colour.
- A new application icon.
- `CONTRIBUTING.md`, `SECURITY.md`, this changelog, and `tools/kb-check.sh`
  to run every kb claim.

### Changed
- The savings and accounts pages open in the card view.
- The live S3 test reads its bucket, region and profile from
  `LEDGER_LIVE_BUCKET`, `LEDGER_LIVE_REGION` and `LEDGER_LIVE_PROFILE`.

### Fixed
- A long name no longer runs over the edge of its card and across the next
  one.

## [0.1.1] - 2026-09-29

### Fixed
- A file store's compare-and-write is locked across app instances and
  processes, so two running copies cannot both write.
- The history import's dry run no longer writes anything.

## [0.1.0] - 2026-09-29

The first release: a desktop client for an existing ledger document, for
Windows and Linux.

### Added
- Nine screens: accounts, holdings, retirement, income, budget, savings,
  reconcile, history and storage, each list with a card and a table view.
- Storage on this computer, S3, Google Drive or a NAS share, with one primary,
  mirrors, and a durable outbox for edits made while the primary is
  unreachable. Every write is conditional.
- Credentials in the OS keychain, or read from an AWS profile at request time.
- Holdings with buy and sell, share prices from Finnhub, and price charts.
- Retirement plans, automatic top-ups and a projection to a target year.
- Statements that settle and undo exactly.
- A change history shared between machines through the primary store, each
  entry naming the machine that made it.
- Import of a ledger written by another client, and a one-off import of the
  prototype plugin's history.

[Unreleased]: https://github.com/kairos-tech-oh/home-ledger/commits/main
