---
id: home-ledger.the-app-wears-its-own-icon
project: home-ledger
category: correctness
severity: warn
environment: any
depends_on: []
---

# The app, its window and its installer carry the Home Ledger icon

## Claim
Every icon `tauri.conf.json` bundles exists at the size its name says, the
Windows `icon.ico` carries 16, 24, 32, 48, 64 and 256 pixel images, and
`icon.ico` and `icon.png` are the ones generated from the house-and-coins
artwork (`gpt_app_icon.png`, beside this repository) with `tauri icon`.

## Why
The icon is what the taskbar, the Start menu, the installer and Add/Remove
Programs show. An `.ico` missing a size makes Windows scale another one and it
blurs, and a regenerated set that quietly reverted to the old art would ship
unnoticed. The hashes pin the artwork; when the icon changes on purpose,
regenerate and update them here.

```bash
npx tauri icon ../gpt_app_icon.png -o <dir>            # 32, 128, 128@2x, ico, icns
npx tauri icon ../gpt_app_icon.png -o <dir> -p 256     # 256x256.png
npx tauri icon ../gpt_app_icon.png -o <dir> -p 1024    # 1024x1024.png, used as icon.png
```

## Check
```bash
node -e "const fs = require('fs'); const conf = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8')); const dim = (p) => { const b = fs.readFileSync(p); return b.readUInt32BE(16) + 'x' + b.readUInt32BE(20); }; const want = { '32x32.png': '32x32', '128x128.png': '128x128', '128x128@2x.png': '256x256', '256x256.png': '256x256', 'icon.png': '1024x1024' }; for (const [f, d] of Object.entries(want)) if (dim('src-tauri/icons/' + f) !== d) throw new Error(f + ' is ' + dim('src-tauri/icons/' + f)); for (const f of conf.bundle.icon) if (!fs.existsSync('src-tauri/' + f)) throw new Error('missing ' + f); const ico = fs.readFileSync('src-tauri/icons/icon.ico'); const sizes = []; for (let i = 0; i < ico.readUInt16LE(4); i++) sizes.push(ico[6 + 16 * i] || 256); if (sizes.sort((a, b) => a - b).join() !== '16,24,32,48,64,256') throw new Error('ico has ' + sizes);"
sha256sum src-tauri/icons/icon.ico | grep -q '^f778960b4803e23931e26d8ae518de9915fcd6f399ca8c7dc0b08f597e783914 '
sha256sum src-tauri/icons/icon.png | grep -q '^be08c0e575fabd2b9737a6f115ff9b7afc8a790f16c7418ba2419f475e433550 '
```

## Depends On
None
