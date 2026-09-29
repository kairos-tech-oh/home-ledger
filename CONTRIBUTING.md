# Contributing to Home Ledger

Thank you for wanting to help. Home Ledger keeps a household's money, so the
bar for a change is that it is **proven**, not just reviewed: most of the bugs
that mattered in this project read as correct and were only caught by checking
against something independent. See [docs/STATUS.md](docs/STATUS.md) for where
things stand and the story behind that rule.

Security problems are the one exception to everything below. Please report
them privately, as [SECURITY.md](SECURITY.md) describes, not as an issue.

## Before you start

- **For anything larger than a small fix, open an issue first** and say what
  you want to change and why. It saves you writing something that does not
  fit the design.
- The six decisions at the top of [docs/STATUS.md](docs/STATUS.md) are
  settled, not open questions: Tauri and Rust rather than Electron, Rust owning
  every number, Svelte for the interface, one primary store with a durable
  outbox, and a clean break from the prototypes.

## Setting up

You need [Rust](https://rustup.rs) (1.90 or newer) and Node 22 or newer. On
Linux, also the webview packages listed in the [README](README.md#building-it).
On Windows, the Visual Studio C++ build tools.

```sh
npm install && npm --prefix ui install
npm run dev      # the app, with the interface hot-reloading
npm run check    # everything CI checks
```

## The rules the code keeps

These are what the project promises. A change that breaks one will not be
merged, however good it is otherwise.

1. **Rust holds every number.** Validation, caps, the audit trail and every
   derived figure live in the Rust crates. The interface renders what it is
   given and never does arithmetic that a balance depends on.
2. **Money is `Decimal`, never a float.** Amounts are fixed-point, rounded half
   away from zero. A kb claim scans the domain and math crates for `f32` and
   `f64`.
3. **Only the primary store is written**, and every write is conditional. Read
   [docs/STORAGE.md](docs/STORAGE.md) before touching `ledger-store`, and
   [docs/ADDING-A-STORE.md](docs/ADDING-A-STORE.md) before adding a backend.
4. **An unreachable store is never an empty one.** Code that cannot read a
   store must say so, never fall back to "nothing stored yet".
5. **No personal identity anywhere:** not in the ledger, the history, the
   machine label, test data, fixtures, docs, or commit metadata. Use invented
   names and round numbers. Never commit a real ledger, or figures copied from
   one.
6. **Secrets stay in the OS keychain** or the AWS credentials file they came
   from, and never reach `config.json`, logs or the ledger.

## Making a change

### Tests

Every behaviour change comes with a test that fails without it. Name tests as
sentences that state the rule, the way the existing ones do:
`a_refused_edit_changes_nothing`, not `test_cas_2`.

Test against something independent when you can: a figure computed another
way, a sum taken to more digits, the provider's documented behaviour. A test
that restates the implementation proves nothing.

### kb claims

The `kb/` folder is the shortest description of what the app promises. Each
claim says what is true, why it matters, and gives the commands that prove it.
If your change adds a promise, add a claim; if it changes one, update the
claim. A claim is only useful if its check **fails when the promise breaks**,
so try breaking your change and make sure the claim notices.

```sh
tools/kb-check.sh                      # every claim
tools/kb-check.sh kb/your-claim.md     # just yours
```

Copy the front matter from an existing claim. `environment: any` means the
check must run on any machine with the repository and its dependencies, with
no personal files, accounts or other checkouts.

### Before you open a pull request

```sh
npm run check          # svelte-check, cargo fmt, clippy with -D warnings, tests
tools/kb-check.sh      # every kb claim
```

Both must pass. For a change to the interface, also run the app and use the
change; `svelte-check` proves it compiles, not that it works. Say in the pull
request what you clicked through.

### Commits

Write the subject as a plain sentence about what the change does, in the
imperative, without a prefix or a trailing full stop:

```
Lock a file store's compare-and-write across instances and processes
```

The body, when there is one, says why. Keep each commit to one change that
builds and passes on its own.

## Licensing of contributions

Home Ledger is licensed under the [Apache License 2.0](LICENSE). By submitting
a contribution — a pull request, a patch, or code in an issue — you agree that
it is licensed under the same terms, as section 5 of the license describes, and
that you have the right to contribute it. Only submit work that is yours, or
that you are entitled to license this way.

## Reporting a bug

Open an issue with the version (from the installer's name, or Add/Remove
Programs on Windows), the platform,
which store is primary, what you did, what you expected and what happened.
**Do not attach your ledger** or screenshots of real balances: describe the
shape of the data instead, or reproduce it with invented figures.
