#!/bin/bash
# Open a pull request that proposes a new release.
#
#   scripts/release.sh 0.2.0
#   scripts/release.sh 0.2.0 --dry-run
#
# Bumps the version in Cargo.toml and Cargo.lock on a branch, pushes it, and
# opens a pull request. Merging that pull request is the release: the release
# workflow builds it, publishes the tarballs, and creates the tag afterwards.
#
# The version is written in one place and read as the tag: `ketch self update`
# compares the running binary's version against the release tag, so deriving
# one from the other is what keeps them level. Bumping by hand in an ordinary
# commit is what this command exists to stop.

set -euo pipefail
IFS=$'\n\t'

usage() {
  cat >&2 <<'USAGE'
usage: scripts/release.sh <version> [--dry-run]

  <version>   the new version, without a leading `v` (e.g. 0.2.0)
  --dry-run   say what would happen, change nothing

Run it on a clean, up-to-date main. It opens the pull request; merging it
publishes the release, which is also what creates the tag.
USAGE
  exit 2
}

VERSION=""
DRY_RUN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1; shift ;;
    -h|--help) usage ;;
    -*) echo "unknown option: $1" >&2; usage ;;
    *)
      [ -z "$VERSION" ] || { echo "unexpected argument: $1" >&2; usage; }
      VERSION="$1"; shift ;;
  esac
done
[ -n "$VERSION" ] || usage

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

die() { echo "error: $*" >&2; exit 1; }
step() { printf '==> %s\n' "$*"; }

# ---------------------------------------------------------------------------
# Checks that are cheaper to fail now than after a branch exists
# ---------------------------------------------------------------------------

case "$VERSION" in
  v*) die "give the version without the leading \`v\`: ${VERSION#v}" ;;
esac
printf '%s' "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$' \
  || die "\`$VERSION\` is not a version like 0.2.0 or 1.0.0-rc.1"

CURRENT="$(awk '/^\[package\]/ { in_pkg = 1; next }
                /^\[/          { in_pkg = 0 }
                in_pkg && /^version[[:space:]]*=/ {
                  split($0, q, "\""); print q[2]; exit
                }' Cargo.toml)"
[ -n "$CURRENT" ] || die "could not read the current version from Cargo.toml"

[ "$VERSION" != "$CURRENT" ] || die "Cargo.toml is already at $CURRENT"

# Only the numeric core is compared. Prerelease ordering is not modelled — this
# is here to catch a version that goes backwards, not to rank release
# candidates against each other.
awk -v new="$VERSION" -v old="$CURRENT" 'BEGIN {
  split(new, a, /[.-]/); split(old, b, /[.-]/)
  for (i = 1; i <= 3; i++) {
    if (a[i] + 0 > b[i] + 0) exit 0
    if (a[i] + 0 < b[i] + 0) exit 1
  }
  exit 0
}' || die "$VERSION is older than the current $CURRENT"

TAG="v$VERSION"
BRANCH="release/$TAG"

command -v gh >/dev/null || die "the GitHub CLI (gh) is required to open the pull request"
gh auth status >/dev/null 2>&1 || die "gh is not authenticated; run \`gh auth login\`"

git rev-parse --git-dir >/dev/null 2>&1 || die "not a git repository"
[ -z "$(git status --porcelain)" ] || die "working tree is not clean; commit or stash first"

DEFAULT_BRANCH="$(gh repo view --json defaultBranchRef -q .defaultBranchRef.name 2>/dev/null || echo main)"
CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
[ "$CURRENT_BRANCH" = "$DEFAULT_BRANCH" ] \
  || die "run this on $DEFAULT_BRANCH; you are on $CURRENT_BRANCH"

step "fetching origin"
git fetch --quiet origin "$DEFAULT_BRANCH" --tags

git rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null \
  && die "tag $TAG already exists"
[ "$(git rev-parse HEAD)" = "$(git rev-parse "origin/$DEFAULT_BRANCH")" ] \
  || die "$DEFAULT_BRANCH is not level with origin/$DEFAULT_BRANCH; pull or push first"
git rev-parse --verify --quiet "refs/heads/$BRANCH" >/dev/null \
  && die "branch $BRANCH already exists locally"

# ---------------------------------------------------------------------------
# What the pull request will say
# ---------------------------------------------------------------------------

LAST_TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"
if [ -n "$LAST_TAG" ]; then
  RANGE="$LAST_TAG..HEAD"
  SINCE="since $LAST_TAG"
else
  RANGE="HEAD"
  SINCE="since the beginning — this is the first release"
fi
CHANGES="$(git log "$RANGE" --no-merges --reverse --pretty=format:'- %s' || true)"
[ -n "$CHANGES" ] || CHANGES="- (no commits found $SINCE)"

if [ "$DRY_RUN" -eq 1 ]; then
  step "dry run: nothing will be changed"
  echo "  version   $CURRENT -> $VERSION"
  echo "  branch    $BRANCH"
  echo "  tag       $TAG (created by the release workflow, after it publishes)"
  echo "  changes   $SINCE"
  printf '%s\n' "$CHANGES" | sed 's/^/    /'
  exit 0
fi

# ---------------------------------------------------------------------------
# The bump
# ---------------------------------------------------------------------------

step "branching $BRANCH"
git checkout --quiet -b "$BRANCH"

# Rewrite only the version inside [package]. `rust-version` and every
# dependency's inline `version =` must be left alone.
step "setting version to $VERSION"
awk -v new="$VERSION" '
  /^\[package\]/ { in_pkg = 1; print; next }
  /^\[/          { in_pkg = 0 }
  in_pkg && /^version[[:space:]]*=/ && !done {
    printf "version = \"%s\"\n", new; done = 1; next
  }
  { print }
' Cargo.toml > Cargo.toml.new && mv Cargo.toml.new Cargo.toml

# The lock records the crate's own version too, and `--locked` builds fail if
# the two disagree — which is exactly what CI and the release workflow use.
cargo update --workspace --quiet 2>/dev/null \
  || cargo update --workspace --quiet --offline \
  || die "could not update Cargo.lock; run \`cargo update --workspace\` and retry"

# Prove the rewrite did what it claimed rather than trusting the awk above.
WROTE="$(cargo metadata --no-deps --format-version 1 --offline 2>/dev/null \
  | sed -n 's/.*"name":"ketch","version":"\([^"]*\)".*/\1/p')"
[ "$WROTE" = "$VERSION" ] \
  || die "Cargo.toml now reads \`$WROTE\`, not \`$VERSION\` — nothing was pushed"

# The commit message and the pull request body are assembled with `printf %s`,
# never interpolated into a heredoc or a double-quoted string. A commit subject
# holding a backtick — which this repository's own style invites — would
# otherwise be run as a command on its way into the message.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

{
  printf 'Release %s\n\n' "$TAG"
  printf 'Bumps the version the release workflow publishes and tags.\n\n'
  printf '%s\n' "$CHANGES"
} > "$TMP/message"

git add Cargo.toml Cargo.lock
git commit --quiet -F "$TMP/message"

step "pushing $BRANCH"
git push --quiet -u origin "$BRANCH"

step "opening the pull request"
{
  printf 'Bumps `Cargo.toml` and `Cargo.lock` from `%s` to `%s`.\n\n' "$CURRENT" "$VERSION"
  printf '## What is in it\n\nCommits %s:\n\n' "$SINCE"
  printf '%s\n\n' "$CHANGES"
  printf '## Publishing it\n\n'
  printf 'Merging this pull request is the release. The release workflow re-runs the\n'
  printf 'whole gate, builds and signs both macOS architectures, publishes the\n'
  printf 'tarballs with an aggregate `SHA256SUMS`, and creates `%s` last — so the\n' "$TAG"
  printf 'tag exists only if the release actually completed. Nothing else to type.\n\n'
  printf 'The tag is derived from `Cargo.toml`, which is why the version is bumped\n'
  printf 'here rather than by hand: `ketch self update` compares the running\n'
  printf "binary's version against the release tag, and a mismatch breaks upgrades\n"
  printf 'for everyone already installed.\n'
} > "$TMP/body"

gh pr create \
  --base "$DEFAULT_BRANCH" \
  --head "$BRANCH" \
  --title "Release $TAG" \
  --body-file "$TMP/body"

step "done — merging it publishes the release and creates $TAG"
