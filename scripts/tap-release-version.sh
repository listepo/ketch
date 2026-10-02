#!/bin/bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# Derive the cask version from a published release tag and optionally
# assert it matches the version the release workflow computed.
#
#   scripts/tap-release-version.sh <tag> [expected_version]
#
# Prints the version without a leading `v`. Exits 1 when expected_version
# is given and differs.

set -euo pipefail

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <tag> [expected_version]" >&2
  exit 2
fi

tag="$1"
expected="${2:-}"

version="${tag#v}"
if [ "$version" = "$tag" ]; then
  echo "release tag must start with v: $tag" >&2
  exit 1
fi

if [ -n "$expected" ] && [ "$version" != "$expected" ]; then
  echo "release version $version from $tag does not match workflow version $expected" >&2
  exit 1
fi

printf '%s\n' "$version"
