#!/bin/sh
# The crate version must match the latest git tag, and the changelog must
# carry an entry for it — otherwise `ketch self upgrade` and install.sh
# look for a tag whose binaries never shipped.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

version="$(awk '/^\[package\]/ { in_pkg = 1; next }
                /^\[/          { in_pkg = 0 }
                in_pkg && /^version[[:space:]]*=/ {
                  split($0, q, "\""); print q[2]; exit
                }' Cargo.toml)"
[ -n "$version" ] || { echo "crate-version: no version in Cargo.toml" >&2; exit 1; }

tag="$(git tag --sort=-v:refname | head -n 1)"
[ -n "$tag" ] || { echo "crate-version: no git tags found" >&2; exit 1; }
[ "$tag" = "v$version" ] \
    || { echo "crate-version: Cargo.toml says $version but latest tag is $tag" >&2; exit 1; }

grep -q "^\#\# \[$version\](.*releases/tag/v$version)" CHANGELOG.md \
    || { echo "crate-version: CHANGELOG.md has no [$version] entry linked to releases/tag/v$version" >&2; exit 1; }
echo "crate-version: $version matches $tag with a changelog entry"