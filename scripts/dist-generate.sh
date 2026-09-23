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

create_old = """          # Write and read notes from a file to avoid quoting breaking things
          echo "$ANNOUNCEMENT_BODY" > $RUNNER_TEMP/notes.txt

          gh release create"""
create_new = """          # Write and read notes from a file to avoid quoting breaking things
          echo "$ANNOUNCEMENT_BODY" > $RUNNER_TEMP/notes.txt

          # One aggregate checksum file, named so install.sh, install.ps1 and
          # `ketch self upgrade` find it. Sorted, so the file is reproducible.
          (cd artifacts && sha256sum $(ls ketch-*.tar.gz | sort) > SHA256SUMS && cat SHA256SUMS)

          # Archive sizes, so the release page shows them without opening Assets.
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

          gh release create"""
if create_old not in text:
    fail("the Create GitHub Release block is missing or changed")
text = text.replace(create_old, create_new, 1)

path.write_text(text)
print(f"patched {path}: MACOS_* secrets, build-check steps, SHA256SUMS, download sizes")
PY
