#!/bin/sh
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# The macOS app's releases share this repository with the CLI's, and the
# CLI's installers follow /releases/latest. This holds both sides apart:
#
# - desktop-release.yml is dispatch-only, creates every release with
#   make_latest=false under a desktop-v tag, and refuses to run without its
#   secrets or with the placeholder Sparkle key;
# - scripts/desktop-version.sh accepts only a new, higher X.Y.Z;
# - the CLI's release tooling never takes a desktop-v tag for its own.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
wf="$ROOT/.github/workflows/desktop-release.yml"

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
[ "$triggers" = workflow_dispatch ] || fail "desktop-release.yml runs on '$triggers', not only workflow_dispatch"
need "$wf" 'version:' 'the version input'

# Every `gh release create`, continuation lines joined, says --latest=false,
# and nothing else is ever marked latest except the CLI release it restores.
ruby -ryaml -e '
  w = YAML.load_file(ARGV[0])
  runs = w["jobs"].values.flat_map { |j| j["steps"] }.map { |s| s["run"].to_s }
  lines = runs.join("\n").gsub(/\\\n\s*/, " ").lines
  creates = lines.grep(/gh release create/)
  abort "no gh release create in desktop-release.yml" if creates.empty?
  creates.each { |l| abort "gh release create without --latest=false: #{l.strip}" unless l.include?("--latest=false") }
  lines.grep(/--latest(?!=false)/).each do |l|
    abort "a release marked latest: #{l.strip}" unless l =~ /gh release edit "\$before"/
  end
  abort "make_latest set to something but false" if runs.join =~ /make_latest[=:]\s*"?(true|legacy)/
' "$wf" || fail "a release could become /releases/latest"

need "$wf" 'TAG: ${{ steps.version.outputs.tag }}' 'the tag from scripts/desktop-version.sh'
need "$wf" 'gh release create "$TAG"' 'the release created under that tag'
need "$ROOT/scripts/desktop-version.sh" 'tag="desktop-v$version"' 'the desktop-v tag prefix'
need "$ROOT/desktop/cliff.toml" 'tag_pattern = "^desktop-v\[0-9\]' 'the anchored desktop-v tag pattern'
need "$wf" 'git-cliff --config desktop/cliff.toml' 'the app release notes'
need "$wf" '/releases/latest moved from' 'the post-release latest check'

# The order that makes "no tag unless everything passed" true.
order="$(ruby -ryaml -e '
  steps = YAML.load_file(ARGV[0])["jobs"]["release"]["steps"].map { |s| s["name"].to_s }
  puts %w[Check\ the\ version Check\ the\ secrets Archive\ the\ app Notarise\ and\ staple Smoke\ test Generate\ and\ verify\ the\ appcast Publish\ the\ release]
    .map { |n| steps.index(n) || -1 }.join(" ")' "$wf")"
prev=-1
for i in $order; do
    [ "$i" -gt "$prev" ] || fail "desktop-release.yml steps are missing or out of order ($order)"
    prev="$i"
done
need "$wf" 'xcrun notarytool submit' 'notarytool'
need "$wf" 'xcrun stapler staple "$dmg"' 'stapling the image'
need "$wf" 'source=Notarized Developer ID' 'the spctl smoke test'
need "$wf" 'shasum -a 256' 'the checksum'
need "$wf" "if: env.XCFRAMEWORK == 'true'" 'the skipped XCFramework step'

# The secret check, run as the workflow runs it: with nothing set it names
# every secret and stops.
ruby -ryaml -e '
  puts YAML.load_file(ARGV[0])["jobs"]["release"]["steps"].find { |s| s["name"] == "Check the secrets" }["run"]
' "$wf" >"$tmp/check-secrets.sh"
secrets="MACOS_CERTIFICATE MACOS_CERTIFICATE_PWD APPSTORE_CONNECT_KEY APPSTORE_CONNECT_KEY_ID APPSTORE_CONNECT_ISSUER_ID SPARKLE_ED_PRIVATE_KEY"
if (cd "$ROOT" && env -i PATH="$PATH" bash "$tmp/check-secrets.sh" >"$tmp/out" 2>&1); then
    fail "the secret check passed with no secrets"
fi
for s in $secrets; do
    grep -q " $s" "$tmp/out" || fail "the secret check does not name $s"
    need "$wf" "$s: \${{ secrets.$s }}" "the $s mapping"
done
# With every secret set, the placeholder public key still stops it (macOS:
# the check reads the plist with PlistBuddy, as the runner does).
if [ "$(uname -s)" = Darwin ] \
    && /usr/libexec/PlistBuddy -c 'Print :SUPublicEDKey' "$ROOT/desktop/macos/Ketch/Info.plist" \
        | grep -qx SPARKLE_ED_PUBLIC_KEY_PLACEHOLDER; then
    set_all=""
    for s in $secrets; do set_all="$set_all $s=x"; done
    # shellcheck disable=SC2086
    if (cd "$ROOT" && env -i PATH="$PATH" $set_all bash "$tmp/check-secrets.sh" >"$tmp/out" 2>&1); then
        fail "the secret check passed with the placeholder SUPublicEDKey"
    fi
    grep -q placeholder "$tmp/out" || fail "the placeholder key is not what stopped it: $(cat "$tmp/out")"
fi

# --- scripts/desktop-version.sh ----------------------------------------------

git init -q "$tmp/repo"
cd "$tmp/repo"
git config user.name test
git config user.email test@example.invalid
git config commit.gpgsign false
git config tag.gpgsign false
git commit -q --allow-empty -m "feat: first"
git tag v9.9.9
version() { bash "$ROOT/scripts/desktop-version.sh" "$@" 2>&1; }

out="$(version 0.1.0)" || fail "the first app version was refused: $out"
echo "$out" | grep -qx 'tag=desktop-v0.1.0' || fail "wrong tag: $out"
echo "$out" | grep -qx 'previous=' || fail "a CLI tag was taken for an app release: $out"

git tag desktop-v0.1.0
git tag desktop-v0.10.0
out="$(version 0.11.0)" || fail "0.11.0 after 0.10.0 was refused: $out"
echo "$out" | grep -qx 'previous=desktop-v0.10.0' || fail "0.10.0 does not sort above 0.1.0: $out"
for bad in 0.10.0 0.9.0 v0.12.0 0.12 0.12.0-beta.1 01.0.0 '0.12.0;id'; do
    if out="$(version "$bad")"; then
        fail "version '$bad' was accepted: $out"
    fi
done

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
