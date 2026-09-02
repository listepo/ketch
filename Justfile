# Small, portable aliases for the repository's Rust and shell workflows.
# Cargo remains the source of truth; Just keeps the everyday checks memorable.

default: check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --locked -- -D warnings

test:
    cargo test --locked

test-install:
    cargo test --locked --test install

test-tui:
    cargo test --locked --features tui

check: fmt-check lint test

cache-info:
    cargo cache-info

cache-dry-run:
    cargo cache-dry-run

cache-autoclean:
    cargo cache-autoclean
