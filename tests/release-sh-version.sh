#!/bin/sh
# package_version() in scripts/release.sh must match Cargo.toml.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

package_version() {
    cargo pkgid --offline --quiet 2>/dev/null | sed 's/.*#//'
}

expected="$(awk '/^\[package\]/ { in_pkg = 1; next }
                /^\[/          { in_pkg = 0 }
                in_pkg && /^version[[:space:]]*=/ {
                  split($0, q, "\""); print q[2]; exit
                }' Cargo.toml)"
actual="$(package_version)"

if [ -z "$actual" ]; then
    echo "release-sh-version: cargo pkgid returned no version" >&2
    exit 1
fi

if [ "$actual" != "$expected" ]; then
    echo "release-sh-version: cargo pkgid says $actual, Cargo.toml says $expected" >&2
    exit 1
fi

# The sed must tolerate registry-style ids, not only path+file URLs.
case "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0" in
    *) stripped=$(printf '%s' "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.0" | sed 's/.*#//') ;;
esac
if [ "$stripped" != "serde@1.0.0" ]; then
    echo "release-sh-version: sed extraction failed for registry ids" >&2
    exit 1
fi
