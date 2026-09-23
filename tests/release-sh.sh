#!/bin/sh
# scripts/release.sh decides the version, writes the changelog entry and makes
# the version commit — exercised in a throwaway repository whose origin is a
# local bare one, so nothing is pushed anywhere real and nothing is dispatched.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if command -v git-cliff >/dev/null 2>&1; then
    CLIFF=git-cliff
else
    CLIFF="$(cd "$ROOT" && mise which git-cliff 2>/dev/null)" \
        || { echo "release-sh: git-cliff is required (mise install)" >&2; exit 1; }
fi
export CLIFF
export CARGO="cargo --offline"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail() {
    echo "release-sh: $*" >&2
    exit 1
}

git init -q --bare "$tmp/origin.git"
git init -q -b main "$tmp/work"
cd "$tmp/work"
git config user.name test
git config user.email test@example.invalid
git config commit.gpgsign false
git config tag.gpgsign false

mkdir scripts src
cp "$ROOT/scripts/release.sh" scripts/
cp "$ROOT/cliff.toml" .
cat >Cargo.toml <<'EOF'
[package]
name = "fixture"
version = "1.2.3"
edition = "2021"
rust-version = "1.70"
EOF
echo 'fn main() {}' >src/main.rs
cat >CHANGELOG.md <<'EOF'
# Changelog

## [Unreleased]

## [1.2.3](https://github.com/listepo/ketch/releases/tag/v1.2.3) - 2020-01-01

### Added

- first
EOF
cargo generate-lockfile --offline --quiet
git add -A
git commit -qm "feat: first"
git tag v1.2.3
git remote add origin "$tmp/origin.git"
git push -q origin main --tags
echo 'fn second() {}' >src/second.rs
git add -A
git commit -qm "feat(cli): second (#7)"
git push -q origin main

out="$(scripts/release.sh minor --dry-run)"
echo "$out" | grep -q 'release v1.3.0' || fail "tagged 1.2.3 raised by minor is not 1.3.0: $out"

out="$(scripts/release.sh patch --no-bump)"
echo "$out" | grep -q 'already released' || fail "--no-bump went ahead on a tagged version: $out"
[ "$(git rev-list --count HEAD)" = 2 ] || fail "--no-bump made a commit"

scripts/release.sh patch --local >/dev/null 2>&1 || fail "--local failed"
[ "$(git log -1 --format=%s)" = "chore: release v1.2.4" ] || fail "wrong version commit subject"
grep -q '^version = "1.2.4"$' Cargo.toml || fail "Cargo.toml was not bumped"
grep -q '^rust-version = "1.70"$' Cargo.toml || fail "rust-version was rewritten"
grep -q 'name = "fixture"' Cargo.lock && grep -q '^version = "1.2.4"$' Cargo.lock \
    || fail "Cargo.lock does not carry 1.2.4"
grep -q '^- \*(cli)\* second (\[#7\](https://github.com/listepo/ketch/pull/7))$' CHANGELOG.md \
    || fail "the entry does not list the commit with its pull request link"

# The new entry goes between Unreleased and the previous release, which is
# left exactly as it was.
headings="$(grep '^## ' CHANGELOG.md | sed 's/ - .*//')"
expected='## [Unreleased]
## [1.2.4](https://github.com/listepo/ketch/releases/tag/v1.2.4)
## [1.2.3](https://github.com/listepo/ketch/releases/tag/v1.2.3)'
[ "$headings" = "$expected" ] || fail "headings out of order:
$headings"

# A version that is not tagged yet is released as it stands.
git push -q origin main
out="$(scripts/release.sh major --dry-run)"
echo "$out" | grep -q 'release v1.2.4' || fail "untagged 1.2.4 was raised: $out"

echo "release-sh: ok"
