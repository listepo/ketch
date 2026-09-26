#!/usr/bin/env bash
# The one place a release version is decided, so `just release`, bump.yml and
# release-plz.yml cannot drift apart.
#
#   scripts/release.sh [patch|minor|major] [--dry-run|--local|--no-bump]
#
# The version in Cargo.toml is the version to release. It is raised only when
# that version is already tagged, so a version a merged release pull request
# wrote is released as it stands. dist creates the tag and the GitHub release
# itself, once every target has built (dispatch-releases in
# dist-workspace.toml), so this script's job ends at "the version commit is on
# origin/main and release.yml is running".
#
#   --dry-run  print the version that would be released; change nothing
#   --local    make the version commit, but neither push nor dispatch
#   --no-bump  release the version in Cargo.toml only if it is untagged; never
#              raise it (release-plz.yml, after a merged release pull request)
#
# The version is written in one place and read as the tag: `ketch self upgrade`
# compares the running binary's version against the release tag. Bumping it by
# hand in an ordinary commit is what this script exists to stop.

set -euo pipefail

cd "$(dirname "$0")/.."

die() { echo "error: $*" >&2; exit 1; }

level="${1:-patch}"
mode="${2:-}"
case "$level" in
  patch | minor | major) ;;
  *) echo "level must be patch, minor or major (got '$level')" >&2; exit 2 ;;
esac
case "$mode" in
  "" | --dry-run | --local | --no-bump) ;;
  *) echo "unknown option: $mode" >&2; exit 2 ;;
esac

# The Rust toolchain is deliberately not pinned (see mise.toml); git-cliff is.
CARGO="${CARGO:-cargo}"
CLIFF="${CLIFF:-mise exec -- git-cliff}"

# Only the version inside [package]: `rust-version` and every dependency's
# inline `version =` are left alone.
package_version() {
  awk '/^\[package\]/ { in_pkg = 1; next }
       /^\[/          { in_pkg = 0 }
       in_pkg && /^version[[:space:]]*=/ { split($0, q, "\""); print q[2]; exit }' Cargo.toml
}

current="$(package_version)"
[ -n "$current" ] || die "could not read the version from Cargo.toml"

# Whether the current version shipped is the remote's answer, not this clone's.
git fetch --quiet --tags origin main

version="$current"
if git rev-parse -q --verify "refs/tags/v$current" >/dev/null; then
  # Only the numeric core is bumped; a prerelease suffix never survives one.
  IFS=. read -r major minor patch <<<"${current%%-*}"
  case "$level" in
    major) version="$((major + 1)).0.0" ;;
    minor) version="$major.$((minor + 1)).0" ;;
    patch) version="$major.$minor.$((patch + 1))" ;;
  esac
fi

echo "current $current -> release v$version"
if [ "$mode" = "--no-bump" ] && [ "$version" != "$current" ]; then
  echo "v$current is already released; the next version comes from a release pull request"
  exit 0
fi
# The workflow reads this to know which tag it dispatched. Not a `&&` one-liner:
# when the variable is unset the test fails, and under `set -e` a failing
# top-level list ends the script.
if [ -n "${GITHUB_OUTPUT:-}" ]; then
  echo "version=$version" >>"$GITHUB_OUTPUT"
fi

if [ "$mode" = "--dry-run" ]; then
  echo "dry run: nothing written"
  exit 0
fi

if [ "$version" != "$current" ]; then
  [ -z "$(git status --porcelain)" ] || die "working tree is not clean; commit or stash first"
  branch="$(git rev-parse --abbrev-ref HEAD)"
  [ "$branch" = main ] || die "run this on main; you are on $branch"
  [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] \
    || die "main is not level with origin/main; pull or push first"

  awk -v new="$version" '
    /^\[package\]/ { in_pkg = 1; print; next }
    /^\[/          { in_pkg = 0 }
    in_pkg && /^version[[:space:]]*=/ && !done { printf "version = \"%s\"\n", new; done = 1; next }
    { print }
  ' Cargo.toml >Cargo.toml.new && mv Cargo.toml.new Cargo.toml
  [ "$(package_version)" = "$version" ] || die "Cargo.toml did not take version $version"
  # The lock records the crate's own version too, and `--locked` builds fail
  # if the two disagree.
  $CARGO update --workspace --quiet

  # Written before the commit, so the version commit is never in its own
  # notes; inserted under `## [Unreleased]` so earlier entries stay as written.
  entry="$(mktemp)"
  trap 'rm -f "$entry" CHANGELOG.md.new' EXIT
  $CLIFF --unreleased --tag "v$version" --strip all >"$entry"
  grep -q '^### ' "$entry" || die "git-cliff found nothing to release since v$current"
  awk -v entry="$entry" '
    { print }
    !done && $0 == "## [Unreleased]" {
      print ""
      while ((getline line < entry) > 0) print line
      done = 1
    }
  ' CHANGELOG.md >CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md

  git add Cargo.toml Cargo.lock CHANGELOG.md
  git commit --quiet -m "chore: release v$version"

  if [ "$mode" = "--local" ]; then
    echo "local: version commit made, not pushed"
    exit 0
  fi
  git push --quiet origin HEAD:main
elif [ "$mode" = "--local" ]; then
  echo "local: v$version is not tagged yet; nothing to commit"
  exit 0
fi

# dist lifts this section as the release notes, and tests/release_changelog.rs
# requires it; a release pull request that lost it must not ship.
git show "origin/main:CHANGELOG.md" 2>/dev/null | grep -q "^## \[$version\](" \
  || die "CHANGELOG.md on origin/main has no entry for $version"

gh workflow run release.yml --ref main --field tag="v$version"
echo "release v$version dispatched"
