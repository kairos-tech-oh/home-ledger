#!/usr/bin/env bash
# Set up a Mac to build and run Home Ledger, then build it.
#
#   tools/macos-install.sh            # check the toolchain, then build the .app
#   tools/macos-install.sh --check    # only say what is missing, change nothing
#   tools/macos-install.sh --dev      # set up, then run `npm run dev`
#   tools/macos-install.sh --dmg      # also build a .dmg (see the note below)
#
# macOS is not a release target: the installers CI publishes are Windows and
# Linux, and nothing here is signed or notarised. This builds the app from
# source for development on a Mac. See docs/MACOS.md.
#
# What it refuses to do: install anything without asking. Homebrew is used
# when it is already present, and every install step is printed and confirmed
# first, because a build script that silently mutates a developer's toolchain
# is worse than one that stops.
set -euo pipefail

cd "$(dirname "$0")/.."

CHECK_ONLY=0
RUN_DEV=0
WANT_DMG=0
ASSUME_YES=0
for arg in "$@"; do
  case "$arg" in
    --check) CHECK_ONLY=1 ;;
    --dev) RUN_DEV=1 ;;
    --dmg) WANT_DMG=1 ;;
    -y | --yes) ASSUME_YES=1 ;;
    -h | --help)
      sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "unknown option: $arg" >&2
      exit 2
      ;;
  esac
done

# The versions the project needs, from the files that state them rather than
# repeated here: a bump in Cargo.toml must not leave this script behind.
RUST_MIN=$(sed -n 's/^rust-version = "\([0-9.]*\)".*/\1/p' Cargo.toml | head -1)
RUST_MIN=${RUST_MIN:-1.90}
NODE_MIN=22

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok() { printf '  \033[32mok\033[0m    %s\n' "$*"; }
bad() { printf '  \033[31mmiss\033[0m  %s\n' "$*"; }
note() { printf '        %s\n' "$*"; }

# Compares dotted versions without sort -V, which BSD sort lacks.
newer_or_same() {
  [ "$1" = "$2" ] && return 0
  local IFS=.
  local -a a=($1) b=($2)
  for i in 0 1 2; do
    local x=${a[i]:-0} y=${b[i]:-0}
    ((x > y)) && return 0
    ((x < y)) && return 1
  done
  return 0
}

confirm() {
  [ "$ASSUME_YES" -eq 1 ] && return 0
  local reply
  printf '  run: %s\n  [y/N] ' "$1"
  read -r reply </dev/tty || return 1
  [[ "$reply" =~ ^[Yy] ]]
}

# A binary that cannot load its libraries aborts rather than exiting non-zero,
# and the shell's own "Abort trap: 6" job notice cannot be silenced by the
# caller's redirection. Run it in a subshell that has closed stderr first, so
# a broken interpreter is reported by this script rather than by bash.
runs() { (
  exec 2>/dev/null
  "$@" >/dev/null 2>&1
); }

# The first line of why it will not run, for the report. Its stderr goes to a
# file from inside the silenced subshell, so the shell's job notice lands
# there too rather than on the terminal.
why_broken() {
  local tmp line
  tmp=$(mktemp -t hl-probe)
  ( exec 2>"$tmp"; "$@" >/dev/null ) || true
  line=$(head -1 "$tmp")
  rm -f "$tmp"
  printf '%s' "${line:-it exits with an error}"
}

# ------------------------------------------------------------------ the checks

say "Checking this Mac"

[ "$(uname -s)" = Darwin ] || {
  echo "This script is for macOS. On Linux and Windows see the README." >&2
  exit 1
}
ok "macOS $(sw_vers -productVersion) on $(uname -m)"

missing=()

# Xcode Command Line Tools: the linker and system headers. The system webview
# is WKWebView, so none of the README's libwebkit2gtk packages apply here.
if xcode-select -p >/dev/null 2>&1; then
  ok "Xcode Command Line Tools ($(xcode-select -p))"
else
  bad "Xcode Command Line Tools"
  note "xcode-select --install"
  missing+=("clt")
fi

if command -v cargo >/dev/null 2>&1 && runs rustc --version; then
  rust_have=$(rustc --version | awk '{print $2}')
  if newer_or_same "$rust_have" "$RUST_MIN"; then
    ok "Rust $rust_have (needs $RUST_MIN or newer)"
  else
    bad "Rust $rust_have is older than $RUST_MIN"
    note "rustup update stable"
    missing+=("rust-old")
  fi
else
  bad "Rust"
  note "install from https://rustup.rs, or: brew install rustup-init && rustup-init"
  missing+=("rust")
fi

if command -v node >/dev/null 2>&1 && runs node --version; then
  node_have=$(node --version | sed 's/^v//')
  if newer_or_same "$node_have" "$NODE_MIN.0.0"; then
    ok "Node $node_have (needs $NODE_MIN or newer)"
  else
    bad "Node $node_have is older than $NODE_MIN"
    missing+=("node-old")
  fi
else
  # A Node on PATH that cannot run at all is its own case, and a common one:
  # a Homebrew node keeps an absolute path to the icu4c it was built against,
  # so an icu4c upgrade leaves the binary on PATH and broken.
  if command -v node >/dev/null 2>&1; then
    bad "Node is on PATH but will not run"
    note "$(why_broken node --version)"
    note "a Homebrew node breaks this way when icu4c moves on; reinstall it:"
    note "brew upgrade node   (or: brew reinstall node)"
  else
    bad "Node"
    note "brew install node, or https://nodejs.org (LTS $NODE_MIN or newer)"
  fi
  missing+=("node")
fi

if [ ${#missing[@]} -gt 0 ]; then
  say "Install what is missing above, then run this again"
  # Offer the Homebrew route only when Homebrew is already here. Installing
  # Homebrew itself is the developer's decision, not this script's.
  if command -v brew >/dev/null 2>&1; then
    formulae=()
    for m in "${missing[@]}"; do
      case "$m" in
        rust) formulae+=("rustup-init") ;;
        node | node-old) formulae+=("node") ;;
      esac
    done
    if [ ${#formulae[@]} -gt 0 ] && [ "$CHECK_ONLY" -eq 0 ]; then
      note "Homebrew is installed, so this can be done for you."
      if confirm "brew install ${formulae[*]}"; then
        brew install "${formulae[@]}"
        # Let rustup add ~/.cargo/bin to the shell profile, or the new terminal
        # the message below asks for would still not find cargo.
        command -v rustup-init >/dev/null 2>&1 && ! command -v cargo >/dev/null 2>&1 &&
          rustup-init -y
        say "Toolchain installed. Open a new terminal so PATH picks it up, then run this again."
      fi
    fi
  fi
  exit 1
fi

[ "$CHECK_ONLY" -eq 1 ] && {
  say "Everything needed is here. Re-run without --check to build."
  exit 0
}

# ------------------------------------------------------------------- the build

say "Installing npm dependencies"
npm install
npm --prefix ui install

if [ "$RUN_DEV" -eq 1 ]; then
  say "Starting the app (npm run dev)"
  note "The first build compiles the whole dependency tree; expect a few minutes."
  exec npm run dev
fi

# macos.conf.json is what makes a bundle possible here at all: the targets in
# src-tauri/tauri.conf.json are the Windows and Linux installers, and none of
# them can be produced on a Mac.
say "Building Home Ledger.app"
note "The first build compiles the whole dependency tree; expect a few minutes."
if [ "$WANT_DMG" -eq 1 ]; then
  npm run build -- --config tools/release/macos.conf.json
else
  npm run build -- --config tools/release/macos.conf.json --bundles app
fi

app="target/release/bundle/macos/Home Ledger.app"
if [ -d "$app" ]; then
  say "Built $app"
  note "Run it:        open \"$app\""
  note "Install it:    cp -R \"$app\" /Applications/"
  note "The hl CLI is inside, at Contents/MacOS/hl"
else
  echo "The build reported success but $app is not there." >&2
  exit 1
fi
