#!/bin/sh
# Copyright (c) 2026 Ivan Tugay
# SPDX-License-Identifier: GPL-3.0-or-later
# Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

# commitlint treats only type!: / type(scope)!: as a breaking header marker, not ! elsewhere.
set -eu

breaking_header() {
    header="${1%%:*}"
    [ "${header%"!"}" != "$header" ]
}

assert_not_breaking() {
    if breaking_header "$1"; then
        echo "commit-msg-breaking: expected not breaking: $1" >&2
        exit 1
    fi
}

assert_breaking() {
    if ! breaking_header "$1"; then
        echo "commit-msg-breaking: expected breaking: $1" >&2
        exit 1
    fi
}

assert_not_breaking "feat(api!): scope bang is not breaking"
assert_breaking "feat!: type bang is breaking"
assert_breaking "feat(api)!: scoped type bang is breaking"
