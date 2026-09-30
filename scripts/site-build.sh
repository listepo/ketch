#!/usr/bin/env bash
# Build site/public: the Pages artifact of .github/workflows/pages.yml (pyrlyn/infra pages.yml
# `build-command`). Needs hugo (scripts/site-install-hugo.sh), node/npm (mise.toml) and python3.
# BASE_URL: the site's public URL (default https://pyrlyn.github.io/ketch).
set -euo pipefail
cd "$(dirname "$0")/.."
BASE_URL="${BASE_URL:-https://pyrlyn.github.io/ketch}"

python3 site/test_sync_docs.py

# Landing Mini UI utilities are Tailwind; Node comes from mise.toml like commitlint's.
npm ci
npm run css

# docs/*.md is the documentation; the site's copies are generated from it so the two can
# never drift.
python3 site/sync-docs.py

hugo --source site --minify --gc --baseURL "$BASE_URL/"

npx --yes pagefind@1 --site site/public

# Check the build produced what it should.
(
cd site/public
for required in index.html sitemap.xml robots.txt docs/index.html \
                docs/manifests/index.html docs/registry/index.html \
                docs/plugins/index.html docs/roadmap/index.html \
                docs/plan/index.html docs/troubleshooting/index.html \
                css/main.css css/mini-ui.css img/og.png \
                pagefind/pagefind-ui.js pagefind/pagefind-ui.css; do
  [ -s "$required" ] || { echo "missing or empty: $required" >&2; exit 1; }
done
# The SEO tags are the point of the landing page; a template that
# silently renders nothing would still produce a valid-looking site.
# --minify drops quotes around attribute values, so match either form.
grep -qE '<meta name="?description"?' index.html
grep -qE '<link rel="?canonical"?' index.html
grep -q 'og:image' index.html
# `ld+json>{` and not `ld+json>"`: Go escapes a string in a script
# context into a JS string literal, which is not parseable JSON-LD.
grep -q 'ld+json>{' index.html
grep -q 'SoftwareApplication' index.html
grep -q 'TechArticle' docs/manifests/index.html
# Hugo truncates an over-long description with an ellipsis, which is
# what a search result would then show. Catch it here rather than on
# the deployed page.
if grep -rq 'name=description content="[^"]*…"' .; then
  echo "a meta description was truncated; keep it under 160 characters:" >&2
  grep -rl 'name=description content="[^"]*…"' . >&2
  exit 1
fi
echo "site ok: $(find . -name '*.html' | wc -l) pages"
)
