# ketch

Catch releases straight from GitHub.

ketch installs command-line tools and macOS apps directly from GitHub releases.
No taps, no formulae, no build step — it downloads what the project already
ships, verifies it, and puts it on your `PATH`.

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

## Contributing

[AGENTS.md](AGENTS.md) documents the layout, the conventions, and the trust
boundaries — read it before changing anything. `cargo test`, `cargo clippy
--all-targets` and `cargo fmt --check` all have to be clean.

To add a package to the registry, see [docs/REGISTRY.md](docs/REGISTRY.md).

## Licence

MIT
