# Toolchain

Project programs and direct packages from manifests.

## Programs

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| mise | brew / curl, then `mise install` | Pinned tool versions | https://github.com/jdx/mise |
| cargo-cache | mise | `just cache` / `just cache-autoclean`; the shared cargo home fills up | https://github.com/matthiaskrgr/cargo-cache |
| cargo-nextest | global (cargo install) | Parallel test runner | https://github.com/nextest-rs/nextest |
| node | mise | commitlint for `just lint-commits` and the commit-msg hook, and the macOS design-token generator and checks; node-based checks live in Just and CI only | https://github.com/nodejs/node |
| rustc | mise (through rustup) | Rust compiler; `mise.toml` pins the one version local work, CI and release builds use | https://github.com/rust-lang/rust |
| cargo | with rustc | Rust build and dependencies | https://github.com/rust-lang/cargo |
| just | cargo install just / brew | Command recipes | https://github.com/casey/just |
| cargo-dist | mise | Generates `release.yml` and builds the release tarballs (`just dist-generate`, `just package`) | https://github.com/axodotdev/cargo-dist |
| git-cliff | mise | Writes the `CHANGELOG.md` entry from `cliff.toml` (`scripts/release.sh`), and the macOS app's release notes from `desktop/cliff.toml` | https://github.com/orhun/git-cliff |
| xcodegen | mise | Generates `desktop/macos/Ketch.xcodeproj` from `project.yml` (`just macos-app`, `just macos-test`) | https://github.com/yonaskolb/XcodeGen |
| Xcode | global (App Store / developer.apple.com) | Builds and tests the macOS app (`xcodebuild`); 26 or later | https://developer.apple.com/xcode/ |
| rust-std aarch64/x86_64-apple-darwin | rustup (`scripts/xcframework.sh` adds the missing one to the pinned toolchain) | Both architectures of `ketch-ffi`'s XCFramework (`just xcframework`) | https://github.com/rust-lang/rust |
| swift-format | with Xcode (`xcrun swift-format`) | Formats and lints the macOS app's Swift | https://github.com/swiftlang/swift-format |
| notarytool, stapler | with Xcode (`xcrun`) | Notarise and staple the macOS app and its `.dmg` (`desktop-release.yml`) | https://developer.apple.com/documentation/security/customizing-the-notarization-workflow |
| hdiutil, codesign, spctl | with macOS | Build the app's `.dmg` (`scripts/desktop-dmg.sh`), sign it, and assess what Gatekeeper will decide | https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution |
| generate_appcast | with the Sparkle package (`build/SourcePackages/artifacts/sparkle/Sparkle/bin/`) | Writes and signs the macOS app's appcast (`scripts/desktop-appcast.sh`) | https://github.com/sparkle-project/Sparkle |
| cargo-fuzz | mise | Builds and runs the libFuzzer targets in `fuzz/` (`just fuzz`) | https://github.com/rust-fuzz/cargo-fuzz |
| rustc nightly | rustup (`rustup toolchain install nightly`) | Only for `cargo +nightly fuzz`; every build stays on the `mise.toml` pin | https://github.com/rust-lang/rust |
| release-plz | local only | `release-plz update` preview; no longer run in CI (bump.yml releases) | https://github.com/release-plz/release-plz |
| ketch | see its README | Installs dunnage | https://github.com/pyrlyn/ketch |
| dunnage | `ketch` | Lossless cleanup of `target/` after tests | https://github.com/listepo/dunnage |
| mandoc | ships with macOS; `apt install mandoc` on Linux | `just lint-man` checks the generated man pages | https://mandoc.bsd.lv |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| arbitrary | local (`fuzz/`) | https://crates.io/crates/arbitrary | Structured fuzz input |
| assert_cmd | local | https://crates.io/crates/assert_cmd | Rust dependency |
| assert_fs | local | https://crates.io/crates/assert_fs | Rust dependency |
| bzip2 | local | https://crates.io/crates/bzip2 | Rust dependency |
| clap | local | https://crates.io/crates/clap | CLI |
| clap_complete | local | https://crates.io/crates/clap_complete | Rust dependency |
| clap_mangen | local | https://crates.io/crates/clap_mangen | ketch's man pages, one per command (`src/man.rs`) |
| console | local (Windows only) | https://crates.io/crates/console | Switches on virtual terminal processing so legacy conhost shows colour instead of escape codes; already in the tree through indicatif |
| crossterm | local | https://crates.io/crates/crossterm | Terminal |
| diesel | local | https://crates.io/crates/diesel | SQLite ORM |
| diesel_migrations | local | https://crates.io/crates/diesel_migrations | SQLite migrations |
| dirs | local | https://crates.io/crates/dirs | Rust dependency |
| dunce | local | https://crates.io/crates/dunce | Canonicalize without Windows UNC prefixes |
| flate2 | local | https://crates.io/crates/flate2 | Rust dependency |
| hex | local | https://crates.io/crates/hex | Rust dependency |
| indicatif | local | https://crates.io/crates/indicatif | Rust dependency |
| insta | local | https://crates.io/crates/insta | Reviewed snapshots |
| jsonschema | local | https://crates.io/crates/jsonschema | Tests that the manifests ketch ships validate against `docs/manifest.schema.json` |
| libsqlite3-sys | local | https://crates.io/crates/libsqlite3-sys | `bundled` compiles SQLite into the binary. Linking the system one would make ketch's single-binary promise depend on what the host happens to ship, and the release builds both macOS architectures where that answer differs. |
| libfuzzer-sys | local (`fuzz/`) | https://crates.io/crates/libfuzzer-sys | libFuzzer targets |
| lzma-rs | local | https://crates.io/crates/lzma-rs | Rust dependency |
| octocrab | local | https://crates.io/crates/octocrab | `ketch registry push` talks to GitHub through octocrab rather than the ureq client the sources use: it needs forks, refs, contents and pull requests, which octocrab already types and paginates. It is async, hence tokio for a runtime to block on; everything else in ketch stays synchronous. |
| pathdiff | local | https://crates.io/crates/pathdiff | Relative path between two paths |
| predicates | local | https://crates.io/crates/predicates | Rust dependency |
| pretty_assertions | local | https://crates.io/crates/pretty_assertions | Rust dependency |
| proptest | local | https://crates.io/crates/proptest | Property tests |
| ratatui | local | https://crates.io/crates/ratatui | TUI |
| rstest | local | https://crates.io/crates/rstest | Parameterized tests |
| schemars | local | https://crates.io/crates/schemars | JSON Schema of `config.toml`, `ketch.lock` and `ketch.toml`, checked by drift tests |
| semver | local | https://crates.io/crates/semver | Rust dependency |
| serde | local | https://crates.io/crates/serde | Serialization |
| serde_json | local | https://crates.io/crates/serde_json | JSON |
| sha2 | local | https://crates.io/crates/sha2 | Rust dependency |
| tar | local | https://crates.io/crates/tar | Rust dependency |
| tempfile | local | https://crates.io/crates/tempfile | Rust dependency |
| terminal_size | local | https://crates.io/crates/terminal_size | Terminal width for `ketch list remote` descriptions |
| unicode-width | local | https://github.com/unicode-rs/unicode-width | Column width of status-line emoji icons |
| thiserror | local | https://crates.io/crates/thiserror | Errors |
| tokio | local | https://crates.io/crates/tokio | Async runtime |
| uniffi | local (`crates/ketch-ffi`) | https://github.com/mozilla/uniffi-rs | Exports the core to Swift (and later other languages) as `ketch-ffi`; its `cli` feature is the in-tree `uniffi-bindgen` (`just xcframework`) |
| toml | local | https://crates.io/crates/toml | Config |
| toml_edit | local | https://crates.io/crates/toml_edit | Writing a chosen `bin` into a user manifest, keeping the rest of the file |
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
| style-dictionary | local | https://github.com/style-dictionary/style-dictionary | Generates the macOS app's `Tokens.swift`, the DESIGN.md front matter and the preview CSS from `desktop/macos/design/tokens.json` (`just design-tokens`) |
| @google/design.md | local | https://github.com/google-labs-code/design.md | Lints `desktop/macos/DESIGN.md` against the DESIGN.md format (`just design-check`) |

## SwiftPM

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| Sparkle | local (`desktop/macos/project.yml`, exact 2.10.0) | https://github.com/sparkle-project/Sparkle | The macOS app's own updates; its `generate_appcast` writes the release's appcast |
