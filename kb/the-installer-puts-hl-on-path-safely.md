---
id: home-ledger.the-installer-puts-hl-on-path-safely
project: home-ledger
category: correctness
severity: error
environment: windows
depends_on: []
---

# The installer ships hl, puts it on PATH without risking PATH, and remembers `ledger`

## Claim
Every build stages `hl` for the bundler, which installs it beside the app.
The Windows installer adds the app's folder to the user's PATH once, and
leaves a PATH at NSIS's string limit untouched, because one read cut short and
written back would lose the rest of it. A checkbox on the Welcome page offers
`ledger` as a second name. The choice is kept in `HKCU\Software\home-ledger\hl`,
which `hl alias` also writes, so a page-less in-app update keeps it, and a
silent or passive install adds nothing new. Uninstalling removes the alias and
the PATH entry; the uninstall an update runs first does not.

## Why
A command line nobody can find is no use. A PATH damaged by an installer
breaks every other program on the machine, and nobody would think to blame
Home Ledger for it.

## Check
```bash
node -e "const c=require('./src-tauri/tauri.conf.json'); if(!c.bundle.externalBin.includes('binaries/hl')) throw 'hl not bundled'; if(c.bundle.windows.nsis.installerHooks!=='windows/hooks.nsh') throw 'no hooks'; if(!c.build.beforeBuildCommand.startsWith('node tools/release/stage-hl.mjs')) throw 'hl not staged';"
grep -q 'IntOp $R3 ${NSIS_MAX_STRLEN} - 2' src-tauri/windows/hooks.nsh
test "$(grep -c '\${If} \$R4 >= \$R3\|\${If} \$R4 < \$R3' src-tauri/windows/hooks.nsh)" = 2
grep -q '\${If} \$UpdateMode <> 1' src-tauri/windows/hooks.nsh
grep -q 'home-ledger\\hl' crates/hl/src/alias.rs
grep -qF 'Software\home-ledger\hl' src-tauri/windows/hooks.nsh
