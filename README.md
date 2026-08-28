<div align="center">

<img src="apps/web/public/favicon.svg" width="72" alt="">

# ketch

**Catch releases straight from GitHub.**

Install command-line tools and macOS apps directly from GitHub releases.
No taps, no formulae, no build step — ketch downloads what a project already
ships, verifies it, and puts it on your `PATH`.

[![ci](https://github.com/listepo/ketch/actions/workflows/ci.yml/badge.svg)](https://github.com/listepo/ketch/actions/workflows/ci.yml)
[![site](https://github.com/listepo/ketch/actions/workflows/pages.yml/badge.svg)](https://github.com/listepo/ketch/actions/workflows/pages.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![platform](https://img.shields.io/badge/platform-macOS-lightgrey.svg)

[Website](https://listepo.github.io/ketch/) ·
[Documentation](https://listepo.github.io/ketch/docs/) ·
[Registry](https://github.com/listepo/ketch-registry) ·
[Roadmap](ROADMAP.md)

</div>

---

```bash
ketch install BurntSushi/ripgrep     # any repo that publishes releases
ketch install rg                     # or a name ketch already knows
ketch install sharkdp/fd@v10.2.0     # or an exact version
```

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | bash
```

That downloads a compiled binary for your machine — there is no runtime to
install and nothing to build. Then make sure `~/.ketch/bin` is on your `PATH`.
`ketch doctor` will tell you if it is not, along with anything else that needs
attention.

## Why

Most command-line tools are already published as a release asset built for your
machine. A package manager does not need to compile them, and a maintainer does
not need to write a formula for them — the artefact is right there. ketch picks
the right one, checks the checksum the project published, unpacks it into a
versioned store, and links it onto your `PATH`.

- **Any repo that ships releases.** No formula, no tap, no waiting for a
  maintainer. Point it at `owner/repo`.
- **Verified, not just downloaded.** Published SHA-256 sums are checked against
  what landed on disk. `require_checksums` refuses anything that publishes none,
  and ketch's own updates never accept trust-on-first-use.
- **Apps as well as binaries.** An `.app` bundle goes to `/Applications`,
  quarantine cleared when the signature checks out, removed cleanly on
  uninstall.
- **One tree.** Everything under `~/.ketch`. Uninstalling leaves nothing behind.
- **Sources beyond GitHub.** A plugin is one executable that answers in JSON —
  no recompile, no ketch release.

## Using it

```bash
ketch install <pkg>...     # install; concurrent by default, --jobs N to change
ketch list                 # what is installed
ketch outdated             # what has a newer release
ketch upgrade              # bring everything unpinned up to date
ketch info <pkg>           # details, including --assets and why each scored
ketch search <query>       # the registry and GitHub
ketch changelog <pkg>      # what changed: the shipped file, or the release notes
ketch update               # refresh the package registry
ketch pin / unpin <pkg>    # hold a version, or let go
ketch link / unlink <pkg>  # re-create the links, or take them away
ketch uninstall <pkg>...   # remove it
ketch lock                 # write ketch.lock from what is installed
ketch sync                 # install what ketch.lock names, at those versions
ketch doctor               # check the environment and the install tree
ketch doctor --fix         # and repair the PATH setup while it is there
ketch path install         # put ~/.ketch/bin on PATH in bash, zsh and fish
ketch plugin list          # the source plugins ketch found
ketch completions zsh      # a completion script for bash, zsh or fish
ketch self update          # upgrade ketch itself
```

Everything lives under `~/.ketch`: versioned payloads in `store/`, links in
`bin/`, and a `state.json` recording what is installed. Nothing is written
outside that tree except the `.app` bundles that belong in `/Applications`, and
the shell startup file `ketch path install` edits when you ask it to.

## Seeing what changed

```bash
ketch changelog rg            # the entry for the version you have
ketch changelog rg --latest   # the release you would get by upgrading
ketch changelog rg --release  # the notes on the release, not the shipped file
```

Most releases carry their history twice: a `CHANGELOG.md` inside the archive
and the notes attached to the release. ketch prefers the file — it is already
on disk, so this works with no network — and falls back to the notes when the
file has no entry for that version, which is what happens whenever a project
tags before writing the heading.

The changelog goes to stdout and everything else to stderr, so
`ketch changelog rg > NOTES.md` leaves nothing but the markdown.

## Installing several at once

```bash
ketch install rg fd bat jq      # four downloads at a time, four progress bars
ketch install rg fd --jobs 1    # one at a time
```

Downloads run concurrently by default and spend their time waiting, so a batch
takes about as long as its slowest package rather than the sum of all of them.
Only the downloading and unpacking overlap: packages are placed into the store
one at a time, in the order you asked for them, so the install tree sees the
same sequence of writes it would have seen anyway.

`upgrade` and `sync` work the same way. `--jobs N` sets the width for any of
them; `jobs` in the config file sets the default.

## The log

Every run is written to `~/.ketch/logs/ketch.log` — including the lines
`--quiet` swallowed and the debug detail `--verbose` would have shown. A failed
command prints where to find it.

```
[2026-08-27T09:12:33Z] INFO (4218): ketch 0.2.0 · install rg fd
[2026-08-27T09:12:34Z] ERROR (4218): HTTP 404 from https://api.github.com/...
```

Set `log_format` to `"json"` for JSON Lines instead — `{"level":"info","time":…,
"pid":…,"msg":…}`, one object per line — `log_level` to `debug` for everything
or `off` for nothing. The file rotates to `ketch.log.1` at 5 MiB, so it never
needs pruning by hand. `ketch doctor` prints where it is and how big it has
grown.

## Reproducing a machine

```bash
ketch lock            # write ./ketch.lock from what is installed
ketch lock --check    # has anything drifted?
ketch sync            # make this machine match the lockfile
```

Commit `ketch.lock` next to your dotfiles and a new machine is one command
behind the old one. The tag is what reproduces everywhere; the recorded
checksum is enforced on a machine of the same target, and elsewhere ketch picks
the asset that fits and verifies it against the source. `--prune` removes what
the lockfile does not name, `--dry-run` shows the plan first. See
[docs/LOCKFILE.md](docs/LOCKFILE.md).

## Getting on PATH

```bash
ketch path              # which shells are set up, and where
ketch path install      # edit the ones you use
ketch path uninstall    # take the block back out
```

It detects bash, zsh and fish — the shell `$SHELL` names, plus any whose startup
file you already keep — and writes one block between markers, so it can rewrite
it if the root moves and remove it cleanly later. `--shell <name>` picks one,
`--all` takes all three, `--dry-run` shows the edit without making it, and
`--print` gives you the line to paste somewhere ketch does not know about.

A line you added yourself is left alone rather than duplicated. `ketch doctor`
reports the PATH as a failure when no shell knows about it, as a warning when a
startup file has it but the current shell predates the edit, and `ketch doctor
--fix` does the setup for you.

## How a package is found

A name is resolved against four tiers, in order:

1. your own manifests in `~/.ketch/manifests/`
2. the fetched package registry — see [docs/REGISTRY.md](docs/REGISTRY.md)
3. the registry compiled into ketch, so common tools work offline
4. inference from `owner/repo`, which is what makes an uncurated repository
   installable with no manifest at all

Inference picks the release asset that matches your machine — reading the OS and
architecture out of the file name, discarding anything that names another
platform, and passing over signature sidecars and checksum files. It explains
its choice under `ketch info --assets`.

When it guesses wrong, a manifest says what to do instead: which asset, which
binaries, under what names. See [docs/MANIFESTS.md](docs/MANIFESTS.md).

Sources other than GitHub are added as plugins — a single executable, no ketch
release required. See [docs/PLUGINS.md](docs/PLUGINS.md).

## The files ketch reads

Every data file is JSON, and every one has a published JSON Schema generated
from the Zod schema that validates it. Point a file's `$schema` at its URL and
your editor will complete and check it as you type.

| File | Schema | What it is |
| --- | --- | --- |
| `~/.ketch/config.json` | `config.schema.json` | your settings, all optional |
| `~/.ketch/manifests/<name>.json` | `manifest.schema.json` | a package manifest of your own |
| `<package>/ketch.json` | `registry.schema.json` | a registry entry — a manifest whose name comes from the folder |
| `./ketch.lock` | `lockfile.schema.json` | what is installed, pinned to exact releases |
| `~/.ketch/state.json` | `state.schema.json` | ketch's own record of what it installed |

The URLs are
`https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/<name>.schema.json`,
and the schemas themselves are committed under
[`packages/schemas/schemas/`](packages/schemas/schemas).

Unknown keys are refused rather than ignored, in every one of them: a misspelt
setting that is silently dropped is worse than one that fails.

## Configuration

`~/.ketch/config.json`, with environment variables taking precedence, and
command-line flags over those:

| Key | Environment | Default |
| --- | --- | --- |
| `apps_dir` | `KETCH_APPS_DIR` | `/Applications` |
| `github_token` | `KETCH_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN` | none |
| `prerelease` | `KETCH_PRERELEASE` | `false` |
| `allow_emulation` | `KETCH_ALLOW_EMULATION` | `true` |
| `link_apps` | `KETCH_LINK_APPS` | `false` |
| `require_checksums` | `KETCH_REQUIRE_CHECKSUMS` | `false` |
| `strip_quarantine` | `KETCH_STRIP_QUARANTINE` | `true` |
| `registry` | `KETCH_REGISTRY` | `listepo/ketch-registry` |
| `self_repo` | `KETCH_SELF_REPO` | `listepo/ketch` |
| `jobs` | `KETCH_JOBS` | `4`, capped at `16` |
| `log_level` | `KETCH_LOG_LEVEL` | `info` |
| `log_format` | `KETCH_LOG_FORMAT` | `text` |

```json
{
  "$schema": "https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/config.schema.json",
  "require_checksums": true,
  "jobs": 8
}
```

The root itself is `KETCH_ROOT` or `--root`; it cannot be set from the config
file, because the file lives inside it. Setting it there is warned about rather
than honoured silently.

The first token variable that is *set* wins even when it is empty, so an
explicitly blank `KETCH_GITHUB_TOKEN` suppresses an inherited `GITHUB_TOKEN`
instead of falling through to it. A token is not required, but it raises
GitHub's rate limit considerably.

## Platform support

macOS only for now. The OS-specific parts sit behind one `Platform` interface in
`packages/core/src/platform/`, so a Linux or Windows backend means implementing
that interface and nothing above it. [ROADMAP.md](ROADMAP.md) says what that
would take, along with everything else ketch does not do yet.

## Documentation

| | |
| --- | --- |
| [docs/MANIFESTS.md](docs/MANIFESTS.md) | The package config: every field, and when you need one |
| [docs/REGISTRY.md](docs/REGISTRY.md) | The registry layout, and how to add a package to it |
| [docs/PLUGINS.md](docs/PLUGINS.md) | The source-plugin protocol, for sources other than GitHub |
| [docs/LOCKFILE.md](docs/LOCKFILE.md) | `ketch.lock`: pinning a machine's tools to exact releases |
| [ROADMAP.md](ROADMAP.md) | What is missing, and what is deliberately out of scope |
| [AGENTS.md](AGENTS.md) | The layout, the conventions and the trust boundaries |

The same pages are published at
**[listepo.github.io/ketch/docs](https://listepo.github.io/ketch/docs/)** — the
site generates them from the Markdown in this repository, so the two cannot
drift.

## Developing

ketch is a TypeScript monorepo: `packages/schemas` (the Zod schemas and the
JSON Schemas they generate), `packages/core` (the install pipeline and
everything under it), `apps/cli` (the command surface and all terminal output).

```bash
pnpm install
pnpm run typecheck        # tsc --build across the project references
pnpm run lint             # oxlint
pnpm run format           # biome format --write
pnpm run format:check     # the same, without writing
pnpm run test             # vitest, unit and end-to-end; no network
pnpm run check            # typecheck, lint, format:check, test — in that order
pnpm exec moon ci         # only what your change affects
```

Run the CLI straight from source against a throwaway tree instead of your real
`~/.ketch`:

```bash
KETCH_ROOT=/tmp/ketch-scratch node apps/cli/src/main.ts doctor
```

Runtimes: **Node 26** is canonical and what CI runs the suite on. **Bun 1.3** is
the fastest and what the day-to-day loop uses — never because something only
works there. **Deno** is supported and smoke-tested. **Perry** compiles the CLI
to the native binary that `install.sh` downloads. That is why the code sticks to
`node:` builtins and erasable TypeScript syntax: anything runtime-specific
breaks one of the four. `mise install` pins every one of them.

## Releasing

```bash
scripts/release.sh 0.2.0        # --dry-run to see it first
```

It opens a pull request bumping the version. Merge it, then tag the merge commit
— that is what compiles both macOS architectures and publishes the tarballs.
[AGENTS.md](AGENTS.md) has the procedure and the reasons behind it.

## Contributing

[AGENTS.md](AGENTS.md) documents the layout, the conventions, and the trust
boundaries — read it before changing anything. `pnpm run check` has to be clean,
and CI enforces the same gates on macOS.

To add a package to the registry, see [docs/REGISTRY.md](docs/REGISTRY.md).

## Licence

MIT — see [LICENSE](LICENSE).
