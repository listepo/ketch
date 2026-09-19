#!/bin/sh
# F1: release.yml must parse, and the notarisation switch must stay load-bearing.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
wf="$ROOT/.github/workflows/release.yml"

if [ ! -f "$wf" ]; then
    echo "release-yml-notarize: missing $wf" >&2
    exit 1
fi

if command -v ruby >/dev/null 2>&1; then
    ruby -ryaml -e "YAML.load_file(ARGV[0])" "$wf"
else
    echo "release-yml-notarize: ruby is required to parse the workflow" >&2
    exit 1
fi

need() {
    if ! grep -q "$1" "$wf"; then
        echo "release-yml-notarize: missing $2" >&2
        exit 1
    fi
}

need 'name: Notarise' 'the Notarise step'
need "vars.KETCH_NOTARIZE == 'true'" 'the KETCH_NOTARIZE gate'
need 'secrets.APPSTORE_CONNECT_KEY' 'APPSTORE_CONNECT_KEY'
need 'secrets.APPSTORE_CONNECT_KEY_ID' 'APPSTORE_CONNECT_KEY_ID'
need 'secrets.APPSTORE_CONNECT_ISSUER_ID' 'APPSTORE_CONNECT_ISSUER_ID'
need 'xcrun notarytool submit' 'notarytool submit'
need 'source=Notarized Developer ID' 'the spctl smoke check'
need 'bump:' 'the manual bump input'
need 'options: \[patch, minor, major\]' 'the patch/minor/major choice'
need '^  propose:' 'the propose job'
