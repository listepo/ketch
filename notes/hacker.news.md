# Hacker News

Platform: https://news.ycombinator.com/submit (post as Show HN)

## Show HN draft

Title:
Show HN: Ketch – install CLI tools straight from GitHub releases, no formulas

URL:
https://github.com/pyrlyn/ketch

Text:

Most command-line tools already publish a prebuilt binary for your OS in their GitHub releases. Getting one onto your PATH still usually means waiting for someone to write a Homebrew formula, or downloading, unpacking and chmod-ing it by hand. Ketch does that last part for you: `ketch install owner/repo` works for any repo that ships releases.

It's a single Rust binary for macOS, Linux and Windows. What it does:

- Picks the right asset for your platform and shows its reasoning (`ketch why AirTalk`, `ketch info --assets AirTalk`).
- Checks the published SHA-256 against what landed on disk. `require_checksums` refuses anything that publishes no sums, and publisher signatures (sigstore, minisign, PGP) are verified when a package declares them.
- Installs each version into its own directory in a versioned store and links it onto PATH. Upgrades keep the previous version, so `ketch rollback AirTalk` just relinks it, with no re-download.
- `ketch lock` / `ketch sync` pin a toolset per project; `pin`, `outdated` and `upgrade` do what you'd expect.
- Everything lives under `~/.ketch`. `ketch self uninstall` lists exactly what it will remove, asks, then removes all of it, including the PATH lines it added.
- Installs `.app` bundles into /Applications on macOS, and supports sources beyond GitHub through plugins (one executable that answers in JSON).

Quick try:

    curl -fsSL https://raw.githubusercontent.com/pyrlyn/ketch/main/install.sh | bash
    ketch install listepo/AirTalk
    ketch why AirTalk

v0.6.0 just shipped: https://github.com/pyrlyn/ketch/releases/tag/v0.6.0

It's triple-licensed: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license. I'd especially like feedback on how it picks assets for repos that name their files oddly.

github.com/pyrlyn/ketch — drafted Show HN post for ketch v0.6.0, triple license noted
