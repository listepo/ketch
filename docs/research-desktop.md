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
lock, one store. It has a menu-bar extra and targets macOS 26 and later. The UI is frosted or
glossy glass with a 3D effect: macOS 26's Liquid Glass material, with depth
from layering, shadows and highlights (creator, 2026-10-01). Licence: the project's triple
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

## Unverified

- Tauri's macOS notarization flow: the page
  https://v2.tauri.app/distribute/sign/macos/ exists, its content was not
  checked.
- No official bundle-size figures were found for any toolkit; sizes are not
  compared here for that reason.
