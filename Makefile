SHELL := /bin/bash
BINDIR ?= /usr/bin
FUZZTIME ?= 30s

.PHONY: all build core rust test test-go test-race test-rust test-scripts test-e2e test-xray test-fuzz test-ppa lint lint-shell version-check docs-check fmt integration docs install clean coverage package-aur package-aur-bin package-ppa

all: build

# Build both binaries into their default locations (not installed).
build: core rust

core:
	cd core/core && go build -o whoisthat-core

rust:
	cargo build --release --locked

test: test-go test-rust test-scripts

test-go:
	cd core/core && go test ./...

# Needs cgo/gcc, so it is kept out of `test` for minimal build images.
test-race:
	cd core/core && go test -race ./...

test-rust:
	cargo test

# Every scripts/*.test.sh plus the init-detection check. ipc-e2e needs built
# binaries, so it runs through `make test-e2e` instead.
test-scripts:
	./scripts/run-tests.sh

package-%:
	./scripts/package.sh $*

# IPC smoke test: a real core in a temp XDG sandbox, read back through the CLI.
# Needs both binaries built (`make core` and `cargo build`).
test-e2e:
	./scripts/ipc-e2e.test.sh

# Validate finalmask fragment/noise combinations with the pinned Xray binary.
# Skips when no binary is installed; set WHOISTHAT_XRAY_BIN to force one.
test-xray:
	@cd core/core && \
	XRAY_BIN="$${WHOISTHAT_XRAY_BIN:-}"; \
	if [ -z "$$XRAY_BIN" ]; then XRAY_BIN="$$(ls -d "$$HOME"/.local/share/whoisthat/runtimes/xray/v*/xray 2>/dev/null | sort -V | tail -1)"; fi; \
	if [ -z "$$XRAY_BIN" ]; then XRAY_BIN="$$(command -v xray || true)"; fi; \
	WHOISTHAT_XRAY_BIN="$$XRAY_BIN" go test ./lib/xrayjson -run TestFinalmaskConfigsWithXray -count=1 -v

# Coverage report with the same line floors as CI.
# Requires: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov
coverage:
	cargo llvm-cov --locked --lcov --output-path lcov.info --fail-under-lines 36
	cd core/core && go test -coverprofile=cover.out ./... && go tool cover -func=cover.out | awk '/^total:/{gsub(/%/,"",$$3); if ($$3+0 < 55) { print "coverage " $$3 "% below 55%"; exit 1 } else { print "go coverage: " $$3 "%" }}'

# Fuzz the untrusted-input parsers; each target needs its own invocation.
test-fuzz:
	cd core/core && go test -run '^$$' -fuzz=FuzzParseUri -fuzztime=$(FUZZTIME) ./lib/parser
	cd core/core && go test -run '^$$' -fuzz=FuzzParseLink -fuzztime=$(FUZZTIME) ./lib/happ
	cd core/core && go test -run '^$$' -fuzz=FuzzParse -fuzztime=$(FUZZTIME) ./lib/xrayjson

# Privileged TUN/firewall scenarios in a throwaway network namespace.
# Requires root (or CAP_NET_ADMIN) and is intentionally not part of `make test`.
# The script re-executes itself through sudo.
integration:
	./scripts/netns-test.sh

# Build a PPA source package and check the .deb contents (needs dpkg-dev).
test-ppa:
	./scripts/ppa-smoke.sh

lint: version-check docs-check
	cargo fmt --all --check
	cargo clippy --all-targets --no-deps --locked -- -D warnings
	cd core/core && go vet ./...
	gofmt -l core/core | (! grep .)
	./scripts/check-swallowed-errors.sh

# shellcheck over every tracked shell script. Needs shellcheck installed.
lint-shell:
	./scripts/lint-shell.sh

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
	sudo install -Dm755 target/release/whoisthat-runner $(BINDIR)/whoisthat-runner
	sudo install -Dm755 core/core/whoisthat-core $(BINDIR)/whoisthat-core
	sudo install -Dm755 scripts/authorize-core /usr/lib/whoisthat/authorize-core
	sudo /usr/lib/whoisthat/authorize-core --bin $(BINDIR)/whoisthat-core --add "$$(id -un)"
	@if [ -e /usr/local/bin/whoisthat ] || [ -e /usr/local/bin/whoisthat-core ]; then \
		echo "Removing legacy /usr/local/bin/whoisthat binaries"; \
		sudo rm -f /usr/local/bin/whoisthat /usr/local/bin/whoisthat-core; \
	fi

clean:
	cargo clean
	rm -f core/core/whoisthat-core
