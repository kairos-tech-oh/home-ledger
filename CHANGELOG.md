# Changelog

What changed in each release, newest first. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/). Before 1.0, a minor version may
change anything.

The public history begins after 0.1.1, so the two releases before it are
described here but have no tags of their own.

## [Unreleased]

### Added
- Encryption, turned on in Settings with a passphrase. The ledger on every
  store, its shared history and snapshots, and the local files holding
  balances are encrypted; a recovery code opens them if the passphrase is
  forgotten, and each computer asks once then keeps the key in its keychain.

## [0.2.6] - 2026-09-30

### Added
- Payday buttons on the Savings page: "Add all" adds one paycheck of an
  earner's contributions to every bucket they fund, with an Undo.
- A warning when open statements together need more from a bucket than it
  holds. Settling one that is short asks where the rest comes from: another
  bucket, everyday spending, or letting the bucket go below zero. Each bucket
  can remember its answer.

## [0.2.5] - 2026-09-30

### Added
- Account sections open with the most accounts first, and Arrange puts them in
  any order. The order is kept in the ledger, so it lasts through updates and
  restarts and is the same on every machine.

## [0.2.4] - 2026-09-29

### Fixed
- The Linux AppImage opens on systems with a recent Mesa (current Arch, Fedora,
  Ubuntu 26.04). It carried its own old copy of libwayland, which a new Mesa
  cannot use, so the window crashed before it drew.

## [0.2.3] - 2026-09-29

### Added
- Clicking an account section's heading hides the accounts under it, and
  clicking again shows them. The heading keeps its count and subtotal.

### Changed
- Adding or editing an account opens a popup instead of a form at the top of
  the page.
- The Storage tab is now called Settings.

## [0.2.2] - 2026-09-29

### Fixed
- "Spent by" is a dropdown of the family's names, with "Someone else…" for a
  name not on the list. The suggestion box it replaces hid every name that did
  not match what it already said.

## [0.2.1] - 2026-09-29

### Added
- Import charges into an open statement from a bank or card CSV export. Columns
  and the sign of a purchase are worked out from the file and can be corrected;
  payments, refunds and charges already on the statement are left out; imported
  charges are attributed to All.
- Property and Vehicle accounts: what a home or car is worth, counted as an
  asset. Either can name the loan secured against it, and the Accounts page
  shows value, what is owed and the equity together.

## [0.2.0] - 2026-09-29

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
- A Dashboard, and the app opens on it: net worth with its 30-day change,
  chosen buckets, accounts and goals, retirement, reconciliation, cash flow,
  available credit, holdings and spending, laid out and customized the way the
  plugin does it and stored in the ledger.
- A daily net worth snapshot, shared between machines, and an import for the
  plugin's `snapshots.json` under Storage.
- Goals: a target and the bucket behind it.
- Planning: every bucket carried forward to a chosen date.
- Spending: what the card statements itemise, by person, item, month, card
  and bucket, and what settling took out of the buckets. Family names are
  offered on a statement's "spent by".
- Budget templates on the Budget tab.
- Updates: the app checks GitHub for a newer release on launch, and under
  Storage → About, and installs it after checking its signature.

### Changed
- The tabs are grouped under Dashboard, Wealth, Cash flow, Plans and Ledger,
  each opening on hover or click.
- Dropdowns are drawn in the app's own dark colours instead of white.

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
