#!/usr/bin/env bash
# Regenerate .github/workflows/release.yml from dist-workspace.toml, then patch
# in what dist has no setting for:
#
# - dist's CODESIGN_* secret names mapped to the MACOS_* secrets this
#   repository holds. CODESIGN_IDENTITY is not a secret: .github/build-setup.yml
#   discovers it on macOS runners.
# - .github/build-check.yml (notarisation and the smoke test) after `dist
#   build`, before each target's artifacts are uploaded.
# - An aggregate `SHA256SUMS` beside the tarballs, and their sizes in the
#   release notes. `install.sh`, `install.ps1` and every `ketch self upgrade`
#   already installed read `SHA256SUMS`; dist's own aggregate is `sha256.sum`.
#
# Invoked by `just dist-generate`. Do not hand-edit release.yml; change
# dist-workspace.toml, .github/build-setup.yml or .github/build-check.yml and
# re-run this.
#
# allow-dirty = ["ci"] is set so `dist plan` / `dist build` accept the patched
# workflow. That same flag makes bare `dist generate` skip writing release.yml,
# so this script briefly clears it, generates, then restores the file.

set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

dist_bin="${DIST:-mise exec -- dist}"

cfg="dist-workspace.toml"
cfg_backup="$(mktemp)"
cp "$cfg" "$cfg_backup"
cleanup() { mv "$cfg_backup" "$cfg"; }
trap cleanup EXIT

# Drop allow-dirty for the generate pass so release.yml is rewritten.
python3 - "$cfg" <<'PY'
from pathlib import Path
import re
import sys
path = Path(sys.argv[1])
text = path.read_text()
text2 = re.sub(
    r"(?m)^(?:#.*post-patched.*\n)?allow-dirty\s*=\s*\[[^\]]*\]\s*\n",
    "",
    text,
    count=1,
)
if text2 == text:
    sys.exit("dist-generate: no allow-dirty line to clear in dist-workspace.toml")
path.write_text(text2)
PY

# shellcheck disable=SC2086
$dist_bin generate

# Restore config (with allow-dirty) before patching, so the working tree matches intent.
mv "$cfg_backup" "$cfg"
trap - EXIT

workflow=".github/workflows/release.yml"
[ -f "$workflow" ] || { echo "expected $workflow after dist generate" >&2; exit 1; }

python3 - "$workflow" .github/build-check.yml <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
check = pathlib.Path(sys.argv[2]).read_text()
text = path.read_text()


def fail(message):
    sys.exit(f"dist-generate patch: {message}")


for old, new in [
    (
        "CODESIGN_CERTIFICATE: ${{ secrets.CODESIGN_CERTIFICATE }}",
        "CODESIGN_CERTIFICATE: ${{ secrets.MACOS_CERTIFICATE }}",
    ),
    (
        "CODESIGN_CERTIFICATE_PASSWORD: ${{ secrets.CODESIGN_CERTIFICATE_PASSWORD }}",
        "CODESIGN_CERTIFICATE_PASSWORD: ${{ secrets.MACOS_CERTIFICATE_PWD }}",
    ),
    (
        "      CODESIGN_IDENTITY: ${{ secrets.CODESIGN_IDENTITY }}\n",
        "      # CODESIGN_IDENTITY: set on macOS by .github/build-setup.yml (not a secret)\n",
    ),
]:
    if old not in text:
        fail(f"missing expected line:\n  {old.strip()}")
    text = text.replace(old, new)

# .github/build-check.yml, indented as steps of build-local-artifacts, right
# before its upload.
anchor = "          name: artifacts-build-local-${{ join(matrix.targets, '_') }}"
idx = text.find(anchor)
if idx < 0:
    fail("missing the build-local-artifacts upload")
step = text.rfind('      - name: "Upload artifacts"', 0, idx)
if step < 0:
    fail("missing the build-local-artifacts upload step")
steps = "".join(
    ("      " + line if line.strip() else line)
    for line in check.splitlines(keepends=True)
)
text = text[:step] + steps + "\n" + text[step:]

# create-release = false: bump.yml made the tag and a draft release (notes
# from CHANGELOG.md); dist uploads to it and publishes it. SHA256SUMS goes up
# with the tarballs, and the sizes are appended to bump's notes.
create_old = """          # If we're editing a release in place, we need to upload things ahead of time
          gh release upload "${{ needs.plan.outputs.tag }}" artifacts/*

          gh release edit "${{ needs.plan.outputs.tag }}" --target "$RELEASE_COMMIT" $PRERELEASE_FLAG --draft=false
"""
create_new = """          # One aggregate checksum file, named so install.sh, install.ps1 and
          # `ketch self upgrade` find it. Sorted, so the file is reproducible.
          (cd artifacts && sha256sum $(ls ketch-*.tar.gz | sort) > SHA256SUMS && cat SHA256SUMS)

          # If we're editing a release in place, we need to upload things ahead of time
          gh release upload "${{ needs.plan.outputs.tag }}" artifacts/*

          # bump.yml wrote the notes; add the archive sizes, so the release page
          # shows them without opening Assets.
          gh release view "${{ needs.plan.outputs.tag }}" --json body --jq .body > "$RUNNER_TEMP/notes.txt"
          {
            echo
            echo "## Download sizes"
            echo
            echo "| File | Size |"
            echo "|---|---:|"
            for f in $(ls artifacts/ketch-*.tar.gz | sort); do
              bytes=$(wc -c <"$f" | tr -d ' ')
              echo "| $(basename "$f") | $(awk -v b="$bytes" 'BEGIN { printf "%.2f MiB", b/1048576 }') |"
            done
          } >> "$RUNNER_TEMP/notes.txt"
          sed -n '/^## Download sizes$/,$p' "$RUNNER_TEMP/notes.txt" | tee -a "$GITHUB_STEP_SUMMARY"

          gh release edit "${{ needs.plan.outputs.tag }}" --target "$RELEASE_COMMIT" $PRERELEASE_FLAG --notes-file "$RUNNER_TEMP/notes.txt" --draft=false
"""
if create_old not in text:
    fail("the release upload block is missing or changed (create-release = false?)")
text = text.replace(create_old, create_new, 1)

# A last job that turns a failed release into a `release-failure` issue (pyrlyn/infra).
NOTIFY = """
  # Added by scripts/dist-generate.sh: a failed release (not a pull request or a dry run)
  # opens or comments on a `release-failure` issue that mentions and assigns @listepo. The
  # only release failure notification: GitHub cannot filter Actions notifications per
  # workflow. Pinned to pyrlyn/infra's ci/notify-release-failure; repin to its merge commit.
  notify-failure:
    needs: [plan, build-local-artifacts, build-global-artifacts, host, custom-tap, announce]
    if: >-
      always() && github.event_name == 'workflow_dispatch' && inputs.tag != 'dry-run'
      && contains(needs.*.result, 'failure')
    runs-on: "ubuntu-22.04"
    timeout-minutes: 5
    permissions:
      "actions": "read"
      "issues": "write"
    steps:
      - uses: pyrlyn/infra/.github/actions/notify-release-failure@d709124d53dd4923eff8f594b3155842508b0049
        with:
          ref: ${{ inputs.tag }}
          needs: ${{ toJSON(needs) }}
"""
if "notify-failure:" not in text:
    text = text.rstrip("\n") + "\n" + NOTIFY

path.write_text(text)
print(
    f"patched {path}: MACOS_* secrets, build-check steps, SHA256SUMS, download sizes, "
    "notify-failure job"
)
PY
