---
title: "Documentation"
description: "How ketch resolves a name, picks a release asset and verifies it — plus the manifest schema, the registry layout and the source-plugin protocol."
---

ketch installs command-line tools and macOS apps directly from GitHub releases.
Most of the time it needs nothing from you but a repository name — these pages
cover the cases where it does, and the formats you would write when it does.

## Installing

```bash
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | sh
```

Then put `~/.ketch/bin` on your `PATH`. `ketch doctor` reports whether it is,
along with anything else that needs attention.

## Everyday use

```bash
ketch install BurntSushi/ripgrep     # any repo that publishes releases
ketch install rg                     # or a name the registry knows
ketch install sharkdp/fd@v10.2.0     # or an exact version
ketch upgrade                        # everything unpinned
ketch uninstall rg                   # and away again
```

Everything lives under `~/.ketch`: versioned payloads in `store/`, links in
`bin/`, and a `state.json` recording what is installed. Nothing is written
outside that tree except `.app` bundles, which belong in `/Applications`.

## How a name is resolved

Four tiers, in order — the first that answers wins:

1. your own manifests in `~/.ketch/manifests/`
2. the fetched package registry
3. the registry compiled into the binary, so common tools work offline
4. inference from `owner/repo`

The last tier is the point of ketch: an uncurated repository is installable with
no manifest at all. Inference picks the release asset matching your machine —
architecture, OS and libc — and `ketch info <pkg> --assets` shows every
candidate with the score it got and why.
