# Research: a desktop app on top of ketch's core

Question: how to build a desktop GUI (macOS, Linux, Windows) for ketch that
reuses the install pipeline — through FFI, or something better.

Status: research only. Nothing here is approved; the recommendation below is a
proposal for the creator. Registry facts were checked on 2026-09-30.

## Decision (creator, 2026-09-30)

A native UI on each platform: macOS first, Windows and Linux later. That
selects option B below with UniFFI and SwiftUI for macOS; option A's Tauri
recommendation is kept as the rejected alternative. The core preparation
(workspace split, reporter, decisions out of the pipeline, a tryable lock) is
the same either way. Tasks: R5–R9, F12, F13 in `plan.md`; Windows and Linux
in `roadmap.md`. The app shares the ketch root with the CLI: one state, one
lock, one store. It has a menu-bar extra and targets macOS 26 and later. Licence: the project's triple
licence; `Cargo.toml` now says `GPL-3.0-only OR LicenseRef-Ketch-Royalty-free-1.0
OR LicenseRef-Ketch-Commercial`.

## Summary

**Recommendation: no FFI.** Turn the repository into a Cargo workspace with a
`ketch-core` library crate, keep the CLI as a thin binary on top of it, and
write the desktop app in Rust so it depends on `ketch-core` like any other
crate. The UI toolkit of choice is **Tauri 2** (Slint as the alternative, see
below). A C ABI / UniFFI layer only pays off if the UI must be written in a
non-Rust language (SwiftUI, WinUI, Flutter), and it would cost three things
ketch cares about: `unsafe_code = "forbid"`, one UI instead of three, and a
single toolchain.

The real work is not the binding. It is untangling the core from the terminal:
the global `ui::` sink, interactive prompts inside the pipeline, and the
process lock shared with a running CLI.

## What the core looks like today (repository facts, `main` at f60d85e)

- Single binary crate, no `src/lib.rs`: every module is `mod x;` in
  `src/main.rs`. About 33k lines in `src/`.
- `Cargo.toml` sets `[lints.rust] unsafe_code = "forbid"`.
- Everything is synchronous (ureq). `tokio` with the `rt` feature exists only for
  `ketch registry push` (octocrab).
- Output coupling: the pipeline talks to the user through `ui::` directly —
  `install.rs` has 35 `ui::` calls, `self_update.rs` 27, `registry.rs` 8,
  `listing.rs` 7. `ui.rs` holds global state (`static TUI:
  Mutex<Option<Arc<tui::Controller>>>`) and `ui::log`/`log::record` go through it.
- Interaction coupling: `ui::confirm`, `ui::prompt`, `ui::prompt_required`
  read from the terminal; `install.rs` calls `confirm`. A GUI has no stdin.
- Prior art: the optional `tui` feature (`src/tui/`) already swaps the
  renderer by routing `ui::` lines into an event channel ("so the install
  pipeline can remain a normal, terminal-agnostic command pipeline"). That is
  the seam a GUI would widen: typed events instead of rendered strings.
- `state::Lock::acquire` is an exclusive process lock on the ketch root. A GUI
  and a CLI running at once must both respect it; the GUI must show "busy"
  rather than block its UI thread on it.
- Several query commands already have `--json` (`src/cli.rs`), which makes the
  sidecar option below possible without new code.

## Options

### A. Rust GUI linking `ketch-core` directly (recommended)

Workspace: `crates/ketch-core` (lib), `ketch` (bin: `cli.rs`, `cmd/`,
terminal `ui` renderer, `complete.rs`), `apps/desktop` (GUI). No ABI boundary,
no `unsafe`, same types (`model.rs`) on both sides, same tests.

Toolkit candidates:

| Toolkit | Latest (date) | License | Rendering | Notes | Source |
| --- | --- | --- | --- | --- | --- |
| Tauri 2 | 2.12.0 (2026-09-26), MSRV 1.90 | Apache-2.0 OR MIT | System WebView (WRY/TAO), UI in HTML/JS, Rust backend over IPC | Bundler (`.msi` via WiX, NSIS, dmg, deb, AppImage), signed updater (`tauri-plugin-updater` 2.13.1, signature mandatory), tray (`tray-icon` feature), sidecars | https://crates.io/api/v1/crates/tauri, https://v2.tauri.app/concept/architecture/, https://v2.tauri.app/distribute/windows-installer/, https://v2.tauri.app/plugin/updater/, https://v2.tauri.app/learn/system-tray/ |
| Slint | 1.18.1 (2026-09-21), MSRV 1.92 | GPL-3.0 OR Slint Royalty-free 2.0 OR commercial | Native toolkit, `.slint` markup compiled to Rust | No WebView; royalty-free license covers proprietary desktop apps; packaging via external tools | https://crates.io/api/v1/crates/slint, https://github.com/slint-ui/slint/blob/master/LICENSE.md |
| iced | 0.14.0 (2025-12-07) | MIT | Native (wgpu, tiny-skia fallback), Elm architecture | README calls it "experimental software" | https://crates.io/api/v1/crates/iced, https://github.com/iced-rs/iced |
| egui / eframe | 0.36.2 (2026-09-08), MSRV 1.95 | MIT OR Apache-2.0 | Immediate mode, wgpu/glow | Great for tools, weak for a polished consumer app look | https://crates.io/api/v1/crates/eframe |
| Dioxus desktop | 0.7.10 (2026-07-30) | MIT OR Apache-2.0 | WebView (wry/tao), components in Rust | WebView may be missing on Windows/Linux per its README | https://crates.io/api/v1/crates/dioxus, https://github.com/DioxusLabs/dioxus/tree/main/packages/desktop |
| gpui | 0.2.2 (2025-10-22) | Apache-2.0 | Native GPU | README: pre-1.0, frequent breaking changes; crates.io lags the Zed repo. Not a fit | https://crates.io/api/v1/crates/gpui |

Why Tauri first: it is the only candidate that ships bundling, signed updates,
tray and installers as first-party parts — the release half of a desktop app,
which ketch would otherwise build by hand next to cargo-dist. A package
manager's UI is lists, search, a changelog (Markdown) and progress: web UI
does that cheaply. Rust business logic stays in Rust; JS only renders.

Why Slint second: a native, lighter app with no WebView, and the Rust API is
generated from `.slint`, so no IPC layer to type. Costs: packaging, updates
and signing are ours (e.g. `cargo-packager` 0.11.8, last released
2025-11-27, https://crates.io/api/v1/crates/cargo-packager), and a smaller
widget ecosystem.

### B. FFI core + native UI per OS

`ketch-ffi` crate exporting the core through a binding generator:

| Tool | Latest (date) | License | Targets | Source |
| --- | --- | --- | --- | --- |
| UniFFI | 0.32.2 (2026-09-23) | MPL-2.0 | Swift, Kotlin, Python, Ruby (C#, Go third-party) | https://crates.io/api/v1/crates/uniffi, https://github.com/mozilla/uniffi-rs |
| flutter_rust_bridge | 2.13.0 (2026-08-23) | MIT | Dart / Flutter | https://crates.io/api/v1/crates/flutter_rust_bridge, https://pub.dev/packages/flutter_rust_bridge |
| diplomat | 0.16.1 (2026-08-20) | MIT OR Apache-2.0 | C, C++, JS; Dart, .NET, Kotlin via plugins | https://crates.io/api/v1/crates/diplomat |
| cbindgen | 0.29.4 (2026-06-09) | MPL-2.0 | C/C++ headers for hand-written `extern "C"` | https://crates.io/api/v1/crates/cbindgen |
| napi-rs | 3.13.0 (2026-09-22) | MIT | Node-API addon (Electron) | https://crates.io/api/v1/crates/napi |
| safer-ffi | 0.1.13 (2024-09-17) | MIT | C | Last release two years ago: treat as unmaintained. https://crates.io/api/v1/crates/safer-ffi |

Costs for ketch:

- The generated scaffolding is `extern "C"` code; it cannot live in a crate
  with `unsafe_code = "forbid"`, so it needs its own `ketch-ffi` crate with a
  relaxed lint — the first `unsafe` in the tree.
- Swift + WinUI + GTK means three UIs and three toolchains for a one-person
  project. Flutter (one UI) adds the Dart toolchain and its own desktop
  packaging; Electron + napi-rs ships Chromium.
- Errors, progress callbacks and cancellation have to be modelled twice: in
  Rust and in the binding's type system.

Worth it only if the goal is a genuinely native macOS app (SwiftUI, menu bar,
Sparkle) and Linux/Windows can wait or stay CLI-only.

### C. GUI over the CLI (sidecar / subprocess)

The GUI runs the installed `ketch` binary and parses `--json` output. Tauri
supports bundling it as a sidecar (`bundle.externalBin`,
https://v2.tauri.app/develop/sidecar/). Zero changes to the core and the
CLI stays the single source of truth, but progress, prompts and errors become
a text protocol, and every command a GUI needs must first grow `--json`.
Good as a prototype; poor as the long-term design.

## Recommended path

1. **Workspace split, no behaviour change.** Move modules into
   `crates/ketch-core` with `src/lib.rs`; the `ketch` binary keeps
   `main.rs`, `cli.rs`, `cmd/`, `complete.rs` and the terminal renderer.
   Public surface = what `cmd/` calls today. All existing tests must pass
   unchanged. MSRV 1.86 stays on `ketch-core` and `ketch`; the desktop crate
   gets its own (Tauri needs 1.90).
2. **Replace the global `ui::` sink with a reporter.** A `Reporter` trait (or
   an event enum + channel, generalising `tui::Event`): `progress`, `status`,
   `warn`, `log`. The CLI implements it with today's `ui.rs`; the TUI and the
   GUI implement it with events. `ui.rs` stays the only place that prints.
3. **Move decisions out of the pipeline.** `confirm`/`prompt` become either
   up-front options (`yes: bool`, chosen binary) or a `Decider` callback the
   frontend implements. The GUI answers with a dialog.
4. **Lock and threading.** Core calls run on a worker thread; the GUI never
   blocks on `state::Lock`; it reports "another ketch is running".
5. **Desktop crate** (Tauri 2): commands `list`, `search`, `install`,
   `upgrade`, `uninstall`, `changelog`, `doctor`, each a few lines over
   `ketch-core`, streaming reporter events to the front end.
6. **Release**: a separate workflow (Tauri bundler + updater keys, Developer
   ID signing reuses the existing certificate). cargo-dist keeps releasing the
   CLI.

Steps 1–3 help the CLI and the TUI on their own (testable core, no stdin in
the pipeline), so they are worth doing even if the desktop app is postponed.

## macOS app project generator (F12, checked 2026-10-01)

F12 wants the app target described in a text file so it is reviewable in
diffs, with the generated `.xcodeproj` left out of git.

| | XcodeGen | Tuist | Plain SwiftPM |
| --- | --- | --- | --- |
| Latest release | 2.46.0, 2026-07-16 ([release](https://github.com/yonaskolb/XcodeGen/releases/tag/2.46.0)) | 4.210.0, 2026-09-28 ([release](https://github.com/tuist/tuist/releases/tag/4.210.0)), with canary builds several times a day | ships with Xcode (Swift 6.4 in Xcode 27.0 here) |
| Licence | MIT ([repository](https://github.com/yonaskolb/XcodeGen)) | MIT, except `server/`, `kura/`, `atlas/` under MPL-2.0 ([LICENSE.md](https://github.com/tuist/tuist/blob/main/LICENSE.md)) | Apache-2.0 ([repository](https://github.com/swiftlang/swift-package-manager)) |
| Spec format | one YAML file ([ProjectSpec.md](https://github.com/yonaskolb/XcodeGen/blob/master/Docs/ProjectSpec.md)) | Swift manifests (`Project.swift`, `Tuist.swift`), compiled before generation | `Package.swift` |
| Scope | generates the project, nothing else | project generation plus a build cache, a server and hosted services ([README](https://github.com/tuist/tuist)) | builds packages, not app bundles |
| Install through mise | `aqua:yonaskolb/XcodeGen` (`mise registry xcodegen`) | `aqua:tuist/tuist` (`mise registry tuist`) | — |

Plain SwiftPM is out: `swift build` produces a bare executable, not a `.app`
bundle with an Info.plist, a login item (`SMAppService.mainApp` registers the
bundle) or the hosted unit and UI test bundles F12's tests need; getting there
means hand-assembling the bundle in a script, which is the project file again
without the tooling.

Tuist does more than F12 needs, and its manifests are Swift that has to
compile before the project exists; the last 100 GitHub releases span only
days and are almost all canaries (API, 2026-10-01), a fast-moving target for
a one-app repository.

**Choice: XcodeGen 2.46.0**, pinned in `mise.toml` as `xcodegen`: one YAML
file (`desktop/macos/project.yml`), a single-purpose tool, MIT, maintained
(release 2026-07-16, pushes on 2026-09-13 per the GitHub API). Recorded in
`toolchain.md`.

Toolchain checked on the development machine: Xcode 27.0 (27A266a), macOS
SDK 27.0, Swift 6.4, `xcodebuild -version`. The deployment target is macOS
26.0. GitHub's `macos-26` arm64 runner defaults to Xcode 26.6 (image
20260907.0351.1, [runner-images](https://github.com/actions/runner-images/blob/main/images/macos/macos-26-arm64-Readme.md)),
which CI uses.

## macOS app updates and releases (F13, checked 2026-10-01)

F13 releases the app from this repository under `desktop-v*` tags with
`make_latest: false` (creator, 2026-10-01), signed with the CLI's Developer ID
certificate and notarised. What remained open: how installed copies update,
where the update feed lives, and how the disk image is built.

### Update mechanism

| | Sparkle 2 | Squirrel.Mac | No in-app updater |
| --- | --- | --- | --- |
| Latest release | 2.10.0, 2026-09-13; 2.9.6, 2026-08-17; default branch pushed 2026-09-28 ([releases API](https://api.github.com/repos/sparkle-project/Sparkle/releases)) | 0.3.2, 2017-08-18 ([releases API](https://api.github.com/repos/Squirrel/Squirrel.Mac/releases/latest)) | — |
| Licence | MIT, with the copyright holders listed in [LICENSE](https://github.com/sparkle-project/Sparkle/blob/2.10.0/LICENSE) (GitHub reports `NOASSERTION` because of the multi-holder text) | MIT ([repository](https://github.com/Squirrel/Squirrel.Mac)) | — |
| Integrity | EdDSA (Ed25519) signature per archive (`SUPublicEDKey`), plus a signed feed with `SURequireSignedFeed` (Sparkle 2.9+) and `SUVerifyUpdateBeforeExtraction` (2.7.3+) ([customization](https://sparkle-project.org/documentation/customization/)) | code-signature match of the new bundle only | — |
| Feed | a static `appcast.xml`, written and signed by `generate_appcast` ([documentation](https://sparkle-project.org/documentation/)) | a JSON endpoint someone has to serve | — |
| SwiftUI | `SPUStandardUpdaterController` and a `CommandGroup(after: .appInfo)` item ([programmatic setup](https://sparkle-project.org/documentation/programmatic-setup/)) | none | — |
| Install | SwiftPM, `https://github.com/sparkle-project/Sparkle`; a binary XCFramework whose checksum is in `Package.swift`, `platforms: [.macOS(.v12)]` ([Package.swift at 2.10.0](https://github.com/sparkle-project/Sparkle/blob/2.10.0/Package.swift)) | Carthage/manual | — |

ketch itself cannot update the app either: it resolves `/releases/latest` for
this repository (`src/source/github.rs`), which by the decision above is
always the CLI.

**Choice: Sparkle 2.10.0**, pinned with `exactVersion` in
`desktop/macos/project.yml`. It is the only maintained option, it is MIT, and
its EdDSA signatures do not depend on the Developer ID certificate, which
expires. The release uses the `generate_appcast` that ships in the same
SwiftPM artifact (`SourcePackages/artifacts/sparkle/Sparkle/bin/`), so the
feed writer and the framework reading it are one version. From its source at
2.10.0 ([generate_appcast/main.swift](https://github.com/sparkle-project/Sparkle/blob/2.10.0/generate_appcast/main.swift)):
`--ed-key-file -` reads the private key from standard input, which is how the
workflow passes the `SPARKLE_ED_PRIVATE_KEY` secret; `--download-url-prefix`
sets the new item's URL; the existing `appcast.xml` in the archives directory
is read and its items kept ([Appcast.swift](https://github.com/sparkle-project/Sparkle/blob/2.10.0/generate_appcast/Appcast.swift));
the feed is signed when the app's Info.plist sets `SURequireSignedFeed`
([ArchiveItem.swift](https://github.com/sparkle-project/Sparkle/blob/2.10.0/generate_appcast/ArchiveItem.swift)).
The key format: `generate_keys -x` exports, for keys made by current versions,
the base64 32-byte Ed25519 seed ([generate_keys/main.swift](https://github.com/sparkle-project/Sparkle/blob/2.10.0/generate_keys/main.swift)).

Observed with 2.10.0 on this machine (2026-10-01, `tests/desktop-appcast.sh`):
when the private key is not the app's pair, `generate_appcast` prints a
warning, exits 0 and writes the item without `sparkle:edSignature`. Every
installed copy would then refuse the update, so the release verifies the
enclosure signature with the exported app's `SUPublicEDKey` through CryptoKit
(`scripts/desktop-appcast-verify.swift`) before anything is published.

### Where the feed lives

`SUFeedURL` must not go through `/releases/latest`, and must not change. Options:

- **A fixed `desktop-appcast` release whose `appcast.xml` asset every app
  release replaces** (chosen): `https://github.com/pyrlyn/ketch/releases/download/desktop-appcast/appcast.xml`.
  No other infrastructure, served from the same place as the `.dmg`, and the
  feed is signed, so the mutable asset is not trusted on its own. The release
  is a prerelease with `make_latest: false`: GitHub never makes a prerelease
  latest, and ketch's own listing skips prereleases when stable ones exist.
  It needs asset replacement, which [immutable releases](https://docs.github.com/en/code-security/supply-chain-security/understanding-your-software-supply-chain/immutable-releases)
  would forbid; the repository has them off (`GET /repos/pyrlyn/ketch/immutable-releases`
  → `enabled: false`, 2026-10-01). Turning them on means moving the feed.
- GitHub Pages (`https://pyrlyn.github.io/ketch/`, built by `pages.yml`): the
  site build would have to fetch the feed from the newest app release on every
  deploy, or a site deploy would drop it.
- A file committed to `main` by the workflow: a bot push to the protected
  branch for every release.

### Latest release

GitHub's [Create a release](https://docs.github.com/en/rest/releases/releases?apiVersion=2022-11-28#create-a-release)
(checked 2026-10-01): `make_latest` is `true`, `false` or `legacy`, "Defaults
to true for newly published releases", and "Drafts and prereleases cannot be
set as latest". `gh` 2.101.0: `gh release create --latest=false` "to
explicitly NOT set as latest"; without the flag it leaves the choice to
GitHub (`gh release create --help`). So `release.yml`'s `gh release create`
(no flag) makes each CLI release latest again, and the app workflow passes
`--latest=false` on every create and checks afterwards that `/releases/latest`
did not move.

The CLI's tag lookups, checked for `desktop-v*`:

- git-cliff: `tag_pattern` is "A regular expression for matching the git
  tags" ([docs](https://git-cliff.org/docs/configuration/git)); it matches
  anywhere in the name. With 2.13.1 and the old `v[0-9].*`, a fixture with a
  `desktop-v5.0.0` tag turned the next CLI entry into `## [desktop-v5.0.0]`
  (`tests/release-sh.sh`). Anchored to `^v[0-9].*`.
- release-plz: `git_only` finds releases by `git_tag_name`
  ([config docs](https://github.com/release-plz/release-plz/blob/release-plz-v0.3.169/website/docs/config.md)),
  turned into `^v(\d+\.\d+\.\d+)$`, anchored
  ([release_regex.rs](https://github.com/release-plz/release-plz/blob/release-plz-v0.3.169/crates/release_plz_core/src/release_regex.rs), release-plz v0.3.169).
- `tests/crate-version.sh` listed every tag; now only `v[0-9]*`.
- ketch's own release selection: when it lists releases instead of asking
  for `/releases/latest` (`ketch install pyrlyn/ketch --pre`, or prereleases
  switched on), `select_release` compared `desktop-v1.0.0`, which does not
  parse as a version, with `v0.8.1` as text (`natural_cmp` in `src/model.rs`),
  ranking the letter `d` above the digit `0`: the app release would have won
  and has no tarball. Tags that are not versions now lose to any that are
  (`src/source/mod.rs`).
- `scripts/release.sh`, pyrlyn/infra's `release-plz.yml` and `tap.yml` use
  exact `v<version>` tags; `sync-docs.yml` ran on any published release and
  would have set the site card's version to `desktop-v…`; it now skips tag
  refs that do not start with `v`.

### Disk image and notarisation

`hdiutil`, which ships with macOS, builds the `.dmg` (UDZO, with an
`/Applications` link); `create-dmg/create-dmg` 1.3.0 (2026-07-02) and
`sindresorhus/create-dmg` 8.1.0 (2026-03-21) (releases API: [create-dmg](https://api.github.com/repos/create-dmg/create-dmg/releases/latest), [sindresorhus](https://api.github.com/repos/sindresorhus/create-dmg/releases/latest))
add a styled window, which nothing here needs, at the cost of a dependency.
Apple's [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
(checked 2026-10-01): export with `xcodebuild -exportArchive
-exportOptionsPlist`; the notary service "accepts disk images (UDIF format),
signed flat installer packages, and ZIP archives" and processes nested
containers; `stapler` attaches a ticket to an app, bundle, disk image or flat
package but not to a ZIP; verify an image with `hdiutil verify` before
submitting. The workflow notarises the app (as a ZIP) and staples it, so a
copy dragged out of the image opens offline, then signs, notarises and
staples the image. `xcodebuild -help` (Xcode 27.0, 27A266a) lists
`developer-id` as the export method and `Developer ID Application` as an
automatic `signingCertificate` selector.

## Unverified

- Tauri's macOS notarization flow: the page
  https://v2.tauri.app/distribute/sign/macos/ exists, its content was not
  checked.
- No official bundle-size figures were found for any toolkit; sizes are not
  compared here for that reason.
- A signed, notarised app release has not run: the Developer ID export with
  Sparkle embedded, both notarisations and the `spctl` smoke test need the
  repository secrets and a `macos-26` runner. Everything up to signing
  (Release build with Sparkle, the `.dmg`, the appcast and its verification)
  ran locally on 2026-10-01.
- Which release-plz version `release-plz/action` v0.5.139 (pinned by
  pyrlyn/infra) runs was not checked; the anchoring above was read at
  release-plz v0.3.169.
- Whether GitHub recomputes `/releases/latest` when the current latest release
  is deleted, and whether it could then pick an app release, was not checked.
