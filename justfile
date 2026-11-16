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
    cargo clippy --all-targets

# Test
test:
    cargo test

# Build an optimized binary
build:
    cargo build --release
