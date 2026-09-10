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

# one-time setup: the pinned node from mise.toml, then commitlint onto it
deps:
    mise install
    mise exec -- npm ci

# opt in to the commit-msg hook; undo with `git config --unset core.hooksPath`
hooks:
    git config core.hooksPath .githooks
    @echo "commit-msg hook enabled (.githooks); undo with: git config --unset core.hooksPath"

# commitlint over fixtures whose good-*/bad-* names state the verdict
lint-commits:
    #!/bin/sh
    set -eu
    mismatches=""
    for fixture in tests/fixtures/commit-msg/*.txt; do
        name=$(basename "$fixture" .txt)
        case "$name" in
            good-*) want=pass ;;
            bad-*) want=fail ;;
            *) echo "lint-commits: $name says neither good nor bad"; exit 1 ;;
        esac
        if mise exec -- npx --no-install commitlint --edit "$fixture" >/dev/null 2>&1; then
            got=pass
        else
            got=fail
        fi
        if [ "$got" != "$want" ]; then
            mismatches="$mismatches\n  $name: want $want, got $got"
        fi
    done
    sh -n .githooks/commit-msg
    if [ -n "$mismatches" ]; then
        echo "lint-commits: fixtures disagreed with their names:$mismatches"
        exit 1
    fi

check: fmt-check lint test lint-commits

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
