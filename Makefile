SHELL := /bin/bash
BINDIR ?= /usr/bin

.PHONY: all build core rust test test-go test-race test-rust test-shell test-bump test-docs test-release test-package test-e2e test-fuzz lint version-check docs-check fmt integration docs install clean coverage package-aur package-aur-bin package-ppa

all: build

# Build both binaries into their default locations (not installed).
build: core rust

core:
	cd core/core && go build -o whoisthat-core

rust:
	cargo build --release --locked

test: test-go test-rust test-shell test-bump test-docs test-release test-package

test-go:
	cd core/core && go test ./...

# Needs cgo/gcc, so it is kept out of `test` for minimal build images.
test-race:
	cd core/core && go test -race ./...

test-rust:
	cargo test

# Root helpers: fake init systems and fake core, no root required.
test-shell:
	./scripts/cgroup-setup.test.sh
	./scripts/run-core.test.sh
	./scripts/install-init.test.sh

# Release-driver tests: throwaway clones, local bare origin, fake gh.
test-bump:
	./scripts/bump.test.sh

# Docs checker: the anchor check must report broken links, not abort.
test-docs:
	./scripts/check-docs.test.sh

# Release upload policy: per-asset backfill and immutability of published releases.
test-release:
	./scripts/release-upload.test.sh

# Packaging targets: aur (source), aur-bin (prebuilt), ppa.
test-package:
	./scripts/package.test.sh

package-%:
	./scripts/package.sh $*

# IPC smoke test: a real core in a temp XDG sandbox, read back through the CLI.
# Needs both binaries built (`make core` and `cargo build`).
test-e2e:
	./scripts/ipc-e2e.test.sh

# Coverage report with line thresholds: lcov for Rust, coverprofile for Go.
# Requires: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov
coverage:
	cargo llvm-cov --lcov --output-path lcov.info --fail-under-lines 36
	cd core/core && go test -coverprofile=cover.out ./... && go tool cover -func=cover.out | tail -1

# Fuzz the untrusted-input parsers; each target needs its own invocation.
test-fuzz:
	cd core/core && go test -run '^$$' -fuzz=FuzzParseUri -fuzztime=30s ./lib/parser
	cd core/core && go test -run '^$$' -fuzz=FuzzParseLink -fuzztime=30s ./lib/happ
	cd core/core && go test -run '^$$' -fuzz=FuzzParse -fuzztime=30s ./lib/xrayjson

# Privileged TUN/firewall scenarios in a throwaway network namespace.
# Requires root (or CAP_NET_ADMIN) and is intentionally not part of `make test`.
# The grep guard fails if no TestNetns* was discovered (renamed/removed tests).
integration:
	cd core/core && set -o pipefail && go test -tags=integration ./lib/proxy/tun/ -run '^TestNetns' -v | tee /tmp/whoisthat-netns.log && grep -Eq -- '--- (PASS|SKIP): TestNetns' /tmp/whoisthat-netns.log

lint: version-check docs-check
	cargo fmt --all --check
	cargo clippy --all-targets --no-deps -- -D warnings
	cd core/core && go vet ./...
	gofmt -l core/core | (! grep .)
	@if grep -rn --include='*.go' --exclude='*_test.go' \
	  -e '_ = proxy_manager.ReloadRouting' \
	  -e '_ = cmd.DB.SetActiveRoutingProfile' \
	  -e '_ = database.SetActiveRoutingProfile' \
	  -e '_ = cmd.DB.SaveRoutingProfile' \
	  -e '_ = database.SaveRoutingProfile' \
	  -e '_ = db.updateGroup' \
	  -e '_ = db.saveDBConfig' \
	  -e '_ = os.Chmod(' \
	  -e '_ = os.WriteFile(' \
	  core/core; then \
		echo 'error: swallowed state error (plan 2026-11-07, item 1)' >&2; \
		exit 1; \
	fi

# Docs must not drift from the code (IPC, config keys, CLI, links, versions).
docs-check:
	./scripts/check-docs.sh

# Every version mirror must agree with its source of truth (Cargo.toml,
# version.go, protocol.txt, PKGBUILD). Also gates release tags.
version-check:
	./scripts/check-versions.sh

fmt:
	cargo fmt --all
	cd core/core && gofmt -w .

docs:
	npm run docs:build

install:
	@if [ "$$(id -u)" = "0" ]; then \
		echo "Run 'make install' without sudo: the build needs your rustup toolchain."; \
		echo "It will ask for sudo only for the install steps."; \
		exit 1; \
	fi
	$(MAKE) build
	sudo install -Dm755 target/release/whoisthat $(BINDIR)/whoisthat
	sudo install -Dm755 core/core/whoisthat-core $(BINDIR)/whoisthat-core
	sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep $(BINDIR)/whoisthat-core
	@if [ -e /usr/local/bin/whoisthat ] || [ -e /usr/local/bin/whoisthat-core ]; then \
		echo "Removing legacy /usr/local/bin/whoisthat binaries"; \
		sudo rm -f /usr/local/bin/whoisthat /usr/local/bin/whoisthat-core; \
	fi

clean:
	cargo clean
	rm -f core/core/whoisthat-core
