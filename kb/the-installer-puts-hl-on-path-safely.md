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

The Windows installer never reads or writes PATH itself. It runs
`hl install-path add|remove`, which reads and writes the user's PATH whole
through the registry API and keeps the value's type. It adds or removes only
the app's folder, and refuses to write anything other than the old value
with that one entry added or removed. It copies the old value to
`HKCU\Software\home-ledger\path-backup` first.

A checkbox on the Welcome page offers `ledger` as a second name. The choice
is kept in `HKCU\Software\home-ledger\hl`, which `hl alias` also writes, so
a page-less in-app update keeps it, and a silent or passive install adds
nothing new. Uninstalling removes the alias and the PATH entry; the
uninstall an update runs first does not.

## Why
0.2.7 edited PATH in NSIS. Past NSIS's string limit, `ReadRegStr` returns an
empty string, not a cut-short one. The script took that for an empty PATH
and replaced a real user's whole PATH with the app's folder, which breaks
every other program on the machine. NSIS has no safe way to hold a long
PATH, so it no longer tries.

## Check
```bash
node -e "const c=require('./src-tauri/tauri.conf.json'); if(!c.bundle.externalBin.includes('binaries/hl')) throw 'hl not bundled'; if(c.bundle.windows.nsis.installerHooks!=='windows/hooks.nsh') throw 'no hooks'; if(!c.build.beforeBuildCommand.startsWith('node tools/release/stage-hl.mjs')) throw 'hl not staged';"
cargo test -p hl install_path
test "$(grep -v '^ *;' src-tauri/windows/hooks.nsh | grep -c 'HKCU "Environment"\|WriteRegExpandStr\|ReadRegStr .*Path')" = 0
grep -qF 'nsExec::ExecToLog '"'"'"$INSTDIR\hl.exe" install-path ${ACTION} "$INSTDIR"'"'" src-tauri/windows/hooks.nsh
grep -q '!insertmacro HlPath add' src-tauri/windows/hooks.nsh
grep -q '!insertmacro HlPath remove' src-tauri/windows/hooks.nsh
grep -q '\${If} \$UpdateMode <> 1' src-tauri/windows/hooks.nsh
grep -qF 'home-ledger\hl' crates/hl/src/alias.rs
grep -qF 'Software\home-ledger\hl' src-tauri/windows/hooks.nsh
```
