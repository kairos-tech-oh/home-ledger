# Porting the ledger to Rust

This app replaces a working prototype: an Omarchy desktop plugin plus a web
app, sharing `core/Model.js` for derived figures and `helper/ledger.py` for
every mutation. Both are proven against real household data and both have
fixture suites. Neither is thrown away — they are the oracle the port is
checked against.

## What is being ported

| Source | Lines | Lands in | Done |
|---|---:|---|---|
| `helper/ledger.py` | 2,931 | `ledger-writer` | every op except `dashboard-set`, `prefs` and the snapshot history |
| `core/Model.js` | 2,188 | `ledger-math` | balance sheet, monthly totals, earners, splits, buckets, planned draws |
| `core/Spending.js` | 194 | `ledger-math` | no |
| `core/Sanitise.js` | 53 | `ledger-domain` | yes |
| record schemas | — | `ledger-domain` | yes |

The document shape is ported in full and verified against a real ledger:
20 accounts, 43 holdings, 39 budget lines, 23 buckets, 2 reconciliations
read, computed and written back **byte-identically**.

## Checking a port against the original

```sh
tools/compare.sh ../kairos.home-ledger ~/.local/state/kairos.home-ledger/ledger.json
```

Runs both implementations over the same document and prints their figures
side by side. The round-trip check — any field a read and write would lose,
add or retype, by path only — is in the `figures` example:
`cargo run -p ledger-math --example figures -- <ledger.json>`.

Use it after porting anything that produces a number. It has already earned
its keep: the first `net_worth` disagreed with the prototype by **six figures**
on real data, from two bugs that read perfectly well in isolation —
liability accounts counted both a typed balance and their itemised debt
lines, and holdings were left out of assets entirely.

## The rule that makes this safe

**Port a function, then prove it agrees with the original.** The prototype's
suites (`tools/check-helper.py`, `tools/check-math.mjs`, `tools/check-credit.py`,
`tools/check-spending.py`) encode behaviour that was arrived at by fixing real
bugs. Each is a list of cases with expected answers, so each becomes a Rust
test with the same inputs and the same expectations.

Where a case is genuinely ambiguous, the prototype wins and a comment says why.
Where the prototype is wrong, the Rust version changes and the test records
what changed and when.

## Deliberate departures

These are places the port does **not** reproduce the original, on purpose.

**Money is decimal, not `float`.** The Python used `round(n, 2)` over binary
floats. Exact for a single amount, lossy in aggregate. `Money` is fixed point.

**Rounding is half away from zero.** Python's `round()` and `rust_decimal`'s
default are both banker's rounding, so `1.005` becomes `1.00`. For money shown
to a person that reads as a bug.

**Splitting is not rounding.** Dividing one amount between earners by rounding
each share independently loses or invents a cent. `ledger_math::split`
distributes the remainder instead, and a sweep over five hundred amounts
asserts the shares always add back up. The prototype's per-share rounding is
the thing being corrected, so this is the one place the two deliberately
disagree.

**Formatting a Decimal with `{:.2}` truncates.** It does not round. Found by
chasing a one-hundredth of a percent disagreement with the prototype, which
turned out to be a real bug: a four-decimal share price displayed a cent low
everywhere it appeared.

**Compounding a projection drifts in `float`.** `projectBalance` multiplies a
balance by `1 + rate/12/100` three hundred and sixty times for a thirty-year
projection, and neither that factor nor the running balance is exact in binary.
The port carries the balance as a `Decimal` held at eight places and agrees to
the cent with the same sum computed at fifty digits; the prototype comes out one
to four cents low, growing with the rate. Checked at 6%, 8% and 10% on the real
ledger. This is the second place the two deliberately disagree, and `compare.sh`
will keep showing it.

**A missing directory is not an empty store.** The prototype read a missing
file as "nothing stored yet". For a network share that is dangerous: an
unmounted share looks identical to an empty one, and adopting "empty" proposes
wiping the ledger. Unreachable and empty are now different answers.

**Every record field tolerates absence.** Documents predate the fields later
versions added — a real goal had no `targetDate`, and the first cut of the
schema refused to parse the whole ledger because of it. Records deserialise
with `#[serde(default)]` at the container level so one missing field can
never make a document unreadable.

**The device label is never the hostname.** Machine names routinely contain a
person's real name, and this value is written into the document and synced
off the machine.

**A trade's history entry shows the old average cost as "from".** The
prototype assigned the new average before building the entry, so its "from"
and "to" were the same figure.

**The audit records who, not just what.** Each entry names the machine that
made it, and history is shared through the source of truth so every machine
sees every other's edits (see STORAGE.md, "The change history").

## Order of work

1. **Domain and storage.** Done — records, document, `Store`, outbox, engine.
2. **The writer's core ops.** Done — records, bucket ops, holdings and trades,
   retirement plans and top-ups, statements with settle and undo.
3. **The math the main screen needs.** Rollups, earner splits, bucket totals.
4. **The remaining surfaces.** Holdings and P/L, planned draws, goals,
   templates, retirement projection, reconciliation, spending analysis.
5. **The remote stores.** S3 first — the prototype's hand-rolled SigV4 is
   proven against the real bucket and is the model. Then Google Drive, then NAS.
6. **Import.** Read a prototype `ledger.json` so no data is retyped.

## What the prototype keeps doing meanwhile

Nothing is switched off. The plugin and the web app continue against their own
S3 object until this app can do everything they can, and the import path is
what moves the data across when that day comes.
