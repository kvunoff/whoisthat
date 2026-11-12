---
title: "Releasing"
description: "Bump versions, build release assets, and understand the CI and packaging workflows."
audience: "developer"
type: "how-to"
---

# Releasing

Releases are driven by `scripts/bump.sh` and the GitHub Actions workflows. Do not edit version mirrors by hand.

## Version bumps and releases

`scripts/bump.sh` turns the release procedure into one validated command. It
never edits version files directly: every write goes through
`scripts/check-versions.sh --set`, which updates all mirrors (Cargo, npm,
PKGBUILD/.SRCINFO, docs) and re-checks them before returning.

Fill the two working texts first (they are gitignored under `.local/bump/`):

```text
.local/bump/commit.md    first non-empty line = commit summary, rest = body
.local/bump/release.md   release notes (bullets or prose)
```

Then run:

| Situation | Command |
| :--- | :--- |
| Major: `0.15.1 → 1.0.0` | `./scripts/bump.sh --kind tui --bump-type 1` |
| Minor: `0.15.1 → 0.16.0` | `./scripts/bump.sh --kind tui --bump-type 2` |
| Patch: `0.15.1 → 0.15.2` | `./scripts/bump.sh --kind tui --bump-type 3` |
| Packaging-only: new patch bundle | `./scripts/bump.sh --kind packaging --bump-type 3` |
| Core fix: new `core-v*` + TUI patch | `./scripts/bump.sh --kind core --bump-type 3` |
| Core + TUI | `./scripts/bump.sh --kind core-bundle --bump-type 3` |
| Breaking IPC | `./scripts/bump.sh --kind protocol --bump-type 3 --protocol 4` |

Add `--dry-run` to print the version diff and the planned commits/tags/releases
without writing anything, and `--push` to push `main`/tags and create **draft**
GitHub releases. After a successful `--push`, the texts move to `*.md.backup`
and the working files are reset to an empty template.

Published release assets are immutable. Every kind creates new tags; the script
never moves or force-pushes an existing `v*` tag. `core` therefore bumps the TUI
patch too, and `packaging` ships as a new patch bundle.

Safety rules enforced by the script:

- the texts must be filled (the template marker is rejected);
- the tree must already pass `scripts/check-versions.sh`;
- new tags and releases must not exist;
- `kind packaging` refuses `src/**` or `core/core/**` changes unless
  `--allow-source` is passed;
- untracked files are refused unless `--allow-untracked` is passed;
- version files are restored if a bump step fails before committing.

### Release assets

`bump.sh --push` creates `core-v*` and `v*` as drafts. The `release.yml`
workflow then builds static musl binaries and publishes:

* `v*`: `whoisthat-linux-<arch>.tar.gz` (both binaries, both desktop entries,
  icon and license), `.deb`, `.rpm`, and `SHA256SUMS`;
* `core-v*`: `whoisthat-core-linux-<arch>.tar.gz` and `SHA256SUMS`.

Only after every asset is uploaded does CI publish `core-v*`, publish `v*` and
set `v*` as GitHub Latest. `install.sh` resolves `/releases/latest`, so users
never see a release without binaries. Every archive also carries a GitHub
build-provenance attestation (`gh attestation verify`).

Distribution channels are published next by the `publish-packages` matrix:
`aur` (source), `aur-bin` (prebuilt) and `ppa` (Ubuntu 24.04/26.04). All three
go through the same entrypoint:

```bash
./scripts/package.sh <aur|aur-bin|ppa> --tag vX.Y.Z --publish
```

Each target defines `prepare`, `build`, `publish` and `verify` in
`scripts/package/<target>.sh`. Without `--publish` the command only builds the
metadata in a temporary directory, so it is safe to run locally. The matrix
needs one `PACKAGING_SSH_KEY` for both AUR and Launchpad SFTP (`AUR_SSH_KEY` is
accepted as a legacy fallback) and `LAUNCHPAD_GPG_KEY` /
`LAUNCHPAD_GPG_PASSPHRASE` (plus optional `LAUNCHPAD_GPG_KEYID`) for the PPA.
Launchpad requires a GPG-signed upload even over SFTP, so the SSH key cannot
replace the GPG key. The first push of each AUR package creates the package
base. `LAUNCHPAD_USER` and `LAUNCHPAD_PPA` default to `kvunoff` and
`whoisthat`. A manual `workflow_dispatch` can pass `series` (for example
`resolute`) to upload only that PPA series; an empty value means
`noble,resolute`.

`release.yml` can also be dispatched manually with a tag. It checks out that
tag (never the branch) and refuses unless `check-versions.sh --tag <tag>`
confirms the version files match. Published assets are immutable: a dispatch
attaches only missing assets and never overwrites an existing one, then
rebuilds `SHA256SUMS` from the release contents so `install.sh` still verifies.
Archives are not byte-reproducible (tar/gzip and deb/rpm embed build
timestamps), so content is not hash-compared.

---

## CI/CD workflows

Continuous integration is handled via GitHub Actions in `.github/workflows/`:

* **`ci.yml`**: Runs on push and PRs targeting `main`:
  * Rust: `cargo fmt --all --check`, `cargo clippy --all-targets --no-deps -- -D warnings`, `cargo test` on a pinned rustc.
  * MSRV: `cargo check --all-targets --locked` on the `rust-version` declared in `Cargo.toml`.
  * Go: `go vet ./...`, `go build ./...`, `go test ./...`, `go mod tidy -diff`, `govulncheck`.
  * Shell: `shellcheck -S warning` over `install.sh`, `debian/postinst` and `scripts/*.sh`, then `./scripts/install.test.sh`, `./scripts/check-docs.test.sh`, `./scripts/release-upload.test.sh`, `./scripts/package.test.sh`.
  * Bump driver: `./scripts/bump.test.sh` (separate job).
  * Version consistency: `./scripts/check-versions.sh` (Cargo/package/PKGBUILD/aur-bin/.SRCINFO/docs/Xray pins).
  * Static build: `cargo build --target x86_64-unknown-linux-musl` plus a `CGO_ENABLED=0` core build, asserting neither binary has an `INTERP` segment.
  * Docs: `npm run docs:build` plus `./scripts/check-docs.sh` (IPC/config/CLI names, links, anchors, toolchain strings).
  * Integration: privileged netns suite (`-tags=integration`), failing if no `TestNetns*` ran.
  * IPC smoke: builds both binaries and reads a sandboxed core through `whoisthat core status --json` (`make test-e2e`).
  * Coverage: `cargo llvm-cov` (lcov) and `go test -coverprofile` artifacts; report only, no `--fail-under` gate.
* **`release.yml`**: Triggers on every `v*` tag push (or manually with `tag`):
  1. `verify` checks out the tag, runs `cargo test`, `go vet` and `go test`; `build` depends on it.
  2. Builds amd64/arm64 static musl binaries and verifies neither ELF has an `INTERP` segment (static or static-pie).
  3. Creates the bundle tarballs, `.deb`/`.rpm` via `nfpm`, and smoke-tests the packages in Debian and Fedora containers.
  4. Attests build provenance for `dist/*`.
  5. Uploads assets to the existing releases (drafts replaceable, published assets never overwritten), verifies the attestations, publishes `core-v*` first and `v*` last, marks `v*` Latest.
  6. Runs the `publish-packages` matrix (`aur`, `aur-bin`, `ppa`). Each row calls
     `./scripts/package.sh <target> --tag "$TAG" --publish`; the target computes
     `pkgrel`, regenerates `PKGBUILD`/`.SRCINFO` or the Debian source package,
     and pushes to AUR or uploads to Launchpad.
  7. Runs the real `install.sh` in Debian, Ubuntu, Fedora and Arch containers.
* **`core-sync.yml`**: Mirrors `core/core` into `kvunoff/whoisthat-core` on
  `core/core/**` changes and `core-v*` tags.
* **`deploy-docs.yml`**: Builds and deploys this VitePress documentation to GitHub Pages whenever changes are committed to `docs/**`.

## See also

- [Build & Run](/developers/build)
- [Testing](/developers/testing)
- [Documentation Guide](/developers/documentation)
