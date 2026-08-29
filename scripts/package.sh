#!/bin/bash
# Compile the CLI for one target and produce the release tarball for it.
#
# Used by both the release workflow and CI, so packaging breaks on a pull
# request rather than at tag time, when the tag has already been pushed.
#
#   scripts/package.sh <target> <output-dir>
#
# The asset name is `ketch-<target>.tar.gz`, and the binary inside it is named
# `ketch` at the archive root. That is what install.sh greps SHA256SUMS for and
# what `ketch self update`'s `findBinary` looks for after unpacking. Renaming
# either breaks upgrades for everyone already installed.
#
# Bun compiles the binary. Perry is the intended compiler and the reason the
# code is written to erasable-syntax, `node:`-builtins-only rules, but as of
# 0.5.1220 it cannot link any program that calls `fetch` on macOS: its prebuilt
# `libperry_stdlib.a` references `js_ext_http_client_*` symbols the published
# package does not define. A five-line `await fetch(...)` reproduces it, and
# downloading is the whole job here. Switching back is this one command.

set -euo pipefail

TARGET="${1:?usage: package.sh <target> <output-dir>}"
OUT_DIR="${2:?usage: package.sh <target> <output-dir>}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Bun cross-compiles, so any target builds anywhere. The release workflow still
# runs one runner per architecture, because a slice that has never been executed
# is a slice nobody has tested.
case "$TARGET" in
  aarch64-apple-darwin) BUN_TARGET="bun-darwin-arm64" ;;
  x86_64-apple-darwin)  BUN_TARGET="bun-darwin-x64" ;;
  *)
    echo "unknown target: $TARGET" >&2
    echo "expected aarch64-apple-darwin or x86_64-apple-darwin" >&2
    exit 1
    ;;
esac

mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

echo "==> compiling $TARGET"
bun build --compile --target="$BUN_TARGET" apps/cli/src/main.ts --outfile "$STAGE/ketch"

if [ ! -x "$STAGE/ketch" ]; then
  echo "bun reported success but left no executable at $STAGE/ketch" >&2
  exit 1
fi

cp README.md LICENSE "$STAGE/"
chmod 755 "$STAGE/ketch"

TARBALL="$OUT_DIR/ketch-$TARGET.tar.gz"
tar -czf "$TARBALL" -C "$STAGE" ketch README.md LICENSE

echo "==> $TARBALL"
shasum -a 256 "$TARBALL"
