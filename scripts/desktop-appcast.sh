#!/usr/bin/env bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-only
# Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

# Writes the macOS app's Sparkle appcast for one release, then checks it
# against the app it ships.
#
#   SPARKLE_ED_PRIVATE_KEY=... scripts/desktop-appcast.sh <dir> <Ketch.app> <X.Y.Z> <owner/repo>
#
# <dir> holds Ketch-<X.Y.Z>.dmg and, from the second release on, the current
# appcast.xml: generate_appcast keeps its earlier items and adds this one, and
# signs the feed as a whole because the app sets SURequireSignedFeed. The new
# item's download URL is this release's asset, never /releases/latest.
#
# generate_appcast is the one in the Sparkle package the app links
# (project.yml pins it exactly), found in xcodebuild's SourcePackages after
# the build; SPARKLE_BIN overrides the directory. The private key reaches it
# on standard input, never as an argument or a file.

set -euo pipefail

die() { echo "desktop-appcast: $*" >&2; exit 1; }

[ $# -eq 4 ] || { echo "usage: $0 <dir> <Ketch.app> <X.Y.Z> <owner/repo>" >&2; exit 2; }
dir="$1"
app="$2"
version="$3"
repo="$4"
[ -n "${SPARKLE_ED_PRIVATE_KEY:-}" ] || die "SPARKLE_ED_PRIVATE_KEY is not set"

root="$(cd "$(dirname "$0")/.." && pwd)"
bin="${SPARKLE_BIN:-$root/desktop/macos/build/SourcePackages/artifacts/sparkle/Sparkle/bin}"
[ -x "$bin/generate_appcast" ] || die "no generate_appcast in $bin; build the app first"

dmg="$dir/Ketch-$version.dmg"
[ -f "$dmg" ] || die "no $dmg"

tag="desktop-v$version"
# No deltas: they are built from earlier archives, which are not kept here.
printf '%s\n' "$SPARKLE_ED_PRIVATE_KEY" | "$bin/generate_appcast" \
  --ed-key-file - \
  --download-url-prefix "https://github.com/$repo/releases/download/$tag/" \
  --link "https://github.com/$repo/releases/tag/$tag" \
  --maximum-deltas 0 \
  "$dir"

xcrun swift "$root/scripts/desktop-appcast-verify.swift" "$app" "$dir/appcast.xml" "$dmg" "$version"
