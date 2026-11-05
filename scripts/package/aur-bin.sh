#!/usr/bin/env bash
# AUR binary package: packages the static release archives.

prepare() {
    require_cmd git ssh sha256sum
    setup_ssh

    AUR_DIR="$WORKDIR/aur-bin"
    log "cloning AUR whoisthat-bin"
    git clone --quiet ssh://aur@aur.archlinux.org/whoisthat-bin.git "$AUR_DIR"

    PKGREL=$(compute_pkgrel "$AUR_DIR")
    log "pkgrel: $PKGREL"

    local amd64="$ARTIFACT_DIR/whoisthat-linux-amd64.tar.gz"
    local arm64="$ARTIFACT_DIR/whoisthat-linux-arm64.tar.gz"
    [ -f "$amd64" ] || die "missing $amd64"
    [ -f "$arm64" ] || die "missing $arm64"

    SHA_AMD64=$(sha256 "$amd64")
    SHA_ARM64=$(sha256 "$arm64")
}

build() {
    cp "$ROOT/pkg/aur-bin/PKGBUILD" "$AUR_DIR/PKGBUILD"
    cp "$ROOT/pkg/aur-bin/whoisthat-bin.install" "$AUR_DIR/whoisthat-bin.install"

    sed -i "s/^pkgver=.*/pkgver=${VERSION}/" "$AUR_DIR/PKGBUILD"
    sed -i "s/^pkgrel=.*/pkgrel=${PKGREL}/" "$AUR_DIR/PKGBUILD"
    sed -i "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('${SHA_AMD64}')/" "$AUR_DIR/PKGBUILD"
    sed -i "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('${SHA_ARM64}')/" "$AUR_DIR/PKGBUILD"

    write_srcinfo "$AUR_DIR" > "$AUR_DIR/.SRCINFO"

    if [ "$DRY_RUN" = 1 ]; then
        log "dry run: would push v${VERSION}-${PKGREL} to AUR"
        sed -n '1,40p' "$AUR_DIR/.SRCINFO"
    fi
}

publish() {
    git -C "$AUR_DIR" config user.email "ci@github-actions"
    git -C "$AUR_DIR" config user.name "GitHub Actions"
    git -C "$AUR_DIR" add -A

    if git -C "$AUR_DIR" diff --cached --quiet; then
        log "AUR is already up to date"
        return 0
    fi

    git -C "$AUR_DIR" commit -m "v${VERSION}-${PKGREL}: update package"
    git -C "$AUR_DIR" push
    log "pushed v${VERSION}-${PKGREL} to AUR"
}

verify() {
    bash -n "$AUR_DIR/PKGBUILD"
    grep -q "pkgver = ${VERSION}" "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no pkgver ${VERSION}"
    grep -q "pkgrel = ${PKGREL}" "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no pkgrel ${PKGREL}"
    grep -Eq "sha256sums_x86_64 = [0-9a-f]{64}" "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no pinned sha256sums_x86_64"
    grep -Eq "sha256sums_aarch64 = [0-9a-f]{64}" "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no pinned sha256sums_aarch64"
    grep -q "source_x86_64 = " "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no source_x86_64"
    grep -q "source_aarch64 = " "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no source_aarch64"
}
