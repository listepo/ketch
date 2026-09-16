# Toolchain

Project programs and direct packages from manifests.

## Programs

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| mise | brew / curl, then `mise install` | Pinned tool versions | https://github.com/jdx/mise |
| cargo-cache | mise | `just cache` / `just cache-autoclean`; the shared cargo home fills up | https://github.com/matthiaskrgr/cargo-cache |
| cargo-nextest | global (cargo install) | Parallel test runner | https://github.com/nextest-rs/nextest |
| node | mise | commitlint for `just lint-commits` and the commit-msg hook; node-based checks live in Just and CI only | https://github.com/nodejs/node |
| rustc | rustup / system | Rust compiler | https://github.com/rust-lang/rust |
| cargo | with rustc | Rust build and dependencies | https://github.com/rust-lang/cargo |
| just | cargo install just / brew | Command recipes | https://github.com/casey/just |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| assert_cmd | local | https://crates.io/crates/assert_cmd | Rust dependency |
| assert_fs | local | https://crates.io/crates/assert_fs | Rust dependency |
| bzip2 | local | https://crates.io/crates/bzip2 | Rust dependency |
| clap | local | https://crates.io/crates/clap | CLI |
| clap_complete | local | https://crates.io/crates/clap_complete | Rust dependency |
| crossterm | local | https://crates.io/crates/crossterm | Terminal |
| diesel | local | https://crates.io/crates/diesel | SQLite ORM |
| diesel_migrations | local | https://crates.io/crates/diesel_migrations | SQLite migrations |
| dirs | local | https://crates.io/crates/dirs | Rust dependency |
| dunce | local | https://crates.io/crates/dunce | Canonicalize without Windows UNC prefixes |
| flate2 | local | https://crates.io/crates/flate2 | Rust dependency |
| hex | local | https://crates.io/crates/hex | Rust dependency |
| indicatif | local | https://crates.io/crates/indicatif | Rust dependency |
| insta | local | https://crates.io/crates/insta | Reviewed snapshots |
| libsqlite3-sys | local | https://crates.io/crates/libsqlite3-sys | `bundled` compiles SQLite into the binary. Linking the system one would make ketch's single-binary promise depend on what the host happens to ship, and the release builds both macOS architectures where that answer differs. |
| lzma-rs | local | https://crates.io/crates/lzma-rs | Rust dependency |
| octocrab | local | https://crates.io/crates/octocrab | `ketch registry push` talks to GitHub through octocrab rather than the ureq client the sources use: it needs forks, refs, contents and pull requests, which octocrab already types and paginates. It is async, hence tokio for a runtime to block on; everything else in ketch stays synchronous. |
| pathdiff | local | https://crates.io/crates/pathdiff | Relative path between two paths |
| predicates | local | https://crates.io/crates/predicates | Rust dependency |
| pretty_assertions | local | https://crates.io/crates/pretty_assertions | Rust dependency |
| proptest | local | https://crates.io/crates/proptest | Property tests |
| ratatui | local | https://crates.io/crates/ratatui | TUI |
| rstest | local | https://crates.io/crates/rstest | Parameterized tests |
| semver | local | https://crates.io/crates/semver | Rust dependency |
| serde | local | https://crates.io/crates/serde | Serialization |
| serde_json | local | https://crates.io/crates/serde_json | JSON |
| sha2 | local | https://crates.io/crates/sha2 | Rust dependency |
| tar | local | https://crates.io/crates/tar | Rust dependency |
| tempfile | local | https://crates.io/crates/tempfile | Rust dependency |
| thiserror | local | https://crates.io/crates/thiserror | Errors |
| tokio | local | https://crates.io/crates/tokio | Async runtime |
| toml | local | https://crates.io/crates/toml | Config |
| trycmd | local | https://crates.io/crates/trycmd | Rust dependency |
| typed-path | local | https://crates.io/crates/typed-path | Cross-platform path types for archive members |
| ureq | local | https://crates.io/crates/ureq | Rust dependency |
| walkdir | local | https://crates.io/crates/walkdir | Rust dependency |
| zip | local | https://crates.io/crates/zip | Rust dependency |

## npm / pnpm

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| @commitlint/cli | local | https://www.npmjs.com/package/@commitlint/cli | Commit messages |
| @commitlint/config-conventional | local | https://www.npmjs.com/package/@commitlint/config-conventional | Commit rules |
