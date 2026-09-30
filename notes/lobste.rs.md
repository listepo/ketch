# Lobsters

Platform: https://lobste.rs/stories/new (tags: show, rust, unix)

## Submission draft

Title:
ketch: install CLI tools from GitHub releases with checksum verification and rollback

URL:
https://github.com/pyrlyn/ketch

Author comment:

I wrote this. ketch installs any tool that ships prebuilt binaries in GitHub releases, without a formula. It selects the asset for the current OS and architecture (`ketch why AirTalk` explains the choice), verifies the published SHA-256 and, when declared, sigstore, minisign or PGP signatures, and installs each version into its own directory so `ketch rollback AirTalk` is a relink.

    ketch install listepo/AirTalk

State lives under `~/.ketch`; `ketch lock` and `ketch sync` pin versions per project. Single Rust binary, macOS, Linux and Windows. v0.6.0: https://github.com/pyrlyn/ketch/releases/tag/v0.6.0

License is a triple license: GPLv3, a royalty-free license for proprietary desktop, mobile and web apps, or a commercial license.

I'd appreciate critique of the asset-selection heuristics and the verification model.

github.com/pyrlyn/ketch — drafted Lobsters submission with a technical author comment
