#!/usr/bin/env python3
"""Mirror the repository's Markdown into the Hugo site.

The files in `docs/` are the documentation. They are written to be read on
GitHub, in an editor, and on the website, and keeping a second copy in
`site/content` would mean keeping two copies honest. So the site's copies are
generated: front matter is added here, links between the documents are rewritten
to site URLs, and the result is ignored by git.

Run it before `hugo`; the Pages workflow does exactly that.
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "site" / "content" / "docs"

# source path -> (slug, title, sidebar weight, description)
PAGES = [
    (
        "docs/MANIFESTS.md",
        "manifests",
        "Manifests",
        20,
        "The package config: where a package comes from, which release asset to "
        "take, and what to expose once it is unpacked. Every field, and when you "
        "need one at all.",
    ),
    (
        "docs/REGISTRY.md",
        "registry",
        "The package registry",
        30,
        "One folder per package, one ketch.toml inside. How the registry is laid "
        "out, what ketch validates before trusting an entry, and how to add a "
        "package to it.",
    ),
    (
        "docs/PLUGINS.md",
        "plugins",
        "Source plugins",
        40,
        "Install from GitLab, Gitea or an internal artifact server. A plugin is "
        "one executable that answers in JSON — the whole protocol, with a "
        "working example.",
    ),
    (
        "docs/LOCKFILE.md",
        "lockfile",
        "The lockfile",
        45,
        "ketch.lock pins one machine's tools to exact releases: what it "
        "records, what reproduces on another machine and what does not, and "
        "how sync catches up.",
    ),
    (
        "ROADMAP.md",
        "roadmap",
        "Roadmap",
        50,
        "What ketch does not do yet and what it would take: Linux and Windows "
        "backends, signature verification, man pages and shell completions.",
    ),
    (
        "AGENTS.md",
        "contributing",
        "Contributing",
        60,
        "The layout, the conventions and the trust boundaries — what to read "
        "before changing anything, and where each kind of change belongs.",
    ),
]

# Every way one document links to another, mapped to the slug it becomes. Longer
# paths first, so `docs/REGISTRY.md` never matches as a bare `REGISTRY.md`.
LINKS = sorted(
    (
        [(src, slug) for src, slug, _, _, _ in PAGES]
        + [(pathlib.Path(src).name, slug) for src, slug, _, _, _ in PAGES]
    ),
    key=lambda pair: -len(pair[0]),
)


def front_matter(title: str, description: str, weight: int, source: str) -> str:
    return (
        "---\n"
        f'title: "{title}"\n'
        f'description: "{description}"\n'
        f"weight: {weight}\n"
        f'source: "{source}"\n'
        "---\n\n"
    )


def rewrite_links(body: str) -> str:
    """Point inter-document links at the site instead of at the repository."""
    for source, slug in LINKS:
        # Sibling pages, so one level up and back down: /docs/x/ -> /docs/y/.
        body = body.replace(f"]({source})", f"](../{slug}/)")
        body = body.replace(f"](./{source})", f"](../{slug}/)")
    # The README has no page of its own; the landing page is its equivalent.
    body = body.replace("](README.md)", "](../../)")
    return body


def strip_leading_h1(body: str) -> str:
    """The layout renders the title, so a repeated H1 would show up twice."""
    return re.sub(r"\A#\s+.*\n+", "", body.lstrip())


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    written = []
    for source, slug, title, weight, description in PAGES:
        path = ROOT / source
        if not path.is_file():
            print(f"missing: {source}", file=sys.stderr)
            return 1
        body = rewrite_links(strip_leading_h1(path.read_text()))
        (OUT / f"{slug}.md").write_text(
            front_matter(title, description, weight, source) + body
        )
        written.append(f"{source} -> content/docs/{slug}.md")

    print("\n".join(written))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
