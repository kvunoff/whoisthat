#!/usr/bin/env bash
# Build a PPA source package offline and inspect the resulting .deb.
#
# Needs debhelper, devscripts and dpkg-dev. No network and no Launchpad.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/dist" "$tmp/files"
for file in whoisthat whoisthat-core whoisthat-runner whoisthat.desktop \
    whoisthat-url.desktop whoisthat.svg LICENSE \
    whoisthat.bash _whoisthat whoisthat.fish whoisthat.1; do
    printf 'fixture\n' > "$tmp/files/$file"
done
mkdir -p "$tmp/files/lib/whoisthat/init/openrc"
printf 'fixture\n' > "$tmp/files/lib/whoisthat/cgroup-setup"
printf 'fixture\n' > "$tmp/files/lib/whoisthat/authorize-core"
printf 'fixture\n' > "$tmp/files/lib/whoisthat/init/openrc/whoisthat-core"
tar -czf "$tmp/dist/whoisthat-linux-amd64.tar.gz" -C "$tmp/files" .
tar -czf "$tmp/dist/whoisthat-linux-arm64.tar.gz" -C "$tmp/files" .
./scripts/package.sh ppa --dry-run --workdir "$tmp/work" \
    --artifacts "$tmp/dist" --series noble

version=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
pkgrel=$(sed -n 's/^pkgrel=//p' pkg/aur/PKGBUILD | head -n1)
src="$tmp/work/ppa/noble/whoisthat-$version"
( cd "$src" && dpkg-buildpackage -b -d -us -uc )
deb="$tmp/work/ppa/noble/whoisthat_${version}-${pkgrel}~ubuntu24.04.1_amd64.deb"
dpkg-deb -c "$deb" > "$tmp/deb.list"
grep -q './usr/bin/whoisthat$' "$tmp/deb.list"
grep -q './usr/bin/whoisthat-core$' "$tmp/deb.list"
grep -q './usr/bin/whoisthat-runner$' "$tmp/deb.list"
grep -q './usr/lib/whoisthat/cgroup-setup$' "$tmp/deb.list"
grep -q './usr/lib/whoisthat/authorize-core$' "$tmp/deb.list"
grep -q './usr/share/zsh/site-functions/_whoisthat$' "$tmp/deb.list"
# dh_compress turns the man page into whoisthat.1.gz.
grep -qE './usr/share/man/man1/whoisthat\.1(\.gz)?$' "$tmp/deb.list"
