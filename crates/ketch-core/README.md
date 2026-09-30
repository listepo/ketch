# ketch-core

The library the [`ketch`](../../README.md) binary is built on: everything ketch
does besides parsing its command line.

It owns resolving a name to a manifest across the four tiers, picking a release
and an asset, downloading and verifying it (checksums and publisher
signatures), unpacking it into the store, linking it onto `PATH`, and recording
what happened in `state.json`, `ketch.lock`, the log and `stats.db`. The OS
backends (`platform/`), the release sources and plugins (`source/`), the archive
formats (`extract/`), the embedded registry tier (`src/builtin.toml`) and the
`stats.db` migrations (`migrations/`) live here too.

It does not own the command line. `clap` definitions, command bodies, shell
completion scripts and ketch's own man pages stay in the binary at the
repository root; when the core needs something only the CLI can produce, the
binary passes it in (`extra::SelfDocs`).

Terminal output still goes through `ui`, which lives here for now, and so does
the opt-in full-screen renderer (`tui` feature) that `ui` drives.

The crate shares the workspace version, because it reports that version (user
agent, `ketch --version`, `stats.db`). It is `publish = false` and dist does
not package it: it ships only inside the `ketch` binary.

See [`AGENTS.md`](../../AGENTS.md) for the layout of each module, the
conventions and the trust boundaries.
