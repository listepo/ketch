#!/usr/bin/env bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# Builds ketch-ffi into the C# library desktop/windows/KetchCore: the shared
# library, and the C# bindings uniffi-bindgen-cs generates from that library's
# metadata, both in KetchCore/Generated/. Both are build output (gitignored);
# the projects and tests beside them are not.
#
#   scripts/csharp.sh          the `ffi` profile: what the app ships
#   scripts/csharp.sh --debug  the dev profile: quicker, for tests and CI
#
# `just csharp` runs it; `just csharp-test` runs it and then `dotnet test`.
# The generator is the one mise.toml pins (D10: uniffi-bindgen-cs has no
# release for UniFFI 0.32 yet). Runs on Windows (Git Bash), macOS and Linux,
# so the binding can be tested on any of them.
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

case "$(uname -s)" in
    Darwin) lib=libketch_ffi.dylib ;;
    MINGW* | MSYS* | CYGWIN*) lib=ketch_ffi.dll ;;
    *) lib=libketch_ffi.so ;;
esac

# The mise pin by its install path first: a `cargo install`ed copy in
# ~/.cargo/bin, for an older UniFFI, comes earlier on PATH on many machines,
# and reads 0.32 metadata as garbage ("Invalid string data").
pin="cargo:https://github.com/dennisameling/uniffi-bindgen-cs"
gen=""
if command -v mise >/dev/null && installed=$(mise where "$pin" 2>/dev/null); then
    command -v cygpath >/dev/null && installed=$(cygpath -u "$installed")
    for candidate in "$installed/bin/uniffi-bindgen-cs" "$installed/bin/uniffi-bindgen-cs.exe"; do
        [ -x "$candidate" ] && gen=$candidate && break
    done
fi
[ -n "$gen" ] || gen=$(command -v uniffi-bindgen-cs || true)
[ -n "$gen" ] || {
    echo "uniffi-bindgen-cs is missing: run \`mise install\` (it is pinned in mise.toml)" >&2
    exit 1
}
version=$("$gen" --version)
case "$version" in
    *+v0.32.*) ;;
    *) echo "$gen is $version, not a generator for UniFFI 0.32: run \`mise install\`" >&2; exit 1 ;;
esac

cargo build --locked -p ketch-ffi --lib --profile "$profile"

out="$root/desktop/windows/KetchCore/Generated"
rm -rf "$out"
mkdir -p "$out"
# The bindings come from the metadata compiled into the library, so they can
# never describe a different build than the one copied beside them.
"$gen" --library "$root/target/$dir/$lib" \
    --config "$root/desktop/windows/uniffi.toml" --out-dir "$out" --no-format
cp "$root/target/$dir/$lib" "$out/"

echo "built $out/$lib ($profile) and $out/ketch_ffi.cs"
