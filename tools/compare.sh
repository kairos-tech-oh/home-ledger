#!/usr/bin/env bash
# Check this port's figures against the prototype that is being replaced.
#
#   tools/compare.sh <prototype-checkout> <ledger.json>
#
# Both sides read the same document. Any disagreement is a porting bug, and
# the prototype wins unless there is a note in docs/PORT.md saying otherwise.
set -euo pipefail
# rustup's shim directory is not on PATH in a non-login shell.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
proto="${1:?usage: compare.sh <prototype-checkout> <ledger.json>}"
ledger="${2:?usage: compare.sh <prototype-checkout> <ledger.json>}"
cd "$(dirname "$0")/.."

echo "── prototype (core/Model.js) ──"
node tools/oracle.mjs "$proto/core/Model.js" "$ledger"
echo
echo "── this port (ledger-math) ──"
cargo run -q -p ledger-math --example figures -- "$ledger" | sed -n '/^figures/,/bucket cash/p' | tail -n +2
