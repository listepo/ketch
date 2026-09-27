---
title: ketch
tagline: Catch releases straight from GitHub — a single-binary package manager for command-line tools and apps on macOS, Linux, and Windows.
repo: https://github.com/listepo/ketch
homepage: https://listepo.github.io/ketch/
install: 'curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | bash'
install_alternatives:
  - 'irm https://raw.githubusercontent.com/listepo/ketch/main/install.ps1 | iex'
  - 'brew install --cask listepo/tap/ketch'
  - 'mise use -g github:listepo/ketch'
version: "0.6.0"
accent: "#3DDCB0"
---

<!-- Website copy for the listepo project site. The sync-docs workflow copies this file to
listepo/shop-mvp as content/projects/ketch.md on every change to main and on every v* tag.
Sources (checked 2026-09-27): README.md and the clap CLI in src/cli.rs; version from the latest
GitHub release (v0.6.0); accent is the dark-theme --accent in site/DESIGN.md. -->

## Overview

ketch installs command-line tools and apps from GitHub releases on macOS, Linux, and Windows.
No taps, no formulae, no build step — ketch downloads what a project already ships, verifies
it, and puts it on your `PATH`.

Most command-line tools are already published as a release asset built for your machine. ketch
picks the right one, checks the checksum the project published, unpacks it into a versioned
store, and links it onto your `PATH`.

## Features

- **Any repo that ships releases.** No formula, no tap, no waiting for a maintainer. Point it
  at `owner/repo`, a name from the registry, or an exact version.
- **Verified, not just downloaded.** Published SHA-256 sums are checked against what landed on
  disk. `require_checksums` refuses anything that publishes none, and ketch's own updates never
  accept trust-on-first-use.
- **Apps as well as binaries.** An `.app` bundle goes to `/Applications`, quarantine cleared
  when the signature checks out, removed cleanly on uninstall.
- **One tree.** Everything lives under `~/.ketch`: versioned payloads, links, state. Uninstalling
  leaves nothing behind.
- **Upgrades you can undo.** An upgrade keeps the previous version on disk; `ketch rollback`
  relinks it without re-downloading.
- **Reproducible machines.** `ketch lock` writes `ketch.lock` from what is installed and
  `ketch sync` installs exactly those versions elsewhere.
- **Sources beyond GitHub.** A plugin is one executable that answers in JSON — install from
  GitLab, Gitea or an internal artifact server with no recompile.
- **Local installs.** `ketch install --path` takes a binary, archive, symlink or `.app` that is
  already on disk.

## Install

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | bash
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/listepo/ketch/main/install.ps1 | iex
```

Homebrew, or mise:

```bash
brew install --cask listepo/tap/ketch
mise use -g github:listepo/ketch && ketch path install
```

Then make sure `~/.ketch/bin` is on your `PATH`; `ketch doctor` tells you if it is not.

## Usage examples

Install from any repository that publishes releases, by registry name, or at an exact version:

```bash
ketch install BurntSushi/ripgrep
ketch install rg
ketch install sharkdp/fd@v10.2.0
ketch install --path ./target/release/mytool --name mytool
```

Keep installed tools current, and step back if an upgrade misbehaves:

```bash
ketch list
ketch outdated
ketch upgrade
ketch rollback <pkg>
```

Understand a package before installing it:

```bash
ketch info <pkg> --assets
ketch why <pkg>
ketch search <query>
ketch changelog <pkg>
```

Reproduce one machine's tools on another:

```bash
ketch lock
ketch sync
```

Look after ketch itself:

```bash
ketch doctor --fix
ketch self upgrade
ketch self uninstall
```

## Links

- Repository: <https://github.com/listepo/ketch>
- Website and documentation: <https://listepo.github.io/ketch/>
- Commands reference: <https://listepo.github.io/ketch/docs/commands/>
- Package registry: <https://github.com/listepo/ketch-registry>
- Releases: <https://github.com/listepo/ketch/releases>
- License: MIT (<https://github.com/listepo/ketch/blob/main/LICENSE>)
