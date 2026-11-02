SHELL := /bin/bash
BINDIR ?= /usr/bin

.PHONY: all build core rust test test-go test-race test-rust test-bump test-docs test-release lint version-check docs-check fmt integration docs install clean

all: build

# Build both binaries into their default locations (not installed).
build: core rust

core:
	cd core/core && go build -o whoisthat-core

rust:
	cargo build --release --locked

test: test-go test-rust test-bump test-docs test-release

test-go:
	cd core/core && go test ./...

# Needs cgo/gcc, so it is kept out of `test` for minimal build images.
test-race:
	cd core/core && go test -race ./...

test-rust:
	cargo test

# Release-driver tests: throwaway clones, local bare origin, fake gh.
test-bump:
	./scripts/bump.test.sh

# Docs checker: the anchor check must report broken links, not abort.
test-docs:
	./scripts/check-docs.test.sh

# Release upload policy: per-asset backfill and immutability of published releases.
test-release:
	./scripts/release-upload.test.sh

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
