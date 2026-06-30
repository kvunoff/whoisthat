# Default recipe — list available targets
default:
    @just --list

# Build everything: parser, Go core, Rust TUI
build:
    cd parser && cargo build --release
    cd core/core && go build -o whoisthat-core
    cargo build

# Run the TUI (builds core first if missing)
run:
    @if [ ! -f ./core/core/whoisthat-core ]; then cd core/core && go build -o whoisthat-core; fi
    @if [ ! -f ./parser/target/release/whoisthat-parser ]; then cd parser && cargo build --release; fi
    cargo run

# Run all Rust tests
test-rust:
    cargo test
    cd parser && cargo test

# Run Go core tests
test-go:
    cd core/core && go test ./lib/crypto/... ./lib/AppConfig/... ./db/...

# Run all tests
test: test-rust test-go

# Lint: rustfmt check + clippy + go vet
lint:
    cargo fmt --all --check
    cargo clippy --all-targets --no-deps
    cd core/core && go vet ./...

# Apply rustfmt
fmt:
    cargo fmt --all

# Apply gofmt to Go core
fmt-go:
    cd core/core && gofmt -w .

# Format everything
format: fmt fmt-go

# Apply capabilities to core binary for TUN mode (one-time, needs sudo)
caps:
    sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep ./core/core/whoisthat-core

# Clean all build artifacts
clean:
    cargo clean
    cd parser && cargo clean
    rm -f core/core/whoisthat-core