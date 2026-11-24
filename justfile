# tsuiyomi task runner

# List recipes
default:
    @just --list

# Run the TUI
run *args:
    cargo run -- {{args}}

# Type-check
check:
    cargo check

# Format
fmt:
    cargo fmt

# Lint
clippy:
    cargo clippy --all-targets -- -D warnings

# Test
test:
    cargo test

# Build an optimized binary
build:
    cargo build --release

# Download and package the extension-host runtime (JRE + miwayomi jar)
host-runtime:
    ./scripts/build-host-runtime.sh

# Package the host runtime on Windows from a published jar and a JDK 21
host-runtime-windows:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-host-runtime.ps1

# Build an optimized binary with the extension host embedded
dist: host-runtime
    cargo build --release

# Regenerate the Rust dependency license inventory
notices:
    python3 scripts/dependency-notices.py

# Package the local release binary: just package x86_64-unknown-linux-gnu 0.2.0
package target version:
    TSUIYOMI_TARGET="{{target}}" TSUIYOMI_VERSION="{{version}}" ./scripts/package-release.sh
