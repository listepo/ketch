#!/usr/bin/env bash
# The macOS app's disk image: Ketch.app next to a link to /Applications, the
# usual drag-to-install window, compressed read-only.
#
#   scripts/desktop-dmg.sh <Ketch.app> <out.dmg>
#
# hdiutil ships with macOS, so the image needs no tool beyond the runner's own;
# a styled window (background, icon positions) is the one thing it cannot do,
# and nothing here needs one. Signing, notarising and stapling the image are
# the release workflow's, since they need its secrets; this script is shared
# with tests/desktop-appcast.sh so the check builds images the same way.

set -euo pipefail

[ $# -eq 2 ] || { echo "usage: $0 <Ketch.app> <out.dmg>" >&2; exit 2; }
app="$1"
out="$2"
[ -d "$app/Contents/MacOS" ] || { echo "desktop-dmg: $app is not an app bundle" >&2; exit 1; }

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
# ditto keeps the signature's extended attributes and symlinks intact; cp -R
# is not guaranteed to.
ditto "$app" "$stage/$(basename "$app")"
ln -s /Applications "$stage/Applications"

rm -f "$out"
hdiutil create -quiet -volname Ketch -srcfolder "$stage" -fs HFS+ -format UDZO "$out"
# Apple asks for a verified image before it is submitted for notarisation.
hdiutil verify -quiet "$out"
