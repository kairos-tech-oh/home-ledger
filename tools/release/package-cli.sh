#!/usr/bin/env bash
# Packages a built hl on its own, for a machine with no desktop: a .deb and a
# .tar.gz, named home-ledger-cli. The .deb puts hl in /usr/bin, as the app's
# own .deb does, so the two conflict and either replaces the other.
#
#   tools/release/package-cli.sh <path to hl> <version> <out dir>
#
# The architecture is the build machine's, as dpkg names it (amd64, arm64).
set -euo pipefail
umask 022

hl=$1
version=$2
out=$3
arch=$(dpkg --print-architecture)
here=$(cd "$(dirname "$0")" && pwd)
name="home-ledger-cli_${version}_${arch}"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"

# The tarball: hl, the unit and its settings, and the licence, to copy by hand.
mkdir -p "$work/$name"
install -m 755 "$hl" "$work/$name/hl"
install -m 644 "$here/hl-serve@.service" "$work/$name/"
install -m 644 "$here/hl-serve.default" "$work/$name/hl-serve"
install -m 644 "$here/../../LICENSE" "$work/$name/"
tar -C "$work" -czf "$out/$name.tar.gz" "$name"

# The .deb.
deb="$work/deb"
install -D -m 755 "$hl" "$deb/usr/bin/hl"
install -D -m 644 "$here/hl-serve@.service" "$deb/usr/lib/systemd/system/hl-serve@.service"
install -D -m 644 "$here/hl-serve.default" "$deb/etc/default/hl-serve"
install -D -m 644 "$here/../../LICENSE" "$deb/usr/share/doc/home-ledger-cli/copyright"
mkdir -p "$deb/DEBIAN"
echo /etc/default/hl-serve > "$deb/DEBIAN/conffiles"
cat > "$deb/DEBIAN/control" <<EOF
Package: home-ledger-cli
Version: $version
Architecture: $arch
Maintainer: Kairos Technologies <noreply@github.com>
Homepage: https://github.com/kairos-tech-oh/home-ledger
Section: utils
Priority: optional
Depends: libc6 (>= 2.35)
Conflicts: home-ledger
Replaces: home-ledger
Installed-Size: $(du -sk "$deb/usr" | cut -f1)
Description: Home Ledger from the command line, without the desktop app
 hl reads the ledger's figures and makes its edits from the terminal and
 from scripts, with the same rules and history as the app. hl serve answers
 hl commands over HTTP for a phone on the same tailnet.
EOF
dpkg-deb --root-owner-group --build "$deb" "$out/$name.deb"

ls -l "$out/$name".*
