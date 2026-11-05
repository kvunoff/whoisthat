#!/usr/bin/env bash
# Ubuntu PPA: packages the static release binaries into a signed source
# package and uploads it to Launchpad. Launchpad rebuilds the wrapper package
# per series; no compilation happens there.

prepare() {
    require_cmd dpkg-buildpackage dpkg-source tar sha256sum
    if [ "$PUBLISH" = 1 ]; then
        require_cmd debsign dput
    fi

    local amd64="$ARTIFACT_DIR/whoisthat-linux-amd64.tar.gz"
    local arm64="$ARTIFACT_DIR/whoisthat-linux-arm64.tar.gz"
    [ -f "$amd64" ] || die "missing $amd64"
    [ -f "$arm64" ] || die "missing $arm64"

    mkdir -p "$WORKDIR/dist/amd64" "$WORKDIR/dist/arm64"
    tar -xzf "$amd64" -C "$WORKDIR/dist/amd64"
    tar -xzf "$arm64" -C "$WORKDIR/dist/arm64"

    local arch file
    for arch in amd64 arm64; do
        for file in whoisthat whoisthat-core whoisthat.desktop \
            whoisthat-url.desktop whoisthat.svg LICENSE; do
            [ -f "$WORKDIR/dist/$arch/$file" ] ||
                die "$arch artifact is missing $file"
        done
    done

    setup_ssh
    setup_gpg
}

setup_gpg() {
    [ -n "${LAUNCHPAD_GPG_KEY:-}" ] || return 0

    export GNUPGHOME="$WORKDIR/gnupg"
    mkdir -p "$GNUPGHOME"
    chmod 700 "$GNUPGHOME"

    printf '%s\n' "$LAUNCHPAD_GPG_KEY" | gpg --batch --import >/dev/null 2>&1 ||
        die "cannot import LAUNCHPAD_GPG_KEY"

    KEYID=${LAUNCHPAD_GPG_KEYID:-}
    if [ -z "$KEYID" ]; then
        KEYID=$(gpg --batch --list-secret-keys --with-colons 2>/dev/null |
            awk -F: '/^fpr:/ {print $10; exit}')
    fi
    [ -n "$KEYID" ] || die "no GPG secret key found"

    if [ -n "${LAUNCHPAD_GPG_PASSPHRASE:-}" ]; then
        export LAUNCHPAD_GPG_PASSPHRASE
        cat > "$WORKDIR/gpg-sign" <<'EOF'
#!/bin/sh
exec gpg --batch --pinentry-mode loopback \
    --passphrase "$LAUNCHPAD_GPG_PASSPHRASE" "$@"
EOF
        chmod +x "$WORKDIR/gpg-sign"
        GPG_SIGN_CMD="$WORKDIR/gpg-sign"
    fi
}

series_version() {
    case "$1" in
        jammy) printf '22.04' ;;
        noble) printf '24.04' ;;
        resolute) printf '26.04' ;;
        *) printf '%s' "$1" ;;
    esac
}

write_changelog() {
    local file=$1 debver=$2 series=$3
    cat > "$file" <<EOF
whoisthat (${debver}) ${series}; urgency=medium

  * Release ${VERSION}.

 -- kvunoff <kvunoff@proton.me>  $(date -R)
EOF
}

sign_changes() {
    local changes=$1
    if [ -n "$GPG_SIGN_CMD" ]; then
        debsign -p"$GPG_SIGN_CMD" -k"$KEYID" "$changes"
    elif [ -n "$KEYID" ]; then
        debsign -k"$KEYID" "$changes"
    else
        debsign "$changes"
    fi
}

build() {
    local series src debver changes
    local -a list
    IFS=',' read -r -a list <<< "$SERIES"
    CHANGES_FILES=()

    for series in "${list[@]}"; do
        series=$(printf '%s' "$series" | tr -d '[:space:]')
        [ -n "$series" ] || continue

        debver="${VERSION}-${PKGREL}~ubuntu$(series_version "$series").1"
        src="$WORKDIR/ppa/$series/whoisthat-$VERSION"
        rm -rf "$WORKDIR/ppa/$series"
        mkdir -p "$src"

        cp -a "$WORKDIR/dist" "$src/dist"

        # The orig tarball must contain a single whoisthat-<version>/ top-level
        # directory and no debian/; create it before adding the packaging.
        tar -czf "$WORKDIR/ppa/$series/whoisthat_${VERSION}.orig.tar.gz" \
            -C "$WORKDIR/ppa/$series" "whoisthat-$VERSION"

        cp -a "$ROOT/debian" "$src/debian"
        write_changelog "$src/debian/changelog" "$debver" "$series"

        (
            cd "$src" || exit 1
            dpkg-buildpackage -S -us -uc -d
        )

        # dpkg-buildpackage -S names a source-only upload *_source.changes.
        changes="$WORKDIR/ppa/$series/whoisthat_${debver}_source.changes"
        [ -f "$changes" ] || die "expected $changes"

        if [ "$PUBLISH" = 1 ]; then
            sign_changes "$changes"
        fi

        CHANGES_FILES+=("$changes")
        log "built $debver"
    done

    [ "${#CHANGES_FILES[@]}" -gt 0 ] || die "no PPA series selected"
}

publish() {
    local changes
    for changes in "${CHANGES_FILES[@]}"; do
        # dput-ng reads the sftp profile from the private HOME set up by setup_ssh.
        dput "${DPUT_TARGET:-whoisthat-ppa}" "$changes"
    done
}

verify() {
    local changes dsc
    for changes in "${CHANGES_FILES[@]}"; do
        dsc="${changes%_source.changes}.dsc"
        [ -f "$changes" ] || die "missing $changes"
        [ -f "$dsc" ] || die "missing $dsc"
        grep -q '^Files:' "$changes" || die "$changes has no Files field"
        grep -q '^Distribution:' "$changes" ||
            die "$changes has no Distribution field"
        if [ "$PUBLISH" = 1 ]; then
            grep -q 'BEGIN PGP SIGNED MESSAGE' "$changes" ||
                die "$changes is not signed"
        fi
    done
}
