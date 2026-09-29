# Where this stands

Written 2026-09-22. A running account of what this app is for, what works,
what does not, and the things that were got wrong on the way — because several
of them looked entirely correct at the time.

For the architecture, read [STORAGE.md](STORAGE.md). For the porting rules and
the deliberate departures from the prototype, read [PORT.md](PORT.md).

---

## What this is

A desktop household ledger for **Windows and Linux**, replacing a pair of
prototypes that both still work and are both temporary:

- an **Omarchy/Hyprland plugin** (QML + a Python helper) — the daily driver
- a **web app on a Raspberry Pi** (vanilla JS over `core/Model.js`)

Both read and write one JSON document in S3. This app is the third client of
that same document, and eventually the only one.

### The six decisions behind it

Each of these chose the harder path deliberately. They are not open questions.

1. **Tauri + Rust, not Electron.** The point is a lean binary — currently
   6.8MB — and no garbage-collected runtime sitting in memory.
2. **The Python helper (2,931 lines) ports to Rust**, rather than shipping a
   Python sidecar.
3. **`core/Model.js` (2,188 lines) ports to Rust too**, rather than keeping
   proven JavaScript in the webview. Rust owns *every* number; the interface
   only renders what it is handed.
4. **Svelte 5 + TypeScript** for the interface — a real rewrite, not a port of
   the vanilla client.
5. **Strict primary with a durable outbox** for storage, not multi-master.
   Only the primary is ever written, so two copies cannot diverge. Edits queue
   locally when it is unreachable and replay later.
6. **A clean break.** No shared package with the prototypes. They freeze and
   are retired.

### Two standing constraints

- **Apache-2.0.** Chosen so that contributions arrive under permissive terms
  and the owner keeps every option for later releases, including closed ones.
- **No personal identity anywhere** — not in the data, not in the device
  label, not in git authorship. There is a regression test for the device
  label, and a kb claim for the data.

---

## What works

Roughly 19,100 lines across the Rust crates and the Svelte interface, 277
tests, 25 kb claims.

### The core

| Crate | Does |
|---|---|
| `ledger-domain` | Records, the document, `Money`, lineage |
| `ledger-math` | Every derived figure — rollups, splits, P/L, projections |
| `ledger-writer` | The ops that change the document, and the audit trail |
| `ledger-store` | Local, S3, Google Drive, NAS; the outbox and fallback engine |
| `ledger-config` | Stores, secrets, and building an engine from them |

**Money is `Decimal`, never `f64`.** Rounding is half away from zero, because
banker's rounding turns `1.005` into `1.00` and that reads as a bug to a
person. Splitting an amount between earners distributes the remainder cents
rather than rounding each share, so the shares always add back up — a sweep
over five hundred amounts asserts it.

### Storage

S3 is the **source of truth**, signed with an AWS profile read from
`~/.aws/credentials` at request time, so no second copy of the key exists in
the keychain. The local file is a **read-only mirror**, refreshed after every
successful write. Bucket versioning is on, so every write keeps the previous
copy.

Writes are conditional (compare-and-swap). A refused edit changes nothing. An
unreachable store is a different answer from an empty one — the prototype
treated a missing file as "nothing stored yet", which for an unmounted network
share would have proposed wiping the ledger.

### The screens

Nine tabs: Accounts, Holdings, Retirement, Income, Budget, Savings, Reconcile,
History, Storage. Each list-shaped tab toggles between cards and a table.
Colour means something and only that: a name is white, green is money held,
red is money owed, a pill's colour is its category.

- **Holdings** — add, edit, delete, buy and sell (average-cost accounting), and
  price by hand. Click a name for its chart (Yahoo's free endpoint,
  5D/1M/1Y/5Y), the position, the trades, and a note when the ledger's price has
  drifted from the market.
- **Retirement** — each account's plan: monthly contribution, automatic top-ups
  and target weights. Top-ups due are added once at launch, as the prototype
  does, and on demand. Plus the plugin's projection chart: 4, 6, 8 and 10% to
  the target year set in Settings > Retirement (per machine, seeded once from
  the plugin's prefs), with the same hover, rate cards and footnote. The
  figures match the plugin to the dollar; the cents differ as PORT.md records.
- **Reconcile** — start and edit statements with their charges, settle one
  (buckets, source accounts and the card all move together), undo a settle
  exactly, delete an open one. Settle, undo and delete all ask first.
- **Share prices** — a Finnhub key in the keychain, and a refresh that quotes
  every holding with a symbol.

---

## What does not work yet

Until the first two are done this cannot be the primary.

### 1. Smaller writer gaps

Holdings, retirement plans and statements are writable as of 2026-09-22 (every
`check-helper.py` and `check-credit.py` case for them is a Rust test). Still
missing from the writer:

- **Goals and templates have no screens.** The writer can set, delete and
  activate them (the Goals and Planning views in gap 2 will use it).
- **The new forms have not been clicked through.** They pass `svelte-check` and
  the launch-time top-up was exercised end to end against a throwaway local
  store, but no form has been submitted in the running app.

### 2. Nothing else the prototypes can do

Four views have no equivalent here: **Dashboard, Goals, Planning, Spending**.

### 3. Unproven in anger

- The desktop app has **never written to the shared ledger**. The conditional
  write path is tested against the real bucket with a deliberately stale
  version, which proves the signing and the refusal — but a successful write
  has not happened. Note that the launch-time top-up will write the first time
  an automatic contribution is due, the same as the prototype.
- `settle-check` undoes and re-settles both settled statements in the real
  ledger, in memory, and both come back byte-identical — but no settle has been
  written from the app.
- **No holding in the real ledger has any trades**, so the trades list in the
  holding detail is type-checked and unexercised.
- The Finnhub refresh has **not been run** — it needs a key.

---

## Twists and turns

The theme: every bug that mattered was found by checking against something
independent, never by reading the code again. Each of these looked correct on
review.

### A balance sheet out by six figures

`net_worth` disagreed with the prototype on real data. Two causes, both of
which read as obviously right in isolation: a liability's typed total must
*replace* its debt lines rather than add to them, and holdings belonging to no
account still belong to the household.

### A SigV4 constant written from memory

A signing test failed. The constant recalled from memory was wrong and the
code was right. Both constants are now computed independently, with a comment
saying so.

### A retirement rollup a whole budget line short

The port counted a budget line as feeding a retirement account only when the
line named the account directly. The prototype counts three ways: directly,
through a bucket linked to the account, or by name inside a bucket called
"Retirement". On real data the monthly figure came out **nearly a quarter
low** — and every projection is built on that figure, so the whole screen
was low by the same proportion. Caught by extending the comparison harness,
not by reading the function.

### The prototype is the one that is wrong about projections

Compounding a balance 360 times in `float` drifts. At thirty years the
prototype comes out one to four cents low, growing with the rate. The port,
carrying the balance as a `Decimal`, agrees to the cent with the same sum
taken to fifty digits. This is recorded in [PORT.md](PORT.md) as a deliberate
disagreement, so the comparison harness showing it is not read as a regression
later. **The oracle is not always right — but it is always worth asking.**

### `{:.2}` on a `Decimal` truncates

It does not round. Found by chasing a disagreement of one hundredth of a
percent, which turned out to be real: a four-decimal share price displayed a
cent low everywhere it appeared. The first fix dropped the zero padding, and
its own test caught that.

### Drive was credited with a guarantee it does not have

The storage design claimed Google Drive supports `If-Match`. It does not. The
document was corrected and a weaker compare-and-swap level added, rather than
the code being written against a promise the provider never made.

### A history view over a file nobody wrote

The History tab was built before anything appended to the audit log. It
rendered an empty list correctly and cheerfully.

### Bucket moves that never parsed

Op fields were snake_case and the interface sends camelCase, so every
"move between buckets" was refused as an unknown edit before it reached the
writer. Every Rust test built the op in Rust, so none of them saw it.

### Three things lost on a write

A trade had no `id` field, so trade ids would have been dropped. An absent
`creditDelta` was written as `null`, so the real ledger stopped round-tripping
identically once it had settled statements. And share counts and prices were
displayed — and would have been re-saved from an edit form — at two places
instead of four.

### "cash $…" on a loan

Only visible by running against real data. The label was unconditional.

### A real name in git history

Authorship was rewritten and the repository recreated. The device label had
been taken from the machine's hostname; it now returns a platform string, with
a test.

### Process slips worth naming

- A `git checkout` to undo a mangled file silently reverted an op written
  minutes earlier. Redone, but it cost a rebuild.
- A gate command that summarised results with `grep` reported "clean" while
  three test constructors did not compile. **A summarised gate is not a gate.**

---

## How to check any of it

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix ui run check
```

Against the prototype, over one real document:

```bash
tools/compare.sh ../kairos.home-ledger ~/.local/state/kairos.home-ledger/ledger.json
cargo run -p ledger-math --example figures -- <ledger.json>        # includes the round trip
cargo run -p ledger-writer --example settle-check -- <ledger.json> # settle/undo, in memory
```

The first path is the prototype's own copy; `~/.local/share/home-ledger/` is
this app's local mirror, which can be behind it.

Every claim in `kb/` carries the command that proves it. They are the
shortest description of what this app promises.
