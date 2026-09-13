#!/bin/sh
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
