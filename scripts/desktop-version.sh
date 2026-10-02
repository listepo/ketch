#!/usr/bin/env bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-only
# Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

# The macOS app's release version, checked before anything is built:
#
#   scripts/desktop-version.sh <X.Y.Z>
#
# The app is versioned apart from the CLI and tagged `desktop-v<X.Y.Z>`. The
# version must be plain X.Y.Z, untagged, and above every earlier desktop-v
# tag: Sparkle offers an update only when CFBundleVersion grows, and the
# workflow sets CFBundleVersion to this version. Reads the tags this clone
# has, so fetch them first. Prints `tag=` and `previous=` lines, and appends
# them to $GITHUB_OUTPUT when it is set.

set -euo pipefail

die() { echo "desktop-version: $*" >&2; exit 1; }

[ $# -eq 1 ] || { echo "usage: $0 <X.Y.Z>" >&2; exit 2; }
version="$1"

# No prerelease suffix: Sparkle and semver order `1.0.0-beta` differently, and
# a beta would reach everyone through the one feed.
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] \
  || die "version must be X.Y.Z without a leading v or a suffix (got '$version')"

tag="desktop-v$version"
if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
  die "$tag already exists; a released version is never rebuilt"
fi

previous="$(git tag --list 'desktop-v[0-9]*' | sed 's/^desktop-v//' | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)"
if [ -n "$previous" ]; then
  highest="$(printf '%s\n%s\n' "$previous" "$version" | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)"
  [ "$highest" = "$version" ] || die "$version is not above the last app release $previous"
  previous="desktop-v$previous"
fi

echo "tag=$tag"
echo "previous=$previous"
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  {
    echo "tag=$tag"
    echo "previous=$previous"
  } >>"$GITHUB_OUTPUT"
fi
