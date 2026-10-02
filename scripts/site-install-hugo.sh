#!/usr/bin/env bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# Install Hugo HUGO_VERSION (linux-amd64) from the project's own release, checked against the
# checksum published beside it: the same thing ketch does for everything it installs.
# Used by .github/workflows/pages.yml (pyrlyn/infra pages.yml `setup-command`).
set -euo pipefail
: "${HUGO_VERSION:?set HUGO_VERSION, e.g. 0.165.0}"
work="$(mktemp -d)"
cd "$work"
base="https://github.com/gohugoio/hugo/releases/download/v${HUGO_VERSION}"
tarball="hugo_${HUGO_VERSION}_linux-amd64.tar.gz"
curl -fsSLO "$base/$tarball"
curl -fsSLO "$base/hugo_${HUGO_VERSION}_checksums.txt"
grep " $tarball\$" "hugo_${HUGO_VERSION}_checksums.txt" | sha256sum -c -
tar -xzf "$tarball" hugo
sudo install -m 755 hugo /usr/local/bin/hugo
hugo version
