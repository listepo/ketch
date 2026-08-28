#!/usr/bin/env node
/**
 * Mirror the repository's Markdown into the docs site.
 *
 * The files in `docs/` are the documentation. They are written to be read on
 * GitHub, in an editor, and on the website, and keeping a second copy here
 * would mean keeping two copies honest. So these copies are generated: front
 * matter is added, links between the documents are rewritten to site URLs,
 * and the result is ignored by git.
 *
 * Run it before `docusaurus build`; the Pages workflow does exactly that.
 */

import { mkdirSync, readFileSync, existsSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const out = path.join(here, "docs");

/** The landing page: a separate build, deployed at the site root. */
const LANDING = "https://listepo.github.io/ketch/";

/** Source file, the slug it becomes, its title, sidebar order, and blurb. */
const PAGES = [
  {
    source: "docs/MANIFESTS.md",
    slug: "manifests",
    title: "Manifests",
    position: 20,
    description:
      "The package config: where a package comes from, which release asset to take, and what to expose once it is unpacked. Every field, and when you need one at all.",
  },
  {
    source: "docs/REGISTRY.md",
    slug: "registry",
    title: "The package registry",
    position: 30,
    description:
      "One folder per package, one ketch.json inside. How the registry is laid out, what ketch validates before trusting an entry, and how to add a package to it.",
  },
  {
    source: "docs/PLUGINS.md",
    slug: "plugins",
    title: "Source plugins",
    position: 40,
    description:
      "Install from GitLab, Gitea or an internal artifact server. A plugin is one executable that answers in JSON — the whole protocol, with a working example.",
  },
  {
    source: "docs/LOCKFILE.md",
    slug: "lockfile",
    title: "The lockfile",
    position: 45,
    description:
      "ketch.lock pins one machine's tools to exact releases: what it records, what reproduces on another machine and what does not, and how sync catches up.",
  },
  {
    source: "ROADMAP.md",
    slug: "roadmap",
    title: "Roadmap",
    position: 50,
    description:
      "What ketch does not do yet and what it would take: Linux and Windows backends, signature verification, man pages and shell completions.",
  },
  {
    source: "AGENTS.md",
    slug: "contributing",
    title: "Contributing",
    position: 60,
    description:
      "The layout, the conventions and the trust boundaries — what to read before changing anything, and where each kind of change belongs.",
  },
];

// Every way one document links to another, mapped to the slug it becomes.
// Longest first, so `docs/REGISTRY.md` never matches as a bare `REGISTRY.md`.
const LINKS = [
  ...PAGES.map(({ source, slug }) => [source, slug]),
  ...PAGES.map(({ source, slug }) => [path.basename(source), slug]),
].sort((a, b) => b[0].length - a[0].length);

/**
 * Point inter-document links at the sibling page instead of at the repository.
 *
 * The target keeps its `.md` suffix on purpose: Docusaurus resolves a link to
 * a source file into that file's final URL, while a bare `./registry` is
 * resolved against the current page and lands under it.
 */
function rewriteLinks(body) {
  let text = body;
  for (const [source, slug] of LINKS) {
    text = text.replaceAll(`](${source})`, `](./${slug}.md)`);
    text = text.replaceAll(`](./${source})`, `](./${slug}.md)`);
  }
  // The README has no page of its own; the landing page is its equivalent,
  // and it is a different build, so it can only be reached absolutely.
  return text.replaceAll("](README.md)", `](${LANDING})`);
}

/** The theme renders the title, so a repeated H1 would show up twice. */
function stripLeadingH1(body) {
  return body.trimStart().replace(/^#\s+.*\n+/, "");
}

/** YAML needs the quotes escaped, and these blurbs are prose. */
function quote(text) {
  return `"${text.replaceAll("\\", "\\\\").replaceAll('"', '\\"')}"`;
}

const INDEX = `---
id: index
title: "Documentation"
description: "How ketch installs tools and macOS apps from GitHub releases: manifests, the registry, source plugins and the lockfile."
slug: /
sidebar_position: 10
---

# Documentation

ketch installs command-line tools and macOS apps straight from GitHub
releases. It downloads what a project already ships, verifies it, unpacks it
into a store under \`~/.ketch\`, and links it onto your \`PATH\`.

\`\`\`bash
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | bash
\`\`\`

- **[Manifests](./manifests.md)** — where a package comes from, which release
  asset to take, and what to expose once it is unpacked.
- **[The package registry](./registry.md)** — one folder per package, and how
  to add yours.
- **[Source plugins](./plugins.md)** — install from GitLab, Gitea or an
  internal artifact server; a plugin is one executable that answers in JSON.
- **[The lockfile](./lockfile.md)** — pin a machine's tools to exact releases
  and reproduce them elsewhere.
- **[Roadmap](./roadmap.md)** and **[Contributing](./contributing.md)**.
`;

mkdirSync(out, { recursive: true });
writeFileSync(path.join(out, "index.md"), INDEX);
const written = ["(generated) -> docs/index.md"];
for (const page of PAGES) {
  const file = path.join(root, page.source);
  if (!existsSync(file)) {
    process.stderr.write(`missing: ${page.source}\n`);
    process.exit(1);
  }
  const frontMatter = [
    "---",
    `id: ${page.slug}`,
    `title: ${quote(page.title)}`,
    `description: ${quote(page.description)}`,
    `sidebar_position: ${page.position}`,
    // Recorded so a reader who wants to edit the page knows which file is real.
    `source: ${quote(page.source)}`,
    "---",
    "",
    "",
  ].join("\n");
  const body = rewriteLinks(stripLeadingH1(readFileSync(file, "utf8")));
  writeFileSync(path.join(out, `${page.slug}.md`), frontMatter + body);
  written.push(`${page.source} -> docs/${page.slug}.md`);
}
process.stdout.write(`${written.join("\n")}\n`);
