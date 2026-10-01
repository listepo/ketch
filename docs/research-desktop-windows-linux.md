# Research: toolkits for the Windows and Linux desktop apps

Task R10. Sources checked 2026-10-01 unless a date says otherwise. This is a
companion to the desktop research (`docs/research-desktop.md`), kept in a
separate file like `docs/research-design-system.md`: that page records choices
already made for the macOS app (F12, F13), while this one is an open proposal
for a roadmap item, so the two can be reviewed and revised independently.

Status: research only. Nothing here is approved; the recommendations are
proposals and the open decisions at the end belong to the creator.

The starting point is the creator's decision of 2026-09-30
(`docs/research-desktop.md`, "Decision"; `ROADMAP.md`, "Native desktop apps
for Windows and Linux"): a native UI on each OS over the same core; Windows
reuses R9's UniFFI binding from a native front end; Linux may link
`ketch-core` directly from a Rust toolkit native to the desktop.

## What a toolkit has to fit (repository facts)

- **Core rules** (`AGENTS.md`, "Conventions"). A mutating operation holds
  `state::Lock` for its whole run. The lock never waits, and a second acquire
  in the *same process* fails with `Error::Busy` too, so a GUI has to run
  mutating operations one at a time and treat `Busy` as "another ketch is
  working", not retry in a loop. `cancel::Cancel` is a cloneable flag the host
  keeps one clone of. `Config` and `log::init` are built per operation. And
  `push.rs` owns a tokio runtime and blocks on it, so it must be called from a
  plain worker thread: tokio's `Runtime::block_on` "panics … if called within
  an asynchronous execution context" (https://docs.rs/tokio/1.53.1/tokio/runtime/struct.Runtime.html,
  tokio 1.53.1 in `Cargo.lock`).
- **The binding R9 plans** (`plan.md`, R9): `ketch-ffi` in UniFFI proc-macro
  mode, `crate-type = ["lib", "staticlib"]`, UniFFI 0.32.2, a callback
  interface for the reporter and decider, a `CancelToken` object, a typed
  `KetchError`. Sync methods that the front end calls off its UI thread.
- **Licence.** `Cargo.toml`: `GPL-3.0-only OR LicenseRef-Ketch-Royalty-free-1.0
  OR LicenseRef-Ketch-Commercial`. F12 ships the app under the same three
  (`plan.md`, F12). So a toolkit has to allow two things: distribution of the
  app under GPL-3.0-only, and distribution of a proprietary app by someone
  using ketch under the royalty-free or commercial licence. MIT, Apache-2.0
  and MPL-2.0 allow both. LGPL allows both when it is linked dynamically. A
  GPL-only dependency allows only the first, and a proprietary runtime raises
  a question for the first. That question is flagged below, not answered:
  this is not legal advice.
- **What ketch writes outside its root** (`AGENTS.md`, "Layout"). It writes
  `~/.ketch`, and on Windows it also writes `HKCU\Environment\Path` through
  PowerShell (`crates/ketch-core/src/shell.rs`). It edits shell startup files
  and fills user man and completion directories. User-tier `[hooks]` run
  shell commands. Since a GUI runs the core in process, all of this happens
  inside whatever sandbox or virtualization the app's package puts it in.
- **macOS parity.** F12 has a menu-bar extra. On Windows the equivalent is a
  notification-area icon; on Linux it is a StatusNotifierItem, or nothing.

## Windows

### Candidates

| Toolkit | Latest (date) | Licence | Native look and accessibility | How it reaches the core | Source |
| --- | --- | --- | --- | --- | --- |
| WinUI 3 (Windows App SDK), C# | Windows App SDK 2.5.1 stable (2026-09-16), 1.8.12 servicing (2026-09-24); .NET 10.0.12 LTS (2026-09-08, end of support 2028-11-14) | Source MIT (`microsoft/WindowsAppSDK`, `microsoft/microsoft-ui-xaml`); the NuGet package ships under the Microsoft Software License Terms (see "Licence of the runtime") | Microsoft's own: "For a new native Windows desktop app, use WinUI 3 with the Windows App SDK". Accessibility through UI Automation peers built into the controls | R9's UniFFI binding through `uniffi-bindgen-cs` (below) | https://api.nuget.org/v3/registration5-gz-semver2/microsoft.windowsappsdk/index.json, https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/stable-channel (page dated 2026-09-29), https://learn.microsoft.com/en-us/windows/apps/get-started/ (2026-09-10), https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility-overview (2026-08-21), https://builds.dotnet.microsoft.com/dotnet/release-metadata/releases-index.json |
| WPF, C# | 10.0.12 (2026-09-08) | MIT (`dotnet/wpf`), runtime is .NET (MIT) | Fluent theme since .NET 9; .NET 10 notes say "Fluent UI style support is still in progress", and setting `ThemeMode` in code is an experimental API (`WPF0001`). Learn tells existing WPF apps they need not rewrite, and new apps to use WinUI 3 | Same binding as WinUI | https://github.com/dotnet/wpf/releases, https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90, https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net100 (both dated 2026-02-10), https://learn.microsoft.com/en-us/windows/apps/get-started/ |
| `windows-reactor` (WinUI 3 from Rust) | 0.100.0 (2026-09-03; a 0.0.0 placeholder 2026-04-27), MSRV 1.95 | MIT OR Apache-2.0 | Real WinUI 3 controls ("Reactor reconciles each new view with the native UI tree"). Accessibility properties are set on controls; not tested here | Links `ketch-core` directly: no binding | https://crates.io/api/v1/crates/windows-reactor, https://github.com/microsoft/windows-rs/blob/master/docs/crates/windows-reactor.md, https://github.com/microsoft/windows-rs/blob/master/crates/libs/reactor/readme.md |
| Slint | 1.18.1 (2026-09-21), MSRV 1.92 | GPL-3.0-only OR Slint Royalty-free 2.0 OR Slint commercial | Draws its own widgets; the `native` style is `fluent` on Windows, an emulation of Fluent. AccessKit behind the winit backend's `accessibility` feature | Directly | https://crates.io/api/v1/crates/slint, https://github.com/slint-ui/slint/blob/master/LICENSE.md, https://github.com/slint-ui/slint/blob/master/docs/astro/src/content/docs/reference/std-widgets/style.mdx, https://github.com/slint-ui/slint/blob/master/internal/backends/winit/Cargo.toml |
| iced | 0.14.0 (2025-12-07), MSRV 1.88 | MIT | Own look. README: "Iced is currently experimental software". No AccessKit dependency in its `Cargo.toml` at `master` | Directly | https://crates.io/api/v1/crates/iced, https://github.com/iced-rs/iced |
| egui / eframe | 0.36.2 (2026-09-08), MSRV 1.95 | MIT OR Apache-2.0 | Own immediate-mode look; AccessKit is a default eframe feature | Directly | https://crates.io/api/v1/crates/eframe, https://github.com/emilk/egui/blob/main/crates/eframe/Cargo.toml |
| Avalonia, C# (contrast) | 12.1.3 (2026-09-22) | MIT | Draws its own widgets; one C# UI could cover Windows and Linux over the same binding. Accessibility not checked | Same binding as WinUI | https://www.nuget.org/packages/Avalonia (registration API), https://github.com/AvaloniaUI/Avalonia/releases |

The Rust route used to be closed. `microsoft/windows-app-rs` was archived in
2022 with the note that the Windows App SDK "is too heavily tied to .NET and
Visual Studio to be practically usable with other languages and toolchains"
(https://github.com/microsoft/windows-app-rs, README). `windows-reactor` is
Microsoft's new answer from the same `windows-rs` repository (release 74,
2026-09-03). It does a lot already: components, typed messages,
`spawn_background` for blocking work, and an `AppProxy` that is `Send + Sync`
for posting back to the UI thread. It deploys framework-dependent or
self-contained (`windows-reactor-setup`), and its repository has a
notification-icon sample. But it has had one release, a month ago, and the
notification-icon crate on crates.io is a 0.0.0 placeholder (2026-09-11),
so the tray part is repository-only for now. The reactor guide's own
modal-dialog example uses an `unsafe` block.

### C# over R9's binding

| Generator | Latest (date) | Licence | Fit | Source |
| --- | --- | --- | --- | --- |
| `uniffi-bindgen-cs` (NordSecurity) | v0.11.0+v0.31.0 (2026-06-23): targets UniFFI 0.31.0 | MPL-2.0 | Library mode (`--library`), callbacks, async callbacks, errors and futures have tests in its `dotnet-tests`. Needs .NET 8+ and `AllowUnsafeBlocks`. Installed with `cargo install --git … --tag`, not from crates.io. Calls the native library through `LibraryImport`/`DllImport` by `cdylib_name` | https://github.com/NordSecurity/uniffi-bindgen-cs (README, `bindgen/templates/NamespaceLibraryTemplate.cs`, `dotnet-tests/UniffiCS.BindingTests`) |
| csbindgen | 1.9.8 (2026-05-20) | MIT | C# `DllImport` from `extern "C"` functions: a hand-written C ABI, not R9's UniFFI surface | https://crates.io/api/v1/crates/csbindgen |
| interoptopus | 0.16.5 (2026-09-15) | MIT | Its own FFI description with a C# backend: a second binding next to R9's | https://crates.io/api/v1/crates/interoptopus |

UniFFI itself is at 0.32.2 (2026-09-23); 0.32.0 came out 2026-06-30 and the
last 0.31 is 0.31.2 (2026-06-17) (https://crates.io/api/v1/crates/uniffi). The
C# generator has had no release in the three months since 0.32. Its last
merge was 2026-06-23, and five pull requests opened between 2026-08-05 and
2026-09-14 are still open (GitHub API). The generator README says a consumer
should keep every external generator on the same UniFFI version. So R9 at
0.32.2 and a C# front end do not combine today. Two concrete consequences
for R9, if Windows goes this way:

- UniFFI 0.31.x instead of 0.32.2, or a wait for the generator to catch up.
- `crate-type` gains `cdylib`: C# loads a DLL, and R9 only builds `lib` and
  `staticlib` for the XCFramework.

### Threading fit

Every candidate has a place for the core's rules:

- **WinUI 3 in C#.** The sync UniFFI call runs on a worker task. Reporter
  callbacks arrive on that Rust thread and hop to the UI through
  `DispatcherQueue.TryEnqueue`
  (https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.ui.dispatching.dispatcherqueue).
  The app keeps R9's `CancelToken` for its Cancel button.
- **`windows-reactor`.** `spawn_background` closures must capture only `Send`
  data and return a message when they finish. That is enough for a whole
  operation, but not for progress, which needs the reporter to post through
  an `AppProxy` clone (reactor guide, "Move slow work off the UI thread",
  "Application lifetime"). `Cancel` is cloned straight into the closure.
- **Slint, iced, egui.** Same pattern with their own hop to the UI thread,
  e.g. `slint::invoke_from_event_loop`
  (https://docs.rs/slint/1.18.1/slint/fn.invoke_from_event_loop.html).

In every case the app keeps a single queue of mutating operations, because a
second one in the same process gets `Busy`.

### Packaging

- **MSIX virtualizes what ketch writes.** For a packaged desktop app, "All
  writes under HKCU are copied on write to a private per-user, per-app
  location", and new files under `AppData` go to a private per-app location
  (https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes,
  dated 2025-09-09). `ketch path install` writes `HKCU\Environment\Path`, so
  inside a virtualized MSIX the CLI and new terminals would never see it.
  Opting out takes `desktop6:RegistryWriteVirtualization` /
  `FileSystemWriteVirtualization` set to `disabled`, which requires the
  `unvirtualizedResources` *restricted* capability; Windows 11 adds a
  per-location list, limited to `%USERPROFILE%\AppData` for files
  (https://learn.microsoft.com/en-us/windows/msix/desktop/flexible-virtualization).
  `~/.ketch` itself is outside `AppData`.
- **Unpackaged is supported.** The Windows App SDK deploys
  framework-dependent (its runtime installer) or self-contained, packaged or
  unpackaged
  (https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/deploy-overview,
  dated 2026-09-10). `windows-reactor` supports both deployment modes.
- **winget** takes `msix`, `msi`, `appx`, `exe`, `zip`, `inno`, `nullsoft`,
  `wix`, `burn`, `pwa`, `portable` and `font` installers (manifest schema
  1.12.0, https://github.com/microsoft/winget-cli/blob/master/schemas/JSON/manifests/v1.12.0/manifest.installer.1.12.0.json;
  winget-cli v1.29.380, 2026-09-21). So an unpackaged app in a `zip` or an
  `exe` installer is listed as easily as an MSIX.
- **ketch itself** skips `.msi` and other installer assets when it picks a
  release asset (`crates/ketch-core/src/platform/mod.rs`), so `ketch install`
  cannot install the app the way it installs the CLI unless the app ships as
  an archive.
- **Toolchain without a system install.** `mise` has `dotnet` in its registry
  (`core:dotnet`, mise 2026.9.11), so the .NET SDK can be pinned in
  `mise.toml` like everything else.

### Licence of the runtime

The Windows App SDK NuGet package is under the Microsoft Software License
Terms (https://www.nuget.org/packages/Microsoft.WindowsAppSDK/2.5.1/license).
Files it places next to the app may be redistributed, for both
framework-dependent and self-contained deployment. The redistributor must
"require distributors and external end users to agree to terms that protect
it and Microsoft at least as much as this agreement". The redistributor must
also not distribute it "so that any part of it becomes subject to any license
that requires that the distributable code … be disclosed or distributed in
source code form". How that combines with a GPL-3.0-only distribution of the
app, and whether the framework-dependent runtime counts as a GPL "System
Library", is a legal question for the creator. A framework-dependent app does
not carry the runtime itself. WPF and Avalonia are MIT end to end and do not
raise this.

### Effort

- **C# + WinUI 3 over R9.** A third language and toolchain (.NET), a C#
  binding build next to the XCFramework, and the app itself. About F12's size
  plus the binding work.
- **`windows-reactor`.** One language and no binding, so less glue. But it
  bets on a library with one release, and gaps it has today become ketch's to
  work around.

### Recommendation for Windows

**WinUI 3 on the Windows App SDK (2.5 stable channel), in C# on .NET 10 LTS,
over R9's UniFFI binding through `uniffi-bindgen-cs`, distributed unpackaged
through winget rather than as MSIX.** Why:

- It is Microsoft's stated path for new native apps, and accessibility comes
  from the controls.
- It is what the roadmap already says, and it reuses the binding R9 builds
  for macOS anyway.
- Unpackaged avoids the HKCU and AppData virtualization that would hide
  `ketch path install` from the CLI.

The costs come first: R9 has to agree on a UniFFI version with the C#
generator, and gains `cdylib`.

`windows-reactor` is the alternative worth a time-boxed spike before
committing: native WinUI with no binding and no C# would fit ketch better if
it matures. WPF is the fallback if WinUI's runtime licence is unacceptable for
the GPL build. Slint, iced and egui do not meet "native" (they draw their own
widgets) and stay out, as in `docs/research-desktop.md`.

## Linux

### Candidates

| Toolkit | Latest (date) | Licence | Native look and accessibility | How it reaches the core | Source |
| --- | --- | --- | --- | --- | --- |
| GTK 4 + libadwaita via gtk4-rs | GTK 4.24.0 (2026-09-11), libadwaita 1.10.0 (2026-09-14); crates `gtk4` 0.11.5 (2026-09-20, MSRV 1.92, features up to `v4_24`), `libadwaita` 0.9.2 (2026-07-07, features up to `v1_10`) | GTK and libadwaita LGPL-2.1-or-later (`meson.build` at 4.24.0 / 1.10.0); bindings MIT | libadwaita is "Building blocks for modern GNOME applications": the GNOME look. GTK's accessibility goes to AT-SPI on Linux | Links `ketch-core` directly | https://gitlab.gnome.org/api/v4/projects/GNOME%2Fgtk/releases, https://gitlab.gnome.org/api/v4/projects/GNOME%2Flibadwaita/releases, https://crates.io/api/v1/crates/gtk4, https://crates.io/api/v1/crates/libadwaita, https://gitlab.gnome.org/GNOME/libadwaita/-/blob/main/README.md, https://docs.gtk.org/gtk4/section-accessibility.html |
| Relm4 on top of gtk4-rs | 0.11.0 (2026-04-08), MSRV 1.93, depends on `gtk4` ^0.11.2, `libadwaita` ^0.9.1, `tokio` ^1.51 | Apache-2.0 OR MIT | As GTK; an Elm-style layer, "compatible with GTK4 and libadwaita" | Directly | https://crates.io/api/v1/crates/relm4, https://crates.io/api/v1/crates/relm4/0.11.0/dependencies, https://github.com/Relm4/Relm4 |
| Qt 6 via CXX-Qt | Qt 6.12.0 (directory dated 2026-09-30), 6.11.2 (2026-08-18); `cxx-qt` 0.10.0 (2026-08-24), MSRV 1.85 | Qt: LGPLv3, GPL or commercial, and some modules GPL-only, including "Qt Qml Compiler"; CXX-Qt MIT OR Apache-2.0 | The KDE look through QML/Qt Quick. Accessibility not checked | Directly, but through a C++ bridge (`unsafe extern "C++"` blocks in its own minimal example) and a C++ build | https://download.qt.io/official_releases/qt/, https://doc.qt.io/qt-6/licensing.html, https://crates.io/api/v1/crates/cxx-qt, https://github.com/KDAB/cxx-qt |
| libcosmic (iced fork) | Not on crates.io; `Cargo.toml` says 1.0.0, MSRV 1.93; its iced is a vendored fork; COSMIC epoch-1.9.0 (2026-09-23) | MPL-2.0 | The COSMIC look; an `a11y` feature | Directly, as a git dependency | https://github.com/pop-os/libcosmic, https://github.com/pop-os/cosmic-epoch/releases |
| Slint | 1.18.1 (2026-09-21) | as above | `native` is `qt` on Linux when Qt is installed, else `fluent`; also a `cosmic` style | Directly | as above |
| egui / eframe | 0.36.2 | as above | Own look, AccessKit | Directly | as above |

### What distributions ship

GTK apps outside Flatpak use the system's GTK, which decides the oldest API
the app may call:

| Distribution | GTK 4 | libadwaita | Source |
| --- | --- | --- | --- |
| Ubuntu 24.04 LTS (noble) | 4.14.5 | 1.5.0 | Launchpad API, `getPublishedSources` |
| Ubuntu 25.10 (questing) | 4.20.1 | 1.8.0 | Launchpad API |
| Ubuntu 26.04 LTS (resolute) | 4.22.4 | 1.9.1 | Launchpad API |
| Debian 13 (trixie) | 4.18.6 | 1.7.6 | https://sources.debian.org/api/src/gtk4/, https://sources.debian.org/api/src/libadwaita-1/ |
| Debian 12 (bookworm) | 4.8.3 | 1.2.2 | same |

On Flathub (x86_64) the GNOME 51 runtime is already in use by 139 apps and
GNOME 50 by 705 (https://flathub.org/api/v2/runtimes); KDE 6.11 by 243.

### Packaging

- **Flatpak sandbox against what ketch does.** Flathub requires static
  permissions "kept to an absolute minimum" and portals where they exist. It
  says development tools and similar broad-scope software "are generally not
  well-suited for Flatpak", accepted only from upstream
  (https://github.com/flathub-infra/documentation/blob/main/docs/02-for-app-authors/02-requirements.md).
  ketch needs to write `~/.ketch`, shell startup files and the user's man and
  completion directories, so at least `--filesystem=home` (the `home`
  permission; https://docs.flatpak.org/en/latest/sandbox-permissions.html). A
  hook or any client binary run from the app would run inside the sandbox's
  runtime unless it goes through `flatpak-spawn --host`, which "requires
  access to the org.freedesktop.Flatpak D-Bus interface", i.e. leaving the
  sandbox (https://docs.flatpak.org/en/latest/flatpak-command-reference.html).
  A Flatpak build also needs offline cargo sources, which
  `flatpak-builder-tools/cargo/flatpak-cargo-generator.py` generates
  (https://github.com/flatpak/flatpak-builder-tools/tree/master/cargo).
- **Outside Flatpak**: a tarball, `.deb` or AppImage (e.g. `cargo-packager`,
  see `docs/research-desktop.md`) links the system GTK, so the table above
  sets the floor, e.g. `v4_14` / `v1_5` features for Ubuntu 24.04 LTS.
- **Tray.** GTK 4 has no status icon: `GtkStatusIcon` is documented for GTK 3
  (https://docs.gtk.org/gtk3/class.StatusIcon.html) and absent from GTK 4's
  reference (https://docs.gtk.org/gtk4/class.StatusIcon.html returns 404).
  `ksni` 0.3.6 (2026-07-15, Unlicense) implements StatusNotifierItem from Rust
  (https://crates.io/api/v1/crates/ksni).

### Threading fit

- **gtk4-rs.** The gtk-rs book's answer to a blocked main loop is
  `gio::spawn_blocking` for the work and `glib::spawn_future_local` to receive
  messages on the main loop
  (https://gtk-rs.org/gtk4-rs/stable/latest/book/main_event_loop.html). The
  core call goes in the former, and the reporter sends events on a channel
  the latter drains. `Cancel` is a clone held by the window.
- **Relm4** runs commands on tokio (it depends on `tokio`), and a core call
  must not run inside an async task. In particular `ketch registry push`
  would panic in `block_on`, so it has to go through a blocking spawn there
  too.
- **CXX-Qt.** Results go back to the Qt thread through `CxxQtThread`
  (https://github.com/KDAB/cxx-qt/blob/main/book/src/bridge/extern_rustqt.md).

### Recommendation for Linux

**GTK 4 + libadwaita through gtk4-rs, linking `ketch-core` directly**, with
Relm4 as an optional layer (decision below). Why:

- It is the toolkit native to GNOME, and accessible through AT-SPI.
- There is no FFI, no C++ build, and one language with the core.
- The bindings track GTK closely (`v4_24` and `v1_10` within weeks of the C
  releases).
- LGPL-2.1-or-later, linked dynamically as distributions and Flatpak
  runtimes ship it, suits all three of ketch's licences.

Qt via CXX-Qt is the alternative if KDE is the target. It brings a C++
toolchain, and GPL-only modules have to be kept out of non-GPL builds.
libcosmic only looks native on COSMIC, and it is a git dependency on a fork
of iced.

## Shared

- **One UI for both is what Slint or Avalonia would give**, at the price of
  drawn rather than native widgets. Slint's licence mirrors ketch's
  (GPL-3.0-only, royalty-free with the same embedded exclusion, commercial).
  This contradicts the native-per-OS decision and is recorded only as the
  fallback.
- **Design tokens.** F14's `desktop/macos/design/tokens.json` could be
  generated into XAML resources or GTK CSS. But Fluent and Adwaita bring
  their own design languages, so whether the brand carries over is the
  creator's call.
- **MSRV.** `windows-reactor` needs 1.95, `gtk4` 1.92, Relm4 1.93; ketch's
  promise is 1.86 and the pin is 1.98.1 (`mise.toml`). As
  `docs/research-desktop.md` already says, a desktop crate gets its own MSRV.

## Open decisions for the creator

1. **Windows front-end language.** C# with WinUI 3 over R9's binding (the
   roadmap, recommended), or `windows-reactor` in Rust with no binding (one
   release old). A spike of the latter first, or not?
2. **R9's UniFFI version**, if Windows uses C#: pin 0.31.x now so
   `uniffi-bindgen-cs` v0.11.0 matches, keep 0.32.2 and wait for the
   generator, or give Windows a hand-written C ABI instead. Also: add
   `cdylib` to R9's `crate-type`.
3. **Windows distribution.** Unpackaged through winget (recommended), MSIX
   with the `unvirtualizedResources` restricted capability, or the Microsoft
   Store. Framework-dependent or self-contained Windows App SDK runtime.
4. **The Windows App SDK licence and the GPL-3.0-only build.** Accept, add a
   linking exception for it, or ask counsel. WPF avoids the question.
5. **Linux target desktop.** GNOME with libadwaita (recommended; KDE and
   COSMIC users get GNOME styling), or KDE with Qt.
6. **Relm4 or plain gtk4-rs.** Relm4 is less code and brings tokio. Plain
   gtk4-rs has one fewer dependency.
7. **Linux distribution.** Flatpak (needs `--filesystem=home`, probably host
   spawning for hooks, and Flathub's acceptance), or packages and tarballs
   against the system GTK, which sets the oldest GTK/libadwaita supported.
8. **Tray parity with the macOS menu-bar extra.** Windows notification-area
   icon (yes in both Windows options). Linux StatusNotifierItem through `ksni`,
   or no tray on Linux.
9. **`unsafe` policy for the app crates.** The reactor guide's modal dialogs
   and CXX-Qt bridges need `unsafe`; whether gtk4-rs/Relm4 code builds under
   `unsafe_code = "forbid"` was not checked. Decide whether app crates keep
   `forbid` like `ketch-core`, or relax it the way R9 does for `ketch-ffi`.

## Unverified

- Whether a WinUI 3 C# project builds with the .NET CLI alone (no Visual
  Studio) on a CI runner was not checked.
- `windows-reactor`'s accessibility was not tried. That UI Automation works
  is inferred from it creating real WinUI controls.
- Accessibility of Avalonia and of Qt Quick under CXX-Qt was not checked.
- Whether gtk4-rs subclassing macros or Relm4's macros compile under
  `unsafe_code = "forbid"` was not checked.
- GNOME Shell showing no StatusNotifierItem icons without an extension is a
  common report, not checked against a primary source: **unverified**.
- Flathub's actual reaction to a package-manager GUI with `--filesystem=home`
  is not documented case by case; only the requirements text was read.
- Whether the Windows App SDK runtime (framework-dependent or self-contained)
  is compatible with distributing the app under GPL-3.0-only was not
  researched; it is a legal question.
- No bundle sizes or memory figures are compared, for the same reason as in
  `docs/research-desktop.md`: no official numbers were found.
