#!/bin/sh
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# The macOS app's releases share this repository with the CLI's, and the
# CLI's installers follow /releases/latest. This holds both sides apart:
#
# - release-apple-desktop.yml is dispatch-only and a thin caller of the
#   org-level pyrlyn/infra release-apple-desktop.yml, pinned by commit SHA,
#   with exactly the org secrets it needs, by name, and the desktop-v tag
#   prefix; that workflow checks the version, the secrets and the placeholder
#   Sparkle key, and creates every release with make_latest=false;
# - the CLI's release tooling never takes a desktop-v tag for its own.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
wf="$ROOT/.github/workflows/release-apple-desktop.yml"

fail() {
    echo "desktop-release: $*" >&2
    exit 1
}
need() {
    grep -q -- "$2" "$1" || fail "$(basename "$1") is missing $3"
}

command -v ruby >/dev/null 2>&1 || fail "ruby is required to parse the workflows"
[ -f "$wf" ] || fail "missing $wf"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# --- the workflow ------------------------------------------------------------

# One trigger, workflow_dispatch with a version; `on` parses as true in YAML 1.1.
triggers="$(ruby -ryaml -e 'w = YAML.load_file(ARGV[0]); puts (w["on"] || w[true]).keys.sort.join(",")' "$wf")"
[ "$triggers" = workflow_dispatch ] || fail "release-apple-desktop.yml runs on '$triggers', not only workflow_dispatch"
need "$wf" 'version:' 'the version input'

# No local release steps: one job, the infra reusable workflow, pinned by SHA,
# with the org secrets and the inputs that keep app releases apart from the CLI.
ruby -ryaml -e '
  w = YAML.load_file(ARGV[0])
  jobs = w["jobs"]
  abort "expected one job, got #{jobs.keys.join(",")}" unless jobs.keys == ["release"]
  job = jobs["release"]
  abort "local steps in the release job" if job.key?("steps")
  uses = job["uses"].to_s
  abort "not the infra workflow: #{uses}" unless uses =~ %r{\Apyrlyn/infra/\.github/workflows/release-apple-desktop\.yml@[0-9a-f]{40}\z}
  want = %w[MACOS_CERTIFICATE MACOS_CERTIFICATE_PWD APPSTORE_CONNECT_KEY APPSTORE_CONNECT_KEY_ID APPSTORE_CONNECT_ISSUER_ID SPARKLE_ED_PRIVATE_KEY]
  secrets = job["secrets"]
  abort "secrets must be passed by name, not #{secrets.inspect}" unless secrets.is_a?(Hash)
  abort "secrets are #{secrets.keys.sort.join(",")}, not #{want.sort.join(",")}" unless secrets.keys.sort == want.sort
  secrets.each { |k, v| abort "#{k} is not secrets.#{k}" unless v == "${{ secrets.#{k} }}" }
  with = job["with"] || {}
  abort "version is not the dispatch input" unless with["version"] == "${{ inputs.version }}"
  abort "tag-prefix is not desktop-v" unless with.fetch("tag-prefix", "desktop-v") == "desktop-v"
  abort "appcast-tag is not desktop-appcast" unless with.fetch("appcast-tag", "desktop-appcast") == "desktop-appcast"
  abort "release notes are not desktop/cliff.toml" unless with["cliff-config"] == "desktop/cliff.toml"
  %w[contents].each { |p| abort "the job does not grant #{p}: write" unless job.dig("permissions", p) == "write" }
' "$wf" || fail "release-apple-desktop.yml is not the thin infra caller"
for f in desktop/macos/ExportOptions.plist desktop/macos/Ketch/Info.plist desktop/cliff.toml; do
    [ -f "$ROOT/$f" ] || fail "release-apple-desktop.yml names $f, which does not exist"
done
need "$ROOT/desktop/cliff.toml" 'tag_pattern = "^desktop-v\[0-9\]' 'the anchored desktop-v tag pattern'

# --- the CLI's tooling ignores desktop-v tags --------------------------------

cd "$ROOT"
need cliff.toml 'tag_pattern = "^v\[0-9\]' 'an anchored tag_pattern (git-cliff matches anywhere in the name)'
need release-plz.toml 'git_tag_name = "v{{ version }}"' 'the v-only tag release-plz anchors'
need scripts/release.sh 'refs/tags/v$current' 'the exact v tag lookup'
need tests/crate-version.sh "git tag --list 'v\[0-9\]\*'" 'the v-only tag listing'
need .github/workflows/sync-docs.yml "startsWith(github.ref_name, 'v')" 'the guard against desktop-v releases'
if out="$(bash scripts/tap-release-version.sh desktop-v1.0.0 2>&1)"; then
    fail "tap-release-version.sh took desktop-v1.0.0 as a CLI version: $out"
fi
# The CLI's installers resolve /releases/latest, which make_latest=false keeps
# on the CLI.
need install.sh 'releases/latest' '/releases/latest'
need install.ps1 'releases/latest' '/releases/latest'
need crates/ketch-core/src/source/github.rs '"/releases/latest"' '/releases/latest'
if grep -q 'desktop' .github/workflows/release.yml .github/workflows/bump.yml; then
    fail "a CLI release workflow mentions the desktop app"
fi

echo "desktop-release: ok"
