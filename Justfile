# Small, portable aliases for the repository's Rust and shell workflows.
# Cargo remains the source of truth; Just keeps the everyday checks memorable.

# cargo-cache is pinned in mise.toml, so `just cache` runs the same version
# everywhere. Set CARGO_CACHE to a bare `cargo-cache` if mise is already
# activated in your shell, or to skip mise entirely.
cache := env("CARGO_CACHE", "mise exec -- cargo-cache")

default: check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --locked -- -D warnings

# the same gate under the name people type
alias clippy := lint

test:
    cargo test --locked

test-install:
    cargo test --locked --test install

test-tui:
    cargo test --locked --features tui

check: fmt-check lint test

# $CARGO_HOME sizes (no deletes) and the build output, wherever cargo puts it
cache:
    {{cache}}
    du -sh "$(cargo metadata --no-deps --format-version 1 | tr ',' '\n' | grep target_directory | cut -d'"' -f4)" 2>/dev/null || echo "target: (missing)"

# preview what cache-autoclean would remove; read this before running it
cache-dry-run:
    {{cache}} --autoclean --dry-run

# drop extracted crate/git checkouts; keep archives
cache-autoclean:
    {{cache}} --autoclean
