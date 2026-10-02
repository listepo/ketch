#!/usr/bin/env bash
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# Build the seed corpora in fuzz/corpus/<target>/ from files the repository
# already has: tests/fixtures, the root ketch.toml, src/builtin.toml, the code
# blocks in docs/, the example plugin, and archives made here from a tiny tree.
# Nothing is committed: fuzz/corpus is ignored, and every run of this script
# adds the same files again, so a corpus the fuzzer grew is kept alongside.
#
# `cli-argv` and `extra-paths` take structured input (`arbitrary`), which a
# text file does not seed usefully; they start from an empty corpus.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
corpus="$root/fuzz/corpus"
fixtures="$root/tests/fixtures"

seed() { # <target> <name>, contents on stdin
    mkdir -p "$corpus/$1"
    cat >"$corpus/$1/seed-$2"
}

# Every fenced block of one language in a Markdown file, one seed per block.
blocks() { # <target> <lang> <file>
    local target=$1 lang=$2 file=$3 n=0 inside=0 buf=""
    while IFS= read -r line || [ -n "$line" ]; do
        if [ $inside = 0 ] && [ "$line" = "\`\`\`$lang" ]; then
            inside=1 buf=""
        elif [ $inside = 1 ] && [ "$line" = "\`\`\`" ]; then
            inside=0 n=$((n + 1))
            printf '%s' "$buf" | seed "$target" "$(basename "$file" .md)-$n"
        elif [ $inside = 1 ]; then
            buf+="$line"$'\n'
        fi
    done <"$file"
}

# manifest-toml
seed manifest-toml ketch.toml <"$root/ketch.toml"
seed manifest-toml builtin.toml <"$root/src/builtin.toml"
blocks manifest-toml toml "$root/docs/MANIFESTS.md"
blocks manifest-toml toml "$root/docs/REGISTRY.md"

# lockfile
blocks lockfile toml "$root/docs/LOCKFILE.md"

# state
printf '{"version":1,"packages":{}}\n' | seed state empty.json

# checksum-file
seed checksum-file SHA256SUMS <"$fixtures/trust/SHA256SUMS"
printf 'sha256:%s\n' "$(cut -c1-64 "$fixtures/trust/SHA256SUMS" | head -n1)" | seed checksum-file digest

# plugin-protocol
for sub in capabilities describe releases; do
    sh "$root/examples/ketch-source-example" "$sub" owner/tool | seed plugin-protocol "example-$sub"
done
blocks plugin-protocol json "$root/docs/PLUGINS.md"

# package-spec
for spec in ripgrep ripgrep@14.1.0 BurntSushi/ripgrep github:BurntSushi/ripgrep@v14 example:owner/tool; do
    printf '%s' "$spec" | seed package-spec "$(printf '%s' "$spec" | tr -c 'A-Za-z0-9.@-' _)"
done

# hook-line
printf '%s' 'echo "$KETCH_PREFIX" && ln -sf "$KETCH_PREFIX/bin/tool" "$HOME/tool"' | seed hook-line sh
printf '%s' '"C:\Program Files\tool\setup.exe" /quiet & echo %KETCH_PREFIX%' | seed hook-line cmd

# printable
for f in "$fixtures"/commit-msg/*.txt; do
    seed printable "$(basename "$f")" <"$f"
done
printf 'plain\n\033[2J\033]0;title\007\342\200\256reversed\n' | seed printable escapes
head -c 4096 "$root/CHANGELOG.md" | seed printable CHANGELOG.md

# archive-extract: the signed fixture, and one of each format from a small tree
# with a nested file and an in-tree symlink.
seed archive-extract signedtool.tar.gz <"$fixtures/trust/signedtool.tar.gz"
tree=$(mktemp -d)
trap 'rm -rf "$tree"' EXIT
mkdir -p "$tree/tool-1.0.0/bin" "$tree/tool-1.0.0/share/man/man1"
printf '#!/bin/sh\necho tool\n' >"$tree/tool-1.0.0/bin/tool"
chmod +x "$tree/tool-1.0.0/bin/tool"
printf '.TH TOOL 1\n' >"$tree/tool-1.0.0/share/man/man1/tool.1"
ln -s bin/tool "$tree/tool-1.0.0/tool"
(cd "$tree" && tar -cf - tool-1.0.0) | seed archive-extract tool.tar
(cd "$tree" && tar -czf - tool-1.0.0) | seed archive-extract tool.tar.gz
(cd "$tree" && tar -cjf - tool-1.0.0) | seed archive-extract tool.tar.bz2
(cd "$tree" && tar -cJf - tool-1.0.0) | seed archive-extract tool.tar.xz
if command -v zip >/dev/null; then
    (cd "$tree" && zip -qry tool.zip tool-1.0.0)
    seed archive-extract tool.zip <"$tree/tool.zip"
fi

echo "seeded $(find "$corpus" -name 'seed-*' | wc -l | tr -d ' ') files under $corpus"
