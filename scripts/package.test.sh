#!/usr/bin/env bash
# Offline tests for scripts/package.sh. The real AUR, Launchpad and dpkg
# commands are replaced with shims, so this runs on any Linux CI runner.

set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT" || exit 1

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

VERSION=$(sed -n 's/^version = "\([^"]*\)"$/\1/p' Cargo.toml | head -n1)
PKGREL=$(sed -n 's/^pkgrel=//p' pkg/aur/PKGBUILD | head -n1)
export FAKE_VERSION="$VERSION"

# Keep the tests away from the caller's real ~/.ssh and provide one fake key.
mkdir -p "$TMP/home"
export HOME="$TMP/home"
export PACKAGING_SSH_KEY="fake-ssh-key"

BIN="$TMP/bin"
mkdir -p "$BIN"

fail=0
ok() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

# --- shims ------------------------------------------------------------------

cat > "$BIN/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "${1:-}" = clone ]; then
    shift
    dir=""
    for arg in "$@"; do
        case "$arg" in
            -*) ;;
            *) dir=$arg ;;
        esac
    done
    mkdir -p "$dir"
    case "$*" in
        *whoisthat-bin*) name=whoisthat-bin ;;
        *) name=whoisthat ;;
    esac
    if [ "$name" = whoisthat-bin ]; then
        cat > "$dir/PKGBUILD" <<PKG
pkgname=whoisthat-bin
pkgver=${FAKE_VERSION:?}
pkgrel=1
pkgdesc="prebuilt"
arch=('x86_64' 'aarch64')
url="https://example.com"
license=('MIT')
depends=('iproute2')
optdepends=('xray: optional')
provides=('whoisthat')
conflicts=('whoisthat')
options=(!strip)
install=whoisthat-bin.install
source_x86_64=("a")
source_aarch64=("b")
sha256sums_x86_64=('SKIP')
sha256sums_aarch64=('SKIP')
PKG
    else
        cat > "$dir/PKGBUILD" <<PKG
pkgname=whoisthat
pkgver=${FAKE_VERSION:?}
pkgrel=1
pkgdesc="source"
arch=('x86_64' 'aarch64')
url="https://example.com"
license=('MIT')
depends=('iproute2')
makedepends=('rust' 'go')
install=whoisthat.install
source=("whoisthat-\${pkgver}.tar.gz::https://example.com")
sha256sums=('SKIP')
PKG
    fi
    exit 0
fi

if [ "${1:-}" = -C ]; then
    shift 2
    case "${1:-}" in
        config|add|commit) exit 0 ;;
        diff) exit 1 ;;
        push)
            printf 'pushed\n' >> "${FAKE_GIT_PUSH_LOG:?}"
            exit 0
            ;;
    esac
fi

exit 0
EOF

cat > "$BIN/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o) out=$2; shift 2 ;;
        *) shift ;;
    esac
done
[ -n "$out" ] || exit 1
printf 'fake archive\n' > "$out"
EOF

cat > "$BIN/ssh" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

cat > "$BIN/ssh-keyscan" <<'EOF'
#!/usr/bin/env bash
printf 'ppa.launchpad.net ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ==\n'
EOF

cat > "$BIN/ssh-keygen" <<'EOF'
#!/usr/bin/env bash
printf '2048 SHA256:MGq+4hxD7RduVTcfwlwwboZnsgJC6SL/NltM8ye+gNg no comment (RSA)\n'
EOF

cat > "$BIN/dpkg-buildpackage" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

pkg=$(sed -n '1s/^\([^ ]*\) (.*/\1/p' debian/changelog)
ver=$(sed -n '1s/^[^(]*(\([^)]*\)).*/\1/p' debian/changelog)
series=$(sed -n '1s/^[^)]*) \([^;]*\);.*/\1/p' debian/changelog)
parent=$(cd .. && pwd)

printf 'Format: 3.0 (quilt)\n' > "$parent/${pkg}_${ver}.dsc"
{
    printf 'Format: 1.8\n'
    printf 'Date: now\n'
    printf 'Source: %s\n' "$pkg"
    printf 'Binary: %s\n' "$pkg"
    printf 'Architecture: any\n'
    printf 'Version: %s\n' "$ver"
    printf 'Distribution: %s\n' "$series"
    printf 'Maintainer: test <test@example.com>\n'
    printf 'Files:\n'
    printf ' abc 1 net optional %s_%s.tar.gz\n' "$pkg" "${ver%%-*}"
} > "$parent/${pkg}_${ver}_source.changes"
EOF

cat > "$BIN/dpkg-source" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF

cat > "$BIN/debsign" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
for last; do :; done
printf '%s\n' '-----BEGIN PGP SIGNED MESSAGE-----' >> "$last"
EOF

cat > "$BIN/dput" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "${FAKE_DPUT_LOG:?}"
EOF

chmod +x "$BIN"/*
export PATH="$BIN:$PATH"
export FAKE_GIT_PUSH_LOG="$TMP/git-push.log"
export FAKE_DPUT_LOG="$TMP/dput.log"

# --- artifacts --------------------------------------------------------------

DIST="$TMP/dist"
mkdir -p "$DIST" "$TMP/artifact"
for file in whoisthat whoisthat-core whoisthat.desktop whoisthat-url.desktop \
    whoisthat.svg LICENSE; do
    printf 'fixture\n' > "$TMP/artifact/$file"
done
tar -czf "$DIST/whoisthat-linux-amd64.tar.gz" -C "$TMP/artifact" .
tar -czf "$DIST/whoisthat-linux-arm64.tar.gz" -C "$TMP/artifact" .

# --- tests ------------------------------------------------------------------

echo "package.sh: unknown target"
./scripts/package.sh nope >/dev/null 2>&1
rc=$?
[ "$rc" = 2 ] && ok "exit 2" || bad "expected exit 2, got $rc"

echo "package.sh: aur"
AUR_WORK="$TMP/work-aur"
if ./scripts/package.sh aur --dry-run --workdir "$AUR_WORK" >/dev/null 2>&1; then
    ok "dry run"
else
    bad "dry run failed"
fi
grep -q "pkgver = $VERSION" "$AUR_WORK/aur/.SRCINFO" &&
    ok "pkgver" || bad "pkgver missing"
grep -q 'pkgrel = 2' "$AUR_WORK/aur/.SRCINFO" &&
    ok "pkgrel increments" || bad "pkgrel did not increment"
grep -Eq 'sha256sums = [0-9a-f]{64}' "$AUR_WORK/aur/.SRCINFO" &&
    ok "source checksum pinned" || bad "source checksum missing"

echo "package.sh: aur-bin"
BIN_WORK="$TMP/work-aur-bin"
if ./scripts/package.sh aur-bin --dry-run --workdir "$BIN_WORK" \
    --artifacts "$DIST" >/dev/null 2>&1; then
    ok "dry run"
else
    bad "dry run failed"
fi
grep -Eq 'sha256sums_x86_64 = [0-9a-f]{64}' "$BIN_WORK/aur-bin/.SRCINFO" &&
    ok "amd64 checksum pinned" || bad "amd64 checksum missing"
grep -Eq 'sha256sums_aarch64 = [0-9a-f]{64}' "$BIN_WORK/aur-bin/.SRCINFO" &&
    ok "arm64 checksum pinned" || bad "arm64 checksum missing"

echo "package.sh: ppa"
PPA_WORK="$TMP/work-ppa"
if ./scripts/package.sh ppa --dry-run --workdir "$PPA_WORK" \
    --artifacts "$DIST" --series noble,resolute >/dev/null 2>&1; then
    ok "dry run"
else
    bad "dry run failed"
fi
for series in noble resolute; do
    case "$series" in
        noble) debver="${VERSION}-${PKGREL}~ubuntu24.04.1" ;;
        resolute) debver="${VERSION}-${PKGREL}~ubuntu26.04.1" ;;
    esac
    changelog="$PPA_WORK/ppa/$series/whoisthat-$VERSION/debian/changelog"
    grep -q "$debver" "$changelog" &&
        ok "$series changelog" || bad "$series changelog"
    [ -f "$PPA_WORK/ppa/$series/whoisthat_${debver}_source.changes" ] &&
        ok "$series changes" || bad "$series changes missing"
done

orig="$PPA_WORK/ppa/noble/whoisthat_${VERSION}.orig.tar.gz"
[ -f "$orig" ] && ok "orig tarball" || bad "orig tarball missing"
tar -tzf "$orig" > "$TMP/orig.list"
sed -n '1p' "$TMP/orig.list" | grep -q "^whoisthat-${VERSION}/" &&
    ok "orig top-level directory" || bad "orig top-level directory"
grep -q '/debian/' "$TMP/orig.list" &&
    bad "orig contains debian/" || ok "orig excludes debian/"

echo "package.sh: publish"
PUB_WORK="$TMP/work-pub"
if ./scripts/package.sh ppa --publish --workdir "$PUB_WORK" \
    --artifacts "$DIST" --series noble >/dev/null 2>&1; then
    ok "ppa publish"
else
    bad "ppa publish failed"
fi
[ -s "$FAKE_DPUT_LOG" ] && ok "dput called" || bad "dput not called"
grep -q 'BEGIN PGP SIGNED MESSAGE' "$PUB_WORK/ppa/noble/"*.changes &&
    ok "changes signed" || bad "changes not signed"

AUR_PUB="$TMP/work-aur-pub"
if ./scripts/package.sh aur --publish --workdir "$AUR_PUB" >/dev/null 2>&1; then
    ok "aur publish"
else
    bad "aur publish failed"
fi
[ -s "$FAKE_GIT_PUSH_LOG" ] && ok "git push called" || bad "git push not called"

echo "debian packaging files"
grep -q '^Architecture: amd64 arm64' debian/control &&
    ok "architectures" || bad "architectures"
grep -q 'dist/$(DEB_HOST_ARCH)/whoisthat' debian/rules &&
    ok "rules select arch" || bad "rules do not select arch"
[ -x debian/postinst ] && ok "postinst executable" || bad "postinst missing"
grep -q 'postinstall: debian/postinst' pkg/nfpm.yaml &&
    ok "nfpm postinst path" || bad "nfpm postinst path"

echo
if [ "$fail" -ne 0 ]; then
    echo "package tests FAILED" >&2
    exit 1
fi
echo "package tests passed"
