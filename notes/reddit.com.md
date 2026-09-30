# Reddit

Platform: https://www.reddit.com (suggested: r/commandline, r/rust, r/selfhosted, r/linux)

## Post draft

Title:
I built ketch: install CLI tools straight from GitHub releases, no formula needed. Feedback welcome

Text:

Hey all. Most CLI tools already ship prebuilt binaries in their GitHub releases, but getting one onto your PATH still means waiting for a Homebrew formula or doing the download, unpack and chmod dance by hand. I got tired of that and wrote ketch.

    curl -fsSL https://raw.githubusercontent.com/pyrlyn/ketch/main/install.sh | bash
    ketch install listepo/AirTalk
    ketch why AirTalk        # shows which asset it picked and why
    ketch rollback AirTalk   # back to the previous version, no re-download

It's a single Rust binary for macOS, Linux and Windows. It checks the published SHA-256, keeps every version in its own directory, pins toolsets per project with `ketch lock` and `ketch sync`, and `ketch self uninstall` removes everything it ever touched.

v0.6.0 just came out: https://github.com/pyrlyn/ketch/releases/tag/v0.6.0
Repo: https://github.com/pyrlyn/ketch

It's triple-licensed: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license.

I'd really like to hear where it picks the wrong asset, or which repos it can't handle. Roast away.

github.com/pyrlyn/ketch — drafted casual Reddit post asking the community for feedback
