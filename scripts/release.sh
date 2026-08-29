#!/bin/bash
# Open a pull request that proposes a new release.
#
#   scripts/release.sh 0.2.0
#   scripts/release.sh 0.2.0 --dry-run
#
# Bumps the version in apps/cli/package.json — and in the `VERSION` constant
# apps/cli/src/cli.ts reports, which a test pins to it — on a branch, pushes it,
# and opens a pull request. Merging that pull request is what makes the version
# official; tagging it is what publishes it, and the pull request body says how.
#
# The version is written in two files and is load-bearing in two more places:
# the release workflow refuses a tag that disagrees with apps/cli/package.json,
# because `ketch self update` compares the running binary's version against the
# release tag. A mismatch there breaks upgrades for everyone already installed.
# Bumping by hand is what this command exists to stop.

set -euo pipefail
IFS=$'\n\t'

usage() {
  cat >&2 <<'USAGE'
usage: scripts/release.sh <version> [--dry-run]

  <version>   the new version, without a leading `v` (e.g. 0.2.0)
  --dry-run   say what would happen, change nothing

Run it on a clean, up-to-date main. It opens the pull request; you merge it,
then tag the merge commit to publish.
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

MANIFEST="apps/cli/package.json"
CLI_SOURCE="apps/cli/src/cli.ts"

die() { echo "error: $*" >&2; exit 1; }
step() { printf '==> %s\n' "$*"; }

# The one place this script parses JSON. `node` is the canonical runtime, so it
# is already required to build anything here; hand-rolling a JSON reader out of
# sed to avoid depending on it would be the less reliable choice.
manifest_version() {
  node -e 'const fs = require("node:fs");
    const pkg = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    if (typeof pkg.version === "string") process.stdout.write(pkg.version);' "$MANIFEST"
}

# ---------------------------------------------------------------------------
# Checks that are cheaper to fail now than after a branch exists
# ---------------------------------------------------------------------------

case "$VERSION" in
  v*) die "give the version without the leading \`v\`: ${VERSION#v}" ;;
esac
printf '%s' "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$' \
  || die "\`$VERSION\` is not a version like 0.2.0 or 1.0.0-rc.1"

command -v node >/dev/null || die "node is required to read $MANIFEST"

CURRENT="$(manifest_version)"
[ -n "$CURRENT" ] || die "could not read the current version from $MANIFEST"

[ "$VERSION" != "$CURRENT" ] || die "$MANIFEST is already at $CURRENT"

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

# The constant the binary actually prints. `version.test.ts` asserts the two
# agree, so a bump that moved only one of them would fail the release gate.
grep -qxF "export const VERSION = \"$CURRENT\";" "$CLI_SOURCE" \
  || die "$CLI_SOURCE does not declare VERSION as \`$CURRENT\`; fix it by hand first"

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
  echo "  files     $MANIFEST, $CLI_SOURCE"
  echo "  branch    $BRANCH"
  echo "  tag       $TAG (after merge)"
  echo "  changes   $SINCE"
  printf '%s\n' "$CHANGES" | sed 's/^/    /'
  exit 0
fi

# ---------------------------------------------------------------------------
# The bump
# ---------------------------------------------------------------------------

step "branching $BRANCH"
git checkout --quiet -b "$BRANCH"

# Rewrite only the package's own `version` key. Every dependency's version is
# the *value* of its own name, never of a key called `version`, so matching the
# key is enough — and `!done` stops at the first one regardless. Only the value
# after the colon is replaced, so the line keeps its indentation and its
# trailing comma, whatever they are.
step "setting version to $VERSION"
awk -v new="$VERSION" '
  !done && /^[[:space:]]*"version"[[:space:]]*:/ {
    colon = index($0, ":")
    head = substr($0, 1, colon)
    tail = substr($0, colon + 1)
    sub(/"[^"]*"/, "\"" new "\"", tail)
    print head tail
    done = 1; next
  }
  { print }
' "$MANIFEST" > "$MANIFEST.new" && mv "$MANIFEST.new" "$MANIFEST"

# The constant the CLI prints, kept level with the manifest because
# `version.test.ts` compares them and the release workflow runs that test.
awk -v new="$VERSION" '
  !done && /^export const VERSION = "/ {
    printf "export const VERSION = \"%s\";\n", new; done = 1; next
  }
  { print }
' "$CLI_SOURCE" > "$CLI_SOURCE.new" && mv "$CLI_SOURCE.new" "$CLI_SOURCE"

# pnpm-lock.yaml records the workspace member's *path*, not its version, so a
# bump does not change it. Nothing to regenerate here, on purpose.

# Prove the rewrites did what they claimed rather than trusting the awk above.
WROTE="$(manifest_version)"
[ "$WROTE" = "$VERSION" ] \
  || die "$MANIFEST now reads \`$WROTE\`, not \`$VERSION\` — nothing was pushed"
grep -qxF "export const VERSION = \"$VERSION\";" "$CLI_SOURCE" \
  || die "$CLI_SOURCE was not rewritten to \`$VERSION\` — nothing was pushed"

# The commit message and the pull request body are assembled with `printf %s`,
# never interpolated into a heredoc or a double-quoted string. A commit subject
# holding a backtick — which this repository's own style invites — would
# otherwise be run as a command on its way into the message.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

{
  printf 'Release %s\n\n' "$TAG"
  printf 'Bumps the version the release workflow checks the tag against.\n\n'
  printf '%s\n' "$CHANGES"
} > "$TMP/message"

git add "$MANIFEST" "$CLI_SOURCE"
git commit --quiet -F "$TMP/message"

step "pushing $BRANCH"
git push --quiet -u origin "$BRANCH"

step "opening the pull request"
{
  printf 'Bumps `%s` and `%s` from `%s` to `%s`.\n\n' \
    "$MANIFEST" "$CLI_SOURCE" "$CURRENT" "$VERSION"
  printf '## What is in it\n\nCommits %s:\n\n' "$SINCE"
  printf '%s\n\n' "$CHANGES"
  printf '## Publishing it\n\n'
  printf 'Merging this pull request does not publish anything. Tag the merge commit:\n\n'
  printf '```bash\n'
  printf 'git checkout %s && git pull\n' "$DEFAULT_BRANCH"
  printf 'git tag %s && git push origin %s\n' "$TAG" "$TAG"
  printf '```\n\n'
  printf 'That runs the release workflow, which re-runs the whole gate, checks the\n'
  printf 'tag against `%s`, builds both macOS architectures and publishes the\n' "$MANIFEST"
  printf 'tarballs with an aggregate `SHA256SUMS`.\n\n'
  printf 'The tag must stay level with `%s`: `ketch self update` compares\n' "$MANIFEST"
  printf "the running binary's version against the release tag, so a mismatch\n"
  printf 'breaks upgrades for everyone already installed. The release workflow\n'
  printf 'refuses one, which is why this pull request exists.\n'
} > "$TMP/body"

gh pr create \
  --base "$DEFAULT_BRANCH" \
  --head "$BRANCH" \
  --title "Release $TAG" \
  --body-file "$TMP/body"

step "done — merge it, then tag $TAG to publish"
