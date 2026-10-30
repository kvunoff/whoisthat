SHELL := /bin/bash

.PHONY: all build core rust test test-go test-rust test-bump lint version-check fmt integration docs install clean

all: build

# Build both binaries into their default locations (not installed).
build: core rust

core:
	cd core/core && go build -o whoisthat-core

rust:
	cargo build --release --locked

test: test-go test-rust test-bump

test-go:
	cd core/core && go test ./...

test-rust:
	cargo test

# Release-driver tests: throwaway clones, local bare origin, fake gh.
test-bump:
	./scripts/bump.test.sh

# Privileged TUN/firewall scenarios in a throwaway network namespace.
# Requires root (or CAP_NET_ADMIN) and is intentionally not part of `make test`.
integration:
	cd core/core && go test -tags=integration ./lib/proxy/tun/ -run 'TestNetns' -v

lint: version-check
	cargo fmt --all --check
	cargo clippy --all-targets --no-deps
	cd core/core && go vet ./...
	gofmt -l core/core | (! grep .)

# Every version mirror must agree with its source of truth (Cargo.toml,
# version.go, protocol.txt, PKGBUILD). Also gates release tags.
version-check:
	./scripts/check-versions.sh

fmt:
	cargo fmt --all
	cd core/core && gofmt -w .

docs:
	npm run docs:build

install: build
	sudo install -Dm755 target/release/whoisthat /usr/local/bin/whoisthat
	sudo install -Dm755 core/core/whoisthat-core /usr/local/bin/whoisthat-core
	sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/local/bin/whoisthat-core

clean:
	cargo clean
	rm -f core/core/whoisthat-core
