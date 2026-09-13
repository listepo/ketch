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
    cargo test --all-targets --locked

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

lint-shell:
    bash -n install.sh
    bash -n scripts/package.sh
    bash -n scripts/release.sh
    sh tests/release-yml-notarize.sh

package:
    #!/usr/bin/env bash
    set -euo pipefail
    target=$(rustc -vV | sed -n 's/^host: //p')
    scripts/package.sh "$target" dist
    cd dist
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 *.tar.gz > SHA256SUMS
        shasum -a 256 -c SHA256SUMS
        hash() { shasum -a 256 "$1" | awk '{print $1}'; }
    else
        sha256sum *.tar.gz > SHA256SUMS
        sha256sum -c SHA256SUMS
        hash() { sha256sum "$1" | awk '{print $1}'; }
    fi
    tarball="ketch-${target}.tar.gz"
    expected=$(grep "$tarball" SHA256SUMS | awk '{print $1}')
    actual=$(hash "$tarball")
    [ "$expected" = "$actual" ] || { echo "checksum line unusable" >&2; exit 1; }
    mkdir -p unpacked && tar -xzf "$tarball" -C unpacked
    binary=$(find unpacked \( -name ketch -o -name ketch.exe \) -type f | head -1)
    [ -n "$binary" ] || { echo "no ketch binary in the tarball" >&2; exit 1; }
    "$binary" --version

lint-cask:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "$(uname -s)" != "Darwin" ]; then
        echo "lint-cask: skipped (macOS only, like CI's package job)"
        exit 0
    fi
    mkdir -p cask/Casks
    scripts/cask.sh 0.0.0 "$(printf '%064d' 0)" "$(printf '%064d' 1)" \
        > cask/Casks/ketch.rb
    brew style cask/Casks/ketch.rb

check: fmt-check lint test lint-commits lint-shell package lint-cask

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
