#!/usr/bin/env bash
# AUR source package: builds whoisthat from the released tag archive.

prepare() {
    require_cmd git ssh curl
    setup_ssh

    AUR_DIR="$WORKDIR/aur"
    log "cloning AUR whoisthat"
    git clone --quiet ssh://aur@aur.archlinux.org/whoisthat.git "$AUR_DIR"

    PKGREL=$(compute_pkgrel "$AUR_DIR")
    log "pkgrel: $PKGREL"

    SOURCE_URL="https://github.com/kvunoff/whoisthat/archive/refs/tags/v${VERSION}.tar.gz"
    fetch "$SOURCE_URL" "$WORKDIR/source.tar.gz"
    SOURCE_SHA=$(sha256 "$WORKDIR/source.tar.gz")
    log "source sha256: $SOURCE_SHA"
}

build() {
    cp "$ROOT/pkg/aur/PKGBUILD" "$AUR_DIR/PKGBUILD"
    cp "$ROOT/pkg/aur/whoisthat.install" "$AUR_DIR/whoisthat.install"

    sed -i "s/^pkgver=.*/pkgver=${VERSION}/" "$AUR_DIR/PKGBUILD"
    sed -i "s/^pkgrel=.*/pkgrel=${PKGREL}/" "$AUR_DIR/PKGBUILD"
    sed -i "s/^sha256sums=.*/sha256sums=('${SOURCE_SHA}')/" "$AUR_DIR/PKGBUILD"

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
    grep -Eq "sha256sums = [0-9a-f]{64}" "$AUR_DIR/.SRCINFO" ||
        die ".SRCINFO has no pinned sha256sums"
}
