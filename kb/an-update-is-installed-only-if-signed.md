---
id: home-ledger.an-update-is-installed-only-if-signed
project: home-ledger
category: security
severity: error
environment: any
depends_on: []
---

# An update is installed only if the project's update key signed it

## Claim
Every build carries the update public key and looks for updates only at this
repository's GitHub releases, over HTTPS. The updater checks each installer's
signature against that key before running it, and the check and install happen
in Rust, not in the webview. The base configuration does not ask for update
artifacts, so a build without the private key (a local build, a fork's pull
request) still builds. A tagged release fails rather than publishing without
a signed `latest.json`, and turning Windows signing on without its settings
fails the build rather than shipping unsigned.

## Why
An updater is a way to run new code on every installed machine. Anything that
could swap the download must also have to forge the signature. Losing the
private key only stops future updates. See docs/RELEASING.md.

## Check
```bash
node -e "const c=require('./src-tauri/tauri.conf.json'); const u=c.plugins.updater; if(!u.pubkey||u.pubkey.length<100) throw 'no pubkey'; if(u.endpoints.length!==1||!u.endpoints[0].startsWith('https://github.com/kairos-tech-oh/home-ledger/releases/')) throw 'unexpected endpoint'; if(c.bundle.createUpdaterArtifacts) throw 'base config demands the private key';"
grep -q 'tauri_plugin_updater::Builder::new().build()' src-tauri/src/lib.rs
grep -q 'node tools/release/latest-json.mjs' .github/workflows/build.yml
test "$(grep -c -- '--config ci.conf.json' .github/workflows/build.yml)" = 3
env -u TAURI_SIGNING_PRIVATE_KEY node tools/release/ci-config.mjs "${TMPDIR:-/tmp}/kb-ci.json" > /dev/null && node -e "if(require('fs').readFileSync(process.argv[1],'utf8').includes('createUpdaterArtifacts')) throw 'asked for artifacts without a key'" "${TMPDIR:-/tmp}/kb-ci.json"
! env -u AZURE_SIGNING_ENDPOINT WINDOWS_SIGNING=trusted-signing node tools/release/ci-config.mjs "${TMPDIR:-/tmp}/kb-ci2.json" 2>/dev/null
cargo clippy -p home-ledger -- -D warnings
```

## Depends On
None
