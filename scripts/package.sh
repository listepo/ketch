#!/bin/bash
# Build one target and produce the release tarball for it.
#
# Used by both the release workflow and CI, so packaging breaks on a pull
# request rather than at tag time, when the tag has already been pushed.
#
#   scripts/package.sh <rust-target> <output-dir>
#
# The asset name is `ketch-<target>.tar.gz`, which is what install.sh and
# `ketch self update` both look for. Renaming it breaks upgrades for everyone
# already installed.

set -euo pipefail

TARGET="${1:?usage: package.sh <rust-target> <output-dir>}"
OUT_DIR="${2:?usage: package.sh <rust-target> <output-dir>}"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> building $TARGET"
rustup target add "$TARGET" >/dev/null
cargo build --release --locked --target "$TARGET"

# Cargo can be configured to share a target directory outside the checkout.
# Ask Cargo for the resolved path instead of assuming the default `target/`.
TARGET_DIR="$(cargo metadata --no-deps --format-version=1 | sed -nE 's/.*"target_directory":"([^"]+)".*/\1/p')"
if [ -z "$TARGET_DIR" ]; then
  echo "could not determine Cargo's target directory" >&2
  exit 1
fi
BINARY="$TARGET_DIR/$TARGET/release/ketch"
if [ ! -x "$BINARY" ]; then
  echo "no binary at $BINARY" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp "$BINARY" "$STAGE/ketch"
cp README.md LICENSE "$STAGE/"
chmod 755 "$STAGE/ketch"

TARBALL="$OUT_DIR/ketch-$TARGET.tar.gz"
tar -czf "$TARBALL" -C "$STAGE" ketch README.md LICENSE

echo "==> $TARBALL"
shasum -a 256 "$TARBALL"
