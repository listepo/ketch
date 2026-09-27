# Hashnode

Platform: https://hashnode.com/draft (tags: Rust, CLI, Open Source, DevOps)

## Article draft

Title:
Why I stopped writing Homebrew formulas and built ketch

Subtitle:
A cross-platform installer that takes binaries straight from GitHub releases, verifies them and rolls back in one command.

Body:

Every tool I use already has a release page with binaries for macOS, Linux and Windows. The missing piece was never the build. It was the last mile: choosing the right asset, checking it and putting it somewhere sensible. So I wrote ketch.

```bash
curl -fsSL https://raw.githubusercontent.com/listepo/ketch/main/install.sh | bash
ketch install listepo/AirTalk
ketch why AirTalk
```

A few design decisions I care about:

- **Verification by default.** ketch checks the published SHA-256, can refuse releases with no checksums (`require_checksums`), and verifies sigstore, minisign or PGP signatures when a package declares them.
- **A versioned store.** Each version is installed into its own directory and linked onto PATH, so `ketch rollback AirTalk` is instant.
- **Reproducible machines.** `ketch lock` and `ketch sync` pin a toolset per project.
- **Clean removal.** Everything lives under `~/.ketch`, and `ketch self uninstall` removes it all after showing you the list.

v0.6.0 release notes: https://github.com/listepo/ketch/releases/tag/v0.6.0
Source: https://github.com/listepo/ketch

ketch is triple-licensed: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license.

github.com/listepo/ketch — drafted Hashnode article on design decisions behind ketch v0.6.0
