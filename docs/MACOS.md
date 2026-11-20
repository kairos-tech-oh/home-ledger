# Building and running on macOS

macOS is **not a release target**. The installers CI publishes are Windows and
Linux, as [the README](../README.md) says, and nothing built here is signed or
notarised. What this document covers is building the app from source on a Mac
and developing on it, which works: the whole Rust core, the Tauri shell and the
`hl` CLI compile and run on Apple Silicon with no changes to the source.

If you want the app rather than a development setup, take a Windows or Linux
installer from the [releases page](https://github.com/kairos-tech-oh/home-ledger/releases).

## The short way

```sh
tools/macos-install.sh --check    # say what is missing, change nothing
tools/macos-install.sh            # set up, then build Home Ledger.app
tools/macos-install.sh --dev      # set up, then run it with the UI hot-reloading
```

The script installs nothing without printing the command and asking first, and
it only offers the Homebrew route when Homebrew is already present. Installing
Homebrew, or a toolchain, is your decision and not the script's.

## The long way

You need three things. The system webview is WKWebView, so **none** of the
`libwebkit2gtk` packages the README lists for Linux apply here.

| | Why | Install |
|---|---|---|
| Xcode Command Line Tools | the linker and system headers | `xcode-select --install` |
| Rust 1.90 or newer | the whole core | [rustup.rs](https://rustup.rs), or `brew install rustup-init && rustup-init` |
| Node 22 or newer | the Svelte interface and the Tauri CLI | `brew install node`, or [nodejs.org](https://nodejs.org) |

Then:

```sh
npm install && npm --prefix ui install
npm run dev
```

`npm run dev` needs no macOS-specific flags. It stages a debug `hl` beside the
app first, naming it for the host triple (`hl-aarch64-apple-darwin`), which is
what `externalBin` in `src-tauri/tauri.conf.json` expects.

## Building a bundle

`npm run build` on its own **fails on a Mac**, and that is not a bug. The
`bundle.targets` in `src-tauri/tauri.conf.json` are `nsis`, `msi`, `appimage`,
`deb` and `rpm` — every one of them Windows or Linux. There is also no macOS
entry in `bundle.icon`, so the bundler cannot find an icon it can use and stops
with ``Failed to create app icon: `No matching IconType` ``.

`tools/release/macos.conf.json` supplies both, the same way
`tools/release/ci-config.mjs` layers release settings over the base config:

```sh
# Home Ledger.app
npm run build -- --config tools/release/macos.conf.json --bundles app

# .app and .dmg
npm run build -- --config tools/release/macos.conf.json
```

The result is `target/release/bundle/macos/Home Ledger.app` — about 10 MB,
arm64, ad-hoc signed, with `hl` inside it at `Contents/MacOS/hl`. Run it with
`open`, or copy it into `/Applications`.

### The .dmg needs permission to drive Finder

Tauri builds a `.dmg` with `bundle_dmg.sh`, which uses AppleScript to position
the icons in the window. The first time it runs, macOS asks whether the
terminal may control Finder. Without that permission the step fails:

```
execution error: Finder got an error: A privilege violation occurred. (-10004)
failed to bundle project: error running bundle_dmg.sh
```

Grant it in System Settings → Privacy & Security → Automation, under your
terminal, and build again. The `.app` is unaffected and needs no permission —
build that instead if you only want to run the thing.

## What is not signed

The binary is **ad-hoc signed** (`codesign -dv` reports `adhoc,linker-signed`).
It runs on the machine that built it. It is not notarised, so copying it to
another Mac means Gatekeeper refuses it until that person clears the quarantine
flag by hand. There is no Apple Developer certificate wired into this project,
and `docs/RELEASING.md` describes signing only for Windows.

In-app updates do not apply either. The updater endpoint in
`src-tauri/tauri.conf.json` serves the Windows and Linux artifacts a release
publishes; a Mac build checks it, finds nothing for its platform, and carries
on. Rebuild from source to update.

## Running the checks

`npm run check` passes on macOS unmodified — `svelte-check`, `cargo fmt`,
`cargo clippy -D warnings` and the full test suite, 515 tests.

```sh
npm run check
```

`tools/kb-check.sh` is a different story: it uses `mapfile`, a bash 4 builtin,
and macOS ships bash 3.2 as `/bin/bash`. On a stock Mac it fails immediately
with `mapfile: command not found` and checks nothing. Install a current bash
and put it ahead of the system one:

```sh
brew install bash
PATH="$(brew --prefix)/bin:$PATH" tools/kb-check.sh
```

Since CONTRIBUTING.md asks for both gates before a pull request, a Mac
contributor needs this.

## The keychain

`keyring` picks its backend with a Cargo feature, and the features named in
`crates/ledger-config/Cargo.toml` decide which platforms get a real one. With
no feature for the host platform it falls back to an in-memory store that is
thrown away when the process exits — while still reporting that a keychain is
available and that a write succeeded.

On macOS that means credentials appear to save and are gone on restart: the S3
keys, the Plaid keys, and the data key that opens an encrypted ledger. The last
one matters most, because an encrypted ledger whose key is gone cannot be read.

`apple-native` is in the feature list, so this works. The round trip is a kb
claim — [the keychain is the one the machine has](../kb/the-keychain-is-the-one-the-machine-has.md) —
and you can check any machine with:

```sh
cargo run -q -p ledger-config --example keychain-check
```

`read back: matches` is the answer you want. `read back: nothing there, which
should not happen` means the backend for this platform was not compiled in.

## Known differences on a Mac

**The window is not transparent.** `app.windows[0].transparent` is `true` in
`src-tauri/tauri.conf.json`, and on macOS Tauri also needs
`app.macOSPrivateApi` plus the crate's `macos-private-api` feature for that to
take effect. Neither is set, because turning it on means editing
`src-tauri/Cargo.toml` and the private API it relies on is not something to
enable for a platform nobody ships. The window gets a normal opaque background.
Nothing else is affected.

**`hl alias on` puts the link in `~/.local/bin`**, as it does on Linux
(`docs/CLI.md` says so already). Make sure that directory is on your PATH; the
app does not add it for you, and the Windows installer's PATH handling is
Windows-only.
