# Small, portable aliases for the repository's Rust and shell workflows.
# Cargo remains the source of truth; Just keeps the everyday checks memorable.

# cargo-cache is pinned in mise.toml, so `just cache` runs the same version
# everywhere. Set CARGO_CACHE to a bare `cargo-cache` if mise is already
# activated in your shell, or to skip mise entirely.
cache := env("CARGO_CACHE", "mise exec -- cargo-cache")
# cargo-dist is pinned in mise.toml too; DIST overrides it the same way.
dist := env("DIST", "mise exec -- dist")

default: check

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# the same gate under the name people type
alias clippy := lint

test: && swarfr
    cargo nextest run --workspace --all-targets --locked

test-install:
    cargo nextest run --workspace --locked --all-targets -E 'binary(install)'

test-tui:
    cargo nextest run --workspace --locked --all-targets --features tui

# one-time setup: the pinned node from mise.toml, then commitlint onto it
deps:
    mise install
    mise exec -- npm ci

# local dev only (never committed, never on CI): use the sibling checkout at
# packages/crates/file-backup instead of crates.io.
#
# This is a `paths` override, not a `[patch]`: it swaps the source without
# touching Cargo.lock, so `--locked` builds keep working on the committed
# lock in both directions.
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p .cargo
    # Cargo reads both `.cargo/config` and `.cargo/config.toml` (the latter
    # wins), so a legacy `config` file must not be silently shadowed.
    if [ -e .cargo/config.toml ] || [ -e .cargo/config ]; then
      echo ".cargo/config.toml already exists; leaving it alone"
    else
      printf '%s\n' \
        '# Local-only Cargo overrides, never committed.' \
        '#' \
        '# Written by `just setup` so `file-backup` resolves to the sibling' \
        '# checkout at `packages/crates/file-backup` instead of crates.io.' \
        '# A `paths` override (not a `[patch]`): the source is swapped' \
        '# without touching Cargo.lock, so `--locked` keeps working.' \
        '# CI and release builds never run this, so they always resolve the' \
        '# pinned crates.io version from `Cargo.lock`.' \
        'paths = ["../../packages/crates/file-backup"]' \
        > .cargo/config.toml
      echo "wrote .cargo/config.toml (local file-backup override)"
    fi

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
    bash -n scripts/release.sh
    bash -n scripts/dist-generate.sh
    bash -n scripts/desktop-version.sh
    bash -n scripts/desktop-dmg.sh
    bash -n scripts/desktop-appcast.sh
    bash -n scripts/xcframework.sh
    bash -n fuzz/seed.sh
    sh tests/crate-version.sh
    sh tests/release-sh.sh
    sh tests/release-workflows.sh
    sh tests/ci-yml-triggers.sh
    sh tests/desktop-release.sh

package:
    #!/usr/bin/env bash
    set -euo pipefail
    target=$(rustc -vV | sed -n 's/^host: //p')
    {{dist}} build --artifacts=local --target "$target"
    cd "$(cargo metadata --no-deps --format-version 1 | tr ',' '\n' | grep target_directory | cut -d'"' -f4)/distrib"
    rm -rf unpacked
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 ketch-*.tar.gz > SHA256SUMS
        shasum -a 256 -c SHA256SUMS
        hash() { shasum -a 256 "$1" | awk '{print $1}'; }
    else
        sha256sum ketch-*.tar.gz > SHA256SUMS
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

# every generated man page through `mandoc -Tlint` at warning level; the
# style notes it also prints come from clap_mangen's layout, not ketch's text
lint-man:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v mandoc >/dev/null; then
        echo "lint-man: skipped (mandoc not found; it ships with macOS, apt install mandoc on Linux)"
        exit 0
    fi
    out=target/man-lint
    rm -rf "$out"
    cargo run --locked --quiet -- man --out "$out/man1"
    mandoc -Tlint -Wwarning "$out"/man1/*.1

# what `dist` would release for the version in Cargo.toml, per target
dist-plan:
    {{dist}} plan

# regenerate .github/workflows/release.yml from dist-workspace.toml; never edit it by hand
dist-generate:
    DIST="{{dist}}" scripts/dist-generate.sh

# release.yml is exactly what dist-generate writes; compared with the file as
# it stands, not the index, so an uncommitted regeneration counts
dist-check:
    #!/bin/sh
    set -eu
    before="$(mktemp)"
    trap 'rm -f "$before"' EXIT
    cp .github/workflows/release.yml "$before"
    DIST="{{dist}}" scripts/dist-generate.sh >/dev/null
    diff -u "$before" .github/workflows/release.yml \
        || { echo "release.yml is stale: commit what just dist-generate wrote" >&2; exit 1; }

# regenerate the macOS app's Tokens.swift, DESIGN.md front matter and
# preview.html tokens from desktop/macos/design/tokens.json
design-tokens:
    mise exec -- node desktop/macos/design/build.mjs

# the generated design files are what design-tokens writes (compared with the
# files as they stand, like dist-check), DESIGN.md lints clean, and text meets
# WCAG AA on its glass backgrounds
design-check:
    #!/bin/sh
    set -eu
    before="$(mktemp -d)"
    trap 'rm -rf "$before"' EXIT
    d=desktop/macos
    cp "$d/DESIGN.md" "$d/design/preview.html" "$d/design/generated/Tokens.swift" "$before/"
    mise exec -- node "$d/design/build.mjs"
    stale=0
    for f in DESIGN.md design/preview.html design/generated/Tokens.swift; do
        diff -u "$before/$(basename "$f")" "$d/$f" || stale=1
    done
    [ "$stale" = 0 ] || { echo "design tokens were stale: commit what just design-tokens wrote" >&2; exit 1; }
    mise exec -- node "$d/design/contrast.mjs"
    mise exec -- npx --no-install designmd lint "$d/DESIGN.md" > "$before/lint.json" \
        || { cat "$before/lint.json"; exit 1; }

# release Cargo.toml's version, or the next one if it is tagged (`just release minor --dry-run`)
release level="patch" *flags:
    scripts/release.sh {{level}} {{flags}}

check: fmt-check lint test lint-commits lint-shell lint-man dist-check design-check package lint-cask

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

# Lossless cleanup of this checkout's cargo target dir (compress + dedupe); never deletes.
swarfr:
    #!/usr/bin/env sh
    command -v swarfr >/dev/null || { echo "swarfr not found; install it with: ketch install swarfr"; exit 0; }
    [ -d target ] || exit 0
    swarfr run target || test $? -eq 2

# The macOS app (desktop/macos). XcodeGen writes Ketch.xcodeproj from
# project.yml; the project file and build/ are gitignored. Builds are unsigned:
# signing and notarisation belong to the app's release pipeline.
macos_dir := "desktop/macos"
macos_build := "xcodebuild -project " + macos_dir + "/Ketch.xcodeproj -scheme Ketch -derivedDataPath " + macos_dir + "/build"

macos-project:
    mise exec -- xcodegen generate --quiet --spec {{macos_dir}}/project.yml
    # Tagged like cargo's target/, so backup tools and worktree cleanup treat it as a cache.
    mkdir -p {{macos_dir}}/build
    printf 'Signature: 8a477f597d28d172789f06886806bc55\n# xcodebuild output for the macOS app; safe to delete.\n' > {{macos_dir}}/build/CACHEDIR.TAG

# universal (arm64 + x86_64) Debug build of Ketch.app
macos-app: macos-project
    {{macos_build}} -configuration Debug -destination 'generic/platform=macOS' ARCHS='arm64 x86_64' ONLY_ACTIVE_ARCH=NO -quiet build

# Swift Testing unit tests on the fake core, then the UI smoke test
macos-test: macos-project
    {{macos_build}} -destination 'platform=macOS' test

# the release's disk image and Sparkle appcast, round-tripped with a throwaway
# key: what desktop-release.yml runs, minus signing and notarisation
macos-appcast: macos-app
    sh tests/desktop-appcast.sh

# Built into desktop/macos/KetchCore with the `ffi` profile the app ships.
#
# ketch-ffi's XCFramework (arm64 + x86_64) and its Swift bindings
xcframework:
    scripts/xcframework.sh

# The dev profile, then a Swift test that drives the real core through the
# bindings against a scratch root: what CI's ketch-ffi job runs.
#
# debug XCFramework and bindings, then their Swift test
ffi-test:
    scripts/xcframework.sh --debug
    swift test --package-path {{macos_dir}}/KetchCore

# Needs valac, Meson, Ninja and json-glib; the ketch-capi CI job runs it.
# ketch-capi as the Linux app links it: the library, then the Vala test.
capi-test:
    cargo build --locked -p ketch-capi
    rm -rf target/capi-meson
    meson setup target/capi-meson crates/ketch-capi -Dcapi_dir="$PWD/target/debug"
    meson test -C target/capi-meson --print-errorlogs

# libFuzzer targets in fuzz/ (fuzz/README.md), on nightly and never part of `check`.
# `just fuzz` lists them, `just fuzz <target> [secs]` runs one, `just fuzz all [secs]` each in turn.
fuzz target="" secs="60":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "{{target}}" ]; then exec cargo +nightly fuzz list; fi
    fuzz/seed.sh
    targets="{{target}}"
    if [ "$targets" = all ]; then targets=$(cargo +nightly fuzz list); fi
    for t in $targets; do
        cargo +nightly fuzz run "$t" -- -max_total_time={{secs}} -max_len=16384 -rss_limit_mb=4096
    done
