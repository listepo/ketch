<div align="center">

<img src="site/static/img/favicon.svg" width="72" alt="">

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
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | sh
```

Then make sure `~/.ketch/bin` is on your `PATH`. `ketch doctor` will tell you if
it is not, along with anything else that needs attention.

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
ketch install <pkg>...     # install; --pre for prereleases, @version to pin one
ketch list                 # what is installed
ketch outdated             # what has a newer release
ketch upgrade              # bring everything unpinned up to date
ketch info <pkg>           # details, including --assets and why each scored
ketch search <query>       # the registry and GitHub
ketch update               # refresh the package registry
ketch pin / unpin <pkg>    # hold a version, or let go
ketch uninstall <pkg>...   # remove it
ketch doctor               # check the environment and the install tree
```

Everything lives under `~/.ketch`: versioned payloads in `store/`, links in
`bin/`, and a `state.json` recording what is installed. Nothing is written
outside that tree except the `.app` bundles that belong in `/Applications`.

## How a package is found

A name is resolved against four tiers, in order:

1. your own manifests in `~/.ketch/manifests/`
2. the fetched package registry — see [docs/REGISTRY.md](docs/REGISTRY.md)
3. the registry compiled into the binary, so common tools work offline
4. inference from `owner/repo`, which is what makes an uncurated repository
   installable with no manifest at all

Inference picks the release asset that matches your machine — architecture,
OS, and libc — and explains its choice under `ketch info --assets`.

When it guesses wrong, a manifest says what to do instead: which asset, which
binaries, under what names. See [docs/MANIFESTS.md](docs/MANIFESTS.md).

Sources other than GitHub are added as plugins — a single executable, no ketch
release required. See [docs/PLUGINS.md](docs/PLUGINS.md).

## Configuration

`~/.ketch/config.toml`, with environment variables taking precedence:

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

The root itself is `KETCH_ROOT` or `--root`; it cannot be set from the config
file, because the file lives inside it.

A token is not required, but it raises GitHub's rate limit considerably.

## Platform support

macOS only for now. The OS-specific parts sit behind one `Platform` trait, so
a Linux or Windows backend means implementing that trait and nothing above it.
[ROADMAP.md](ROADMAP.md) says what that would take, along with everything else
ketch does not do yet.

## Documentation

| | |
| --- | --- |
| [docs/MANIFESTS.md](docs/MANIFESTS.md) | The package config: every field, and when you need one |
| [docs/REGISTRY.md](docs/REGISTRY.md) | The registry layout, and how to add a package to it |
| [docs/PLUGINS.md](docs/PLUGINS.md) | The source-plugin protocol, for sources other than GitHub |
| [ROADMAP.md](ROADMAP.md) | What is missing, and what is deliberately out of scope |
| [AGENTS.md](AGENTS.md) | The layout, the conventions and the trust boundaries |

The same pages are published at
**[listepo.github.io/ketch/docs](https://listepo.github.io/ketch/docs/)** — the
site generates them from the Markdown in this repository, so the two cannot
drift.

## Building from source

```bash
cargo build --release          # target/release/ketch
cargo test                     # unit tests and the end-to-end suite; no network
cargo clippy --all-targets     # must be clean
cargo fmt --check              # must be clean
```

Run the binary against a throwaway tree instead of your real `~/.ketch`:

```bash
KETCH_ROOT=/tmp/ketch-scratch cargo run -- doctor
```

## Contributing

[AGENTS.md](AGENTS.md) documents the layout, the conventions, and the trust
boundaries — read it before changing anything. `cargo test`, `cargo clippy
--all-targets` and `cargo fmt --check` all have to be clean, and CI enforces all
three on macOS.

To add a package to the registry, see [docs/REGISTRY.md](docs/REGISTRY.md).

## Licence

MIT — see [LICENSE](LICENSE).
