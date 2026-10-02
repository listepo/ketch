#!/bin/sh
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# The macOS app's update feed, end to end without the release secrets: the
# disk image and appcast scripts the release workflow runs, on a copy of the
# app built by `just macos-app`, signed with a throwaway EdDSA key.
#
# Two releases in a row must leave both in the feed, and a feed signed with a
# key that is not the app's must stop the release. macOS only; needs the
# built app and its resolved Sparkle package (`just macos-appcast` does both).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
built="$ROOT/desktop/macos/build/Build/Products/Debug/Ketch.app"

fail() {
    echo "desktop-appcast: $*" >&2
    exit 1
}

[ "$(uname -s)" = Darwin ] || { echo "desktop-appcast: skipped (macOS only)"; exit 0; }
[ -d "$built" ] || fail "no $built; run just macos-app first"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# A key pair in Sparkle's format: the base64 32-byte Ed25519 seed that
# `generate_keys -x` exports, and the base64 public key for SUPublicEDKey.
cat >"$tmp/keys.swift" <<'EOF'
import CryptoKit
let key = Curve25519.Signing.PrivateKey()
print(key.rawRepresentation.base64EncodedString())
print(key.publicKey.rawRepresentation.base64EncodedString())
EOF
xcrun swift "$tmp/keys.swift" >"$tmp/keys"
private_key="$(sed -n 1p "$tmp/keys")"
public_key="$(sed -n 2p "$tmp/keys")"
other_key="$(xcrun swift "$tmp/keys.swift" | sed -n 1p)"

# The app as a release would build it: this version, this key. Editing the
# plist breaks the ad-hoc signature, so the copy is signed again.
release_app() {
    rm -rf "$tmp/Ketch.app"
    ditto "$built" "$tmp/Ketch.app"
    /usr/libexec/PlistBuddy \
        -c "Set :SUPublicEDKey $public_key" \
        -c "Set :CFBundleVersion $1" \
        -c "Set :CFBundleShortVersionString $1" \
        "$tmp/Ketch.app/Contents/Info.plist"
    codesign --force --deep --sign - "$tmp/Ketch.app" 2>/dev/null
    "$ROOT/scripts/desktop-dmg.sh" "$tmp/Ketch.app" "$tmp/feed/Ketch-$1.dmg"
}

mkdir -p "$tmp/feed"
release_app 0.2.0
SPARKLE_ED_PRIVATE_KEY="$private_key" \
    "$ROOT/scripts/desktop-appcast.sh" "$tmp/feed" "$tmp/Ketch.app" 0.2.0 pyrlyn/ketch >/dev/null 2>&1 \
    || fail "the first release's appcast did not verify"
grep -q 'url="https://github.com/pyrlyn/ketch/releases/download/desktop-v0.2.0/Ketch-0.2.0.dmg"' "$tmp/feed/appcast.xml" \
    || fail "the enclosure is not the desktop-v0.2.0 asset"
grep -q 'releases/latest' "$tmp/feed/appcast.xml" && fail "the appcast points at /releases/latest"

# The next release starts from the published feed only; the old image is gone.
rm "$tmp/feed/Ketch-0.2.0.dmg"
release_app 0.3.0
SPARKLE_ED_PRIVATE_KEY="$private_key" \
    "$ROOT/scripts/desktop-appcast.sh" "$tmp/feed" "$tmp/Ketch.app" 0.3.0 pyrlyn/ketch >/dev/null 2>&1 \
    || fail "the second release's appcast did not verify"
for v in 0.2.0 0.3.0; do
    grep -q "<sparkle:version>$v</sparkle:version>" "$tmp/feed/appcast.xml" || fail "the feed lost $v"
done

# An enclosure whose signature is not over these bytes is refused: the 0.2.0
# item's signature moved onto the 0.3.0 image.
sig2="$(sed -n 's/.*Ketch-0.2.0.dmg".*sparkle:edSignature="\([^"]*\)".*/\1/p' "$tmp/feed/appcast.xml")"
sig3="$(sed -n 's/.*Ketch-0.3.0.dmg".*sparkle:edSignature="\([^"]*\)".*/\1/p' "$tmp/feed/appcast.xml")"
[ -n "$sig2" ] && [ -n "$sig3" ] || fail "the feed items carry no signatures"
sed "s|$sig3|$sig2|" "$tmp/feed/appcast.xml" >"$tmp/tampered.xml"
if xcrun swift "$ROOT/scripts/desktop-appcast-verify.swift" \
    "$tmp/Ketch.app" "$tmp/tampered.xml" "$tmp/feed/Ketch-0.3.0.dmg" 0.3.0 >/dev/null 2>&1; then
    fail "a signature over other bytes passed"
fi

# A private key that is not the app's pair is caught before anything ships.
rm "$tmp/feed/appcast.xml"
if SPARKLE_ED_PRIVATE_KEY="$other_key" \
    "$ROOT/scripts/desktop-appcast.sh" "$tmp/feed" "$tmp/Ketch.app" 0.3.0 pyrlyn/ketch >/dev/null 2>&1; then
    fail "a feed signed with the wrong key passed"
fi

echo "desktop-appcast: ok"
