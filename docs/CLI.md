# `hl`: Home Ledger from the command line

`hl` reads and changes the same ledger the app does. Every figure comes from
the same code the app's screens use, and every edit goes through the same
rules and lands in the same change history, so a script can do anything the
app can and nothing it could not.

On a computer with the Home Ledger app installed, `hl` uses the app's own
setup: the same stores, the same keychain, the same machine name. On one
without, `hl init` sets it up. The installer can also make `ledger` an alias
for `hl`.

## Installing

`hl` comes with the app.

- **Windows:** the installer puts `hl.exe` beside the app and adds that folder
  to your PATH; a new terminal finds it. A checkbox on the installer's first
  page also makes `ledger` a second name for it. Only that one entry is
  ever added or removed, and the PATH it found is copied to
  `HKCU\Software\home-ledger\path-backup` first.
- **Linux:** the .deb and .rpm install it as `/usr/bin/hl`.
- **The `ledger` name, any time:** `hl alias on` (or `off`, or `status`). On
  Windows this is `ledger.cmd` beside `hl.exe`, and the installer keeps your
  choice through updates; on Linux and macOS, a link in `~/.local/bin`.

The app's in-app updates replace `hl` with the app, so the two never drift
apart.

## Conventions

- **`--json`** prints the answer as JSON in a stable shape, the same
  structures the app's screens are given. Without it, tables for a person.
- **Names or ids.** Anything can be named by its name, in any case, as long as
  only one thing has that name, or by its id. An ambiguous name stops with
  the ids that match.
- **`--dry-run`** on any edit shows what it would change and changes nothing.
- **`--via LABEL`** labels a script in the history: "Office PC · cli ·
  nightly-import".
- **Amounts** are plain decimals: `1234.56`, `$1,234.56`, `-12`.
- **The passphrase** of an encrypted ledger is read from `LEDGER_PASSPHRASE`
  or asked for in a terminal, never taken as an argument, which other users of
  the machine can see.
- **`HOME_LEDGER_DIR`** points `hl` (and the app) at a separate folder of
  settings and data: a second ledger, or a test copy that never touches the
  real one.
- Printing into a pipe that closes early (`hl history | head`) stops quietly.

## Exit codes

| Code | Means |
|---:|---|
| 0 | Done |
| 1 | Something went wrong |
| 2 | Used wrongly: an unknown command, an ambiguous name, a bad amount |
| 3 | The ledger is encrypted and this machine is locked: run `hl unlock` |
| 4 | Saved on this machine and queued, but the store could not be reached; it syncs later |
| 5 | Refused by a rule: a locked bucket, a statement that does not add up |

## Commands

### Setting up

| Command | Does |
|---|---|
| `hl status` | The stores and their health, sync, encryption, this machine's name and install id |
| `hl sync` | Pushes edits waiting on this machine to the store |
| `hl init --name "Basement Pi" --s3-bucket B --s3-region R [--s3-key K] [--aws-profile P]` | Sets up a machine without the app, against S3 (keys from `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY` without a profile) |
| `hl init --name "Basement Pi" --local /path/ledger.json` | The same, against a file |
| `hl name "Office PC"` | Renames this machine in the history |
| `hl unlock` · `hl lock` | Unlocks an encrypted ledger and keeps the key, or forgets the key here |
| `hl alias on\|off\|status` | Makes `ledger` a second name for `hl`, or stops |

### Reading

| Command | Shows |
|---|---|
| `hl summary` | Net worth, assets, debts, monthly income and budget |
| `hl accounts` · `hl accounts show <account>` | Accounts, or one in full with equity and what it secures |
| `hl buckets` · `hl buckets show <bucket>` | Buckets, or one with its target, funding, commitments and shortfall |
| `hl budget` · `hl income` · `hl goals` · `hl holdings` · `hl retirement` | As the app's screens |
| `hl statements` · `hl statements show <id or card>` | Card statements, or one with its charges and shortfalls |
| `hl spending [--period 1m\|3m\|6m\|1y\|all\|custom] [--status all\|open\|settled] [--from D --to D]` | The Spending report |
| `hl plan [--to 1y\|6m\|90d\|2027-06-01]` | Every bucket projected forward |
| `hl history [--since 7d] [--by "Laptop"] [--client cli]` | The change history, filtered |
| `hl export [--format json\|csv] [collection]` | The whole ledger, decrypted, or one collection as CSV |

### Changing

| Command | Does |
|---|---|
| `hl account balance <account> <amount>` | Sets a balance |
| `hl bucket add\|spend <bucket> <amount> [--note "…"]` | Adds to a bucket, or spends from it: a spend also leaves the account the bucket is kept in |
| `hl bucket link <bucket> --to <account>` · `hl bucket link --unlinked --to <account>` | Says which account a bucket's money is kept in; `--unlinked` does it for every bucket with none |
| `hl bucket set <bucket> <amount>` · `hl bucket move <bucket> <amount> --to <bucket>` | Sets a bucket's cash; moves between buckets, and between their accounts when they differ |
| `hl payday <earner> [--undo]` | One paycheck's worth into every bucket that earner funds |
| `hl statement new --card C --balance N --date D [--bucket-source A] [--spend-source A] [--no-account-moves]` | Starts a statement, taking its sources from the card's last one when not given |
| `hl statement import <statement> <file.csv> [--member All] [--bucket B]` | Adds the purchases from a bank export; payments and charges already there are left out |
| `hl statement import <statement> --from-bank [--member All] [--bucket B]` | Fetches the card's charges from its bank and adds the ones in the statement's dates, each to the bucket its merchant went to last |
| `hl statement settle <statement> [--cover Overflow\|everyday\|negative\|Groceries=Overflow]…` | Settles, saying where a short bucket's rest comes from |
| `hl statement undo <statement>` | Undoes a settle exactly |
| `hl trade buy\|sell <holding or ticker> <quantity> <price>` | Records a trade |
| `hl prices refresh` · `hl prices set <ticker> <price>` | Refreshes prices from the quote service, or sets one by hand |
| `hl snapshot` | Takes today's net worth point, as the app does when it opens |
| `hl apply <file.json \| ->` | Any edit the app can make, in its own JSON form; a list applies each in turn |
| `hl import <ledger.json> [--replace]` | Reads a ledger from another client; shows what it holds unless `--replace` |
| `hl import-history --machine <name> [--file audit.json] [--dry-run]` | Brings in the Omarchy plugin's history |

### Bank connections

Through Plaid, with your own Plaid keys. See [BANK-CONNECTIONS.md](BANK-CONNECTIONS.md).

| Command | Does |
|---|---|
| `hl bank` | The keys' environment, each connected bank, its accounts and what they are linked to |
| `hl bank keys [--environment sandbox|production] [--client-id ID]` | Saves your Plaid keys once Plaid accepts them. The secret comes from `PLAID_SECRET` or a hidden prompt, never an argument |
| `hl bank keys --forget` | Removes them |
| `hl bank connect` · `hl bank connect --again <bank>` | Opens Plaid's sign-in in your browser and waits; or signs a connection in again |
| `hl bank link <bank account> <ledger account|none>` | Links a bank account, by name, last four digits or id |
| `hl bank fetch` | New transactions and balances from every connected bank |
| `hl bank balances [--apply]` | The bank's balances where they differ from the ledger's; `--apply` accepts them |
| `hl bank disconnect <bank>` | Ends the connection at Plaid and forgets it here |

Bank edits are labelled `plaid` in the history: "Office PC · cli · nightly · plaid".

## Who made a change

Every entry in the history records the machine's name, which program made the
change (`desktop`, `cli`, `mobile`, or `plugin` for history brought in from the
Omarchy plugin), the install's permanent id, the app version, and a script's
`--via` label. Two machines left with the same name are still told apart by
their install id. On a machine with the app, `hl` shares the app's install, so
"Office PC · desktop" and "Office PC · cli" are visibly the same computer.

## Running alongside the app

The app and `hl` can run at once. Writes to the store are conditional, so
neither can overwrite the other's edit, and the queue of edits waiting to
sync is locked across processes, so neither can lose one the other queued.
A command that changes something waits for its history to be shared before
it exits, so a script never quits mid-upload.

## Examples

```sh
# Each payday, from a scheduled task
hl payday Chris --via payroll

# Catch a statement up from the bank's export, then settle it
hl statement import "Chase Sapphire" ~/Downloads/Chase.csv --bucket Groceries
hl statement settle "Chase Sapphire" --cover Overflow

# Net worth, for another tool
hl --json summary | jq -r .net

# Catch a statement up from the bank, every morning
hl statement import "Chase Sapphire" --from-bank --via morning

# What changed this week, made by scripts
hl history --since 7d --client cli
```
