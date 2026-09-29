# Home Ledger

A household balance sheet as a desktop application, for Windows and Linux.

Your data is yours and lives where you tell it to: a file on this computer, an
S3 bucket, Google Drive, a NAS share, or several of those with one of them in
charge and the rest as fallbacks.

> **Status: early, and not yet safe as your only record.** The foundation is
> built and tested — the domain model, every derived figure, the storage layer
> with its fallback logic, and the app shell — but some screens are missing and
> some paths have not been exercised against real data. Keep a backup, and turn
> on versioning if your store offers it. See [docs/STATUS.md](docs/STATUS.md)
> for exactly what works and what does not.

## Why it is built this way

**Rust holds every number.** Validation, caps, the audit trail, and every
derived figure live in Rust, not in the interface. The window is a renderer,
so a bug in the view cannot corrupt a balance, and anything headless — a CLI,
a scheduled report — gets identical arithmetic for free.

**Money is decimal, never a float.** `0.1 + 0.2` is not `0.3` in binary
floating point, and a savings bucket adjusted a few thousand times accumulates
the error. Amounts are fixed-point, rounded half away from zero because that is
what a person expects when they look at their own money.

**It is small.** A release build is a single native binary with a system
webview; there is no bundled browser and no bundled runtime.

## Where your data lives

One store is the **primary** and is the only one ever written. Any others are
**mirrors**, kept current and read only if the primary cannot be reached.

That is a deliberate choice. Two stores that can both accept writes will
eventually both hold edits the other does not, and merging two divergent copies
of one document is not a solvable problem in general. Keeping a single writer
means divergence cannot happen.

So when a store is unreachable, edits do not fail and they do not go somewhere
else. They queue in a durable **outbox** on this machine and land on the
primary when it returns, replayed onto whatever it holds by then.

| Store | Compare-and-swap | Notes |
|---|---|---|
| This computer | local lock | Safe for this machine. Nothing else can write it. |
| S3 | **native** | `If-Match`. Safe for several machines at once. |
| Google Drive | **native** | Revision `If-Match`. Safe for several machines. |
| NAS / network share | best effort | SMB and NFS do not lock dependably. Fine as a mirror; refused as a primary unless you say so explicitly. |

The change history travels the same way: each install publishes its own
history object to the primary, and every machine's History tab shows them all,
labelled with the machine that made each edit. See
[docs/STORAGE.md](docs/STORAGE.md).

A store that cannot promise a real compare-and-swap is not allowed to be the
primary by default, because a fallback chain that treats a network share and an
S3 bucket as equivalent will quietly lose writes on the share.

## Building it

Requires [Rust](https://rustup.rs) and Node 22+.

```sh
npm install && npm --prefix ui install
npm run dev      # run it, with the UI hot-reloading
npm run build    # produce installers for this platform
npm run check    # types, formatting, lints and tests
```

On Linux you also need the webview development packages:

```sh
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf
# Arch
sudo pacman -S webkit2gtk-4.1 gtk3 librsvg
```

Windows installers are built in CI on a Windows runner rather than
cross-compiled, because NSIS, the WebView2 bootstrapper and code signing all
misbehave under cross-compilation. Pushing a `v*` tag drafts a release with the
installers attached.

## Layout

```
crates/ledger-domain   the shapes a ledger is made of, and how input is cleaned
crates/ledger-math     every derived number; pure functions, no I/O
crates/ledger-writer   the only thing that changes a ledger; validation + audit
crates/ledger-store    where it lives: the Store trait, outbox, fallback engine
crates/ledger-config   configured stores, secrets, and building an engine from them
src-tauri              the desktop shell and the commands the UI may call
ui                     Svelte 5 + TypeScript; renders what Rust computes
```

## Contributing

Contributions are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers setting
up, the rules the code keeps, and how a change is proven. Changes by release
are in [CHANGELOG.md](CHANGELOG.md).

Please report security problems privately, as [SECURITY.md](SECURITY.md)
describes, rather than in a public issue.

## License

Home Ledger is licensed under the [Apache License 2.0](LICENSE).
