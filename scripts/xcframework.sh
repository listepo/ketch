#!/usr/bin/env bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# Builds ketch-ffi into the local Swift package desktop/macos/KetchCore:
# an XCFramework holding one arm64 static library (macOS is Apple Silicon only), and
# the Swift bindings UniFFI generates from that library's metadata. Both are
# build output (gitignored); Package.swift and the tests beside them are not.
#
#   scripts/xcframework.sh          the `ffi` profile: what the app ships
#   scripts/xcframework.sh --debug  the dev profile: quicker, for tests and CI
#
# `just xcframework` runs it; `just ffi-test` runs it and then `swift test`.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$(pwd)

profile=ffi
dir=ffi
case "${1:-}" in
    "") ;;
    --debug) profile=dev dir=debug ;;
    *) echo "usage: $0 [--debug]" >&2; exit 2 ;;
esac

[ "$(uname -s)" = Darwin ] || { echo "an XCFramework is built on macOS" >&2; exit 1; }

# The app's deployment target (desktop/macos/project.yml). Set for the Rust
# build too, or the linker warns about every object built for a newer macOS
# than the app it is linked into.
export MACOSX_DEPLOYMENT_TARGET=26.0

package="$root/desktop/macos/KetchCore"
work="$root/target/xcframework"
target=aarch64-apple-darwin

# The Rust toolchain is mise.toml's pin; only its standard library for the
# target may be missing. Adding a target installs no program, and pinning it
# in mise.toml would download it on every Linux and Windows job too.
grep -qx "$target" <<<"$(rustup target list --installed)" || rustup target add "$target"

cargo build --locked -p ketch-ffi --lib --profile "$profile" --target "$target"
lib="$root/target/$target/$dir/libketch_ffi.a"

rm -rf "$work"
mkdir -p "$work/include"
cp "$lib" "$work/libketch_ffi.a"

# The bindings come from the metadata compiled into the library, so they can
# never describe a different build than the one in the XCFramework.
cargo run --locked -q -p ketch-ffi --features bindgen --bin uniffi-bindgen -- \
    generate --library "$lib" --language swift --out-dir "$work/swift"

# SwiftPM finds a binary target's module through a `module.modulemap` beside
# its header; UniFFI names it after the crate.
cp "$work/swift/ketch_ffiFFI.h" "$work/include/"
cp "$work/swift/ketch_ffiFFI.modulemap" "$work/include/module.modulemap"

rm -rf "$package/KetchFFI.xcframework"
xcodebuild -create-xcframework \
    -library "$work/libketch_ffi.a" -headers "$work/include" \
    -output "$package/KetchFFI.xcframework" >/dev/null

mkdir -p "$package/Sources/KetchCore"
cp "$work/swift/ketch_ffi.swift" "$package/Sources/KetchCore/ketch_ffi.swift"

echo "built $package/KetchFFI.xcframework ($profile) and Sources/KetchCore/ketch_ffi.swift"
