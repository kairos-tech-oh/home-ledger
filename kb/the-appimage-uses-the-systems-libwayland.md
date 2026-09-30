---
id: home-ledger.the-appimage-uses-the-systems-libwayland
project: home-ledger
category: correctness
severity: error
environment: ci
depends_on: []
---

# The AppImage uses the system's libwayland, so it starts on hosts with a new Mesa

## Claim
Before the AppImage is bundled, the release workflow seeds Tauri's tool cache
with linuxdeploy's GTK plugin, pinned by commit and SHA-256, with a line
appended that deletes `libwayland-*.so*` from the AppDir. After bundling,
`tools/release/appimage-check.sh` fails the job if the AppImage still holds
any `libwayland-*`. See `docs/RELEASING.md`.

## Why
A bundled libwayland older than the host's Mesa makes EGL fail, and WebKit's
web process aborts before the window draws. On this project's Arch/Intel
machine the shipped 0.2.2 AppImage aborted with `EGL_BAD_ALLOC`; the same
image with its four `libwayland-*` removed opened with no crash.

## Check
```bash
bash -n tools/release/appimage-gtk-plugin.sh
bash -n tools/release/appimage-check.sh
grep -q "find \\\\\"\\\\\$APPDIR\\\\\" -name 'libwayland-\*.so\*' -print -delete" tools/release/appimage-gtk-plugin.sh
grep -qE '^sum=[0-9a-f]{64}$' tools/release/appimage-gtk-plugin.sh
grep -qE '^rev=[0-9a-f]{40}$' tools/release/appimage-gtk-plugin.sh
node -e "const s = require('fs').readFileSync('.github/workflows/build.yml', 'utf8'); const seed = s.indexOf('run: tools/release/appimage-gtk-plugin.sh'), build = s.indexOf('--bundles appimage'), check = s.indexOf('run: tools/release/appimage-check.sh'); if (!(seed > 0 && seed < build && build < check)) throw new Error('seed, bundle and check are not in that order');"
```

## Depends On
None
