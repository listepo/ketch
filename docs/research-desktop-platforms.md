# Research: the desktop apps on macOS, Windows and Linux

Task R11. Sources checked 2026-10-01 unless a date says otherwise. This page
builds on two others and does not repeat them:
[`docs/research-desktop.md`](research-desktop.md) (the macOS app's choices,
F12–F14) and
[`docs/research-desktop-windows-linux.md`](research-desktop-windows-linux.md)
(R10's toolkit comparison). It answers what the three apps do, what is written
once and what each platform writes for itself, the interfaces between those
parts, and which tasks follow.

Status: research. The recommendations are proposals; the decisions listed at
the end belong to the creator. The tasks it produced are `D1`–`D19` in
`plan.md`.

## Starting point

Creator decisions this page takes as given:

- A native UI per platform over one core, macOS first (2026-09-30,
  `docs/research-desktop.md`, "Decision").
- macOS: SwiftUI on macOS 26 with Liquid Glass, over `ketch-ffi` (R9, UniFFI);
  menu-bar extra; shares the ketch root and lock with the CLI (F12).
- Windows: C# and WinUI 3 on the Windows App SDK, over `ketch-ffi` through
  UniFFI; `ketch-ffi` stays on UniFFI 0.32 for now (2026-10-01, this task's
  brief; R9's card).
- Linux: Vala as the main language; the toolkit (GTK or Qt/KDE) and the UI
  markup are this page's to recommend (2026-10-01, this task's brief).

Repository facts used throughout (`main` at 68f9a56):

- `ketch-ffi` exports one object, `KetchCore`, built with
  `new(root, reporter, decider)`, with `root`, `installed`, `search`,
  `outdated`, `install`, `upgrade`, `uninstall`, `changelog` and `doctor`,
  plus `CancelToken`, the `Reporter` and `Decider` callback interfaces and one
  error enum `KetchError { Busy, Cancelled, NotFound, Network, Verification,
  Other }` (`crates/ketch-ffi/src/lib.rs`, `callbacks.rs`, `error.rs`). It
  builds `lib`, `staticlib` and `cdylib` (`crates/ketch-ffi/Cargo.toml`).
- Every `KetchCore` method is synchronous; a mutating one holds `state::Lock`
  for its whole run and a second one, from any thread or process, fails at
  once with `Busy` (`crates/ketch-ffi/src/lib.rs`, header).
- The macOS app runs on `FakeKetchCore`; its view of the core is
  `desktop/macos/Ketch/Core/KetchCoreProtocol.swift`, written before R9 and
  not identical to it (gaps below).
- The design tokens live in `desktop/macos/design/tokens.json` (W3C DTCG
  2025.10) and Style Dictionary 5.5.5 generates Swift, CSS for the preview and
  the `DESIGN.md` front matter through custom formats
  (`desktop/macos/design/build.mjs`, `docs/research-design-system.md`).
- The Figma file "ketch for macOS — Liquid glass" has nine screens in Light
  and Dark: Installed, Discover, Updates, Package detail, Activity, Doctor,
  Settings · Appearance, Uninstall sheet, Menu bar extra
  (`desktop/macos/design/figma.md`).

Sources are cited by key (`[M3]`, `[W4]`, `[L9]`, `[X1]`) and listed with
their URL and version or date under [Sources](#sources) at the end, so the
tables stay readable.

## 1. Capabilities

What the app does, or should do, on each platform: the native API or library,
then its maturity. "Core" means the work is ketch's own and only the UI is per
platform. Status in the macOS app is from `desktop/macos/` (F12, on the fake
core).

| Capability | Core contract | macOS | Windows | Linux (GNOME) |
| --- | --- | --- | --- | --- |
| Installed list | `installed()` | SwiftUI list; exists in F12 | WinUI `ListView` in a `NavigationView` page [W9] | GTK list of `AdwActionRow`s in a boxed list [L11] |
| Search and discover | `search(query, limit)`; no `latest` per result (gap G4) | Exists in F12 | WinUI `AutoSuggestBox` and list (stable controls) [W9] | `GtkSearchEntry` and boxed list [L11] |
| Install, upgrade, uninstall with progress and cancel | `install`/`upgrade`/`uninstall`, `Reporter`, `CancelToken`; `uninstall` takes no token (gap G5) | Exists in F12 | `ProgressBar`/`ProgressRing` [W10] | `GtkProgressBar` [L11] |
| Updates screen | `outdated()`; pinned packages are left out (gap G3) | Exists in F12, with a "held" group | Page in `NavigationView` | Page in the split view |
| Changelog | `changelog(package, version)`, one version (gap G2); text already sanitised | `AttributedString(markdown:)` (F12) | No Markdown control in WinUI was checked: **unverified**; plain text is the floor | No Markdown widget in GTK was checked: **unverified**; plain text is the floor |
| Doctor | `doctor()` reports; repairs nothing (gap G6) | Exists in F12, fixes named but not runnable | Page | Page |
| Activity and log | Reporter events while running; persistent history (`ketch history`, `stats.db`) not exported (gap G6) | Session log in F12 | Page | Page |
| Settings, appearance | App preferences are the app's; ketch's `config.toml` not exported (gap G6) | UserDefaults; tint, glass, accent, wash (F12) | `ElementTheme` follows the system; accent from the system | `AdwStyleManager` colour scheme, `accent-color` since libadwaita 1.6 [L9]; preferences in `AdwPreferencesDialog` (1.5) [L9] |
| Binary-choice dialog | `Decider.choose_binary` | Sheet (F12) | `ContentDialog` [W10] | `AdwAlertDialog` (1.5) [L9] |
| Processes holding files | `Decider.stop_processes` | Not in the app's protocol yet (gap G1) | `ContentDialog` | `AdwAlertDialog` |
| Busy and lock | `KetchError::Busy { pid }` | Banner with Retry (F12) | `InfoBar` [W10] | `AdwBanner` (1.3) [L9] |
| Background update checks | `outdated()` (cached ten minutes, `lib.rs`) on the app's timer | Timer while running (F12) | Timer while running; needs the tray icon to outlive the window | Timer while running; Background portal to outlive the window [L12] |
| Update notifications | App decides which upgrades are new | `UNUserNotificationCenter` (10.14+) [M1]; not built yet | `AppNotificationManager`, works unpackaged [W3] | `GNotification` [L14], Notification portal v2 under Flatpak [L12] |
| Tray or menu-bar extra | Same data as Updates | `MenuBarExtra` with `.window` style (13+) [M2]; exists in F12 | `Shell_NotifyIcon` (Win32) [W6]; no WinUI control, **unverified** [W6] | No GNOME HIG pattern [L11]; StatusNotifierItem via `libayatana-appindicator-glib` 2.0.3, GPL-3.0 [L15]; GNOME Shell without an extension shows none, **unverified** [L15] |
| Launch at login | — | `SMAppService.mainApp` (13+) [M3]; exists in F12 | `ActivationRegistrationManager.RegisterForStartupActivation` [W4] or the `HKCU\…\Run` key [W5] | Background portal `autostart` (v2) [L12], or an XDG autostart entry (spec 0.5) [L14] |
| Self-update of the app | — | Sparkle 2.10.0, EdDSA feed (F13) [M9] | Velopack 1.2.161 [W16] or `winget upgrade` [W16] | The package manager: Flatpak or the distribution; no in-app updater |
| PATH integration | `shell.rs` (`ketch path install`) not exported (gap G6) | Core | Core; writes `HKCU\Environment\Path`, which MSIX would virtualise (R10) | Core; needs `--filesystem=home` under Flatpak (R10) |
| Deep links (`ketch://`) | Parsing and validation belong in the core (trust boundary) | `CFBundleURLTypes` + `onOpenURL` [M4] | `RegisterForProtocolActivation` [W4] | `.desktop` `x-scheme-handler` entry, **unverified**; `GApplication` open [L14] |
| Single instance | — | One app instance by default, **unverified** | `AppInstance.FindOrRegisterForKey` [W13] | `GApplication` uniqueness [L14] |
| Accessibility | — | SwiftUI accessibility modifiers [M7] | UI Automation through the controls [W11] | `GtkAccessible` to AT-SPI [L16] |
| Localisation | Core messages are English (`KetchError` text) | String Catalog, exports XLIFF [M5] | MRT Core `.resw` [W12] | gettext through Meson's i18n module [L17] |
| Dark mode, high contrast, reduced transparency | — | `colorSchemeContrast`, `accessibilityReduceTransparency` [M7]; Liquid Glass follows them (F12) | `ThemeDictionaries` `Light`/`Dark`/`HighContrast` [W15], contrast themes [W11] | `AdwStyleManager` `dark`, `high-contrast` [L9] |
| Crash reporting, off by default | — | MetricKit (`MXMetricManager`, deprecated at macOS 27.2 for `MetricManager`) [M8]; Sentry Cocoa 9.30.0 sends nothing without a DSN [M12] | WER LocalDumps [W19]; Sentry .NET 6.12.0 [W19] | No Vala SDK checked; the `sentry` crate 0.49.3 could report from the Rust side [X8] |
| About and licence | Licence string from `Cargo.toml` | About window (F12) | `ContentDialog` or page | `AdwAboutDialog` (1.5) [L9] |

Maturity, summed up: everything on macOS is stable system API. On Windows the
controls and app-lifecycle APIs are stable in Windows App SDK 2.5.1 [W1]; the
gaps are the tray (Win32 only) and the C# binding (section 4). On Linux the
GTK/libadwaita widgets are stable (GTK 4.24.1, libadwaita 1.10.0 [L8]);
Blueprint is self-declared experimental [L5], and a tray depends on what the
desktop shell supports.

## 2. Common and specific

### Written once

- **The core and its contract.** Everything that touches the install tree is
  in `ketch-core` and reached through `ketch-ffi` (macOS, Windows) or a C ABI
  over the same Rust (Linux, section 5). Each app only shows what the core
  returns and answers what it asks. Validation of anything a deep link
  carries also belongs here, behind `Manifest::validate` and
  `config::validate_repo` (`AGENTS.md`, "Trust boundaries").
- **The contract fixes** (section 3a), once in Rust, so no app grows a
  workaround.
- **Design tokens.** `tokens.json` stays the one source. Style Dictionary has
  no built-in XAML or GTK format [X2], so the generator gains two custom
  formats beside the Swift one: a XAML `ResourceDictionary` with
  `ThemeDictionaries` keyed `Light`, `Dark` and `HighContrast` [W15], and a
  GTK stylesheet that sets libadwaita's CSS variables (`--accent-bg-color`
  and the rest, overridable by apps, libadwaita 1.6+) [L9] using GTK's custom
  properties (GTK 4.16+) [L10]. Only brand tokens travel: accent, status
  colours, spacing, radii, type scale. Glass, wash and elevation tokens stay
  macOS-only, because Mica/Acrylic and Adwaita bring their own surfaces.
  Figma's variable code syntax knows only `WEB`, `ANDROID` and `iOS` [X4], so
  the GTK names can ride on `WEB` and the XAML names live only in
  `tokens.json`.
- **Screen specs.** The nine screens of the Figma file are the common UI
  (section 3b); each platform re-draws them in its own idiom rather than
  copying the glass.
- **Strings.** Key naming and an English glossary are shared. The formats are
  not: Xcode's String Catalog exports XLIFF [M5]; Weblate handles PO,
  RESX/RESW and XLIFF but not `.xcstrings` [X5]; translate-toolkit converts
  PO↔XLIFF and RESX↔PO but has no `.xcstrings` or `.resw` converter [X6]. A
  single generated source would need a converter of ketch's own; each app
  keeping its native format and one Weblate project translating all three is
  cheaper (decision 7).
- **Test fixtures.** One set of language-neutral scenarios (records, event
  streams, errors such as `Busy` and `Cancelled`), produced from the Rust
  records so they cannot drift, read by the Swift, C# and Vala fake cores.

### A shared app layer in Rust: not now

A `ketch-app` crate holding view state, update polling, notification
decisions and caching, exported to all three apps, was weighed and is not
recommended yet:

- The state itself cannot be shared. SwiftUI observes `@Observable`, WinUI
  binds `INotifyPropertyChanged`, GTK binds GObject properties; a Rust store
  would push every change through a callback and each app would mirror it
  anyway.
- What would be shared is small. Caching is already in the core (`outdated`
  reuses an answer from the last ten minutes, `crates/ketch-ffi/src/lib.rs`).
  "Which upgrades are new since the last notification" is a set difference.
  The one real piece of logic is folding reporter events (task ids, sizes,
  progress, `Abandoned`) into per-package progress; that is about one screen
  of code per app.
- It would have to be exported twice: through UniFFI and through the C ABI
  Vala needs (section 5).

So: thin view-models per app, the shared fixtures above to keep their
behaviour equal, and the decision revisited if the event fold turns out to
need the same fix in two apps.

### Per platform

| Concern | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Language and UI | Swift 6, SwiftUI | C#, WinUI 3 on Windows App SDK 2.5.1, .NET 10 LTS [W1] [W2] | Vala 0.56 LTS, GTK 4 + libadwaita [L1] [L8] |
| Markup | none (code) | XAML | Blueprint, compiled to GtkBuilder `.ui` (section 5) |
| Reaching the core | UniFFI Swift bindings in an XCFramework (R9) | UniFFI C# bindings (section 4) | C ABI + hand-written VAPI (section 5) |
| Materials | Liquid Glass (F12) | Mica for the window, Acrylic for transient surfaces [W7] | Adwaita, no translucency |
| Window chrome | Unified toolbar | `TitleBar` control (Windows App SDK 1.7+) [W8] | `AdwHeaderBar` in `AdwToolbarView` (1.4) [L9] |
| Tray | `MenuBarExtra` [M2] | Win32 notification-area icon [W6] | None by default; SNI optional [L15] |
| Notifications | `UNUserNotificationCenter` [M1] | `AppNotificationManager` [W3] | `GNotification` / portal; libportal 0.11.0 ships a VAPI [L14] [L12] [L13] |
| Autostart | `SMAppService` [M3] | `RegisterForStartupActivation` [W4] | Background portal / XDG autostart [L12] [L14] |
| Packaging, signing, updates | `.dmg`, Developer ID, notarisation, Sparkle (F13) | Unpackaged zip or installer, Artifact Signing [W17], winget, Velopack [W16] | Flatpak or distribution packages (R10's decision 7) |
| Accessibility API | NSAccessibility via SwiftUI [M7] | UI Automation [W11] | AT-SPI via `GtkAccessible` [L16] |
| Build | XcodeGen + `xcodebuild` | `dotnet build` (templates are alpha, 0.0.7-alpha) [W18] | Meson (1.12.1) with Vala support [L17] |

## 3. Interfaces

### 3a. The core contract

`ketch-ffi` today against what the macOS app's protocol expects
(`desktop/macos/Ketch/Core/KetchCoreProtocol.swift`); the Windows and Linux
apps would hit the same gaps.

| # | Gap | `ketch-ffi` today | Proposed |
| --- | --- | --- | --- |
| G1 | Callbacks | `Reporter` and `Decider` are `callback_interface`s passed to `KetchCore::new`; UniFFI now calls callback interfaces "(soft) deprecated" in favour of foreign traits [X1]. The app passes a reporter and decider per call and has no `stop_processes` | Foreign traits (`#[uniffi::export(foreign)]`, UniFFI 0.32 [W21]); reporter, decider and cancel token per operation, so one `KetchCore` serves concurrent screens; the app's protocol gains `stop_processes` |
| G2 | Changelog | `changelog(package, version) -> Changelog` for one version | `changelog_range(package, from, to) -> Vec<Changelog>`, newest first, so Updates can show everything between installed and latest |
| G3 | Held packages | `outdated()` leaves pinned packages out; `Upgrade { name, installed, latest, tag }` | Report them with `pinned: true` (and the `ketch.lock` that holds them, when one does) so Updates can show the held group |
| G4 | Latest version | `RegistryPackage { name, source, description }` | `latest: Option<String>` from the listing cache, `None` when unknown |
| G5 | Uninstall | No `CancelToken`, no reporter; an unknown name is `NotFound` | Takes the token and reporter like the others; also removes a leftover store folder for a name with no record, as the CLI does (`install::remove_package_dir`, `src/cmd/pkg.rs`) |
| G6 | Missing operations | Not exported: history, info, pin/unpin, rollback, prune, registry refresh, `path` status and install, doctor fixes, reading ketch's config | Exported on the same pattern, each a thin call into existing core code |
| G7 | Small shape differences | `Busy { pid: Option<u32> }`; stages `Resolving…Installing`; progress keyed by task id | Kept: they are what the core knows. The apps adapt (the macOS protocol was written before R9) |

The proposed app-level contract every app codes against (each app has its own
copy in its language, like `KetchCoreProtocol.swift`):

- `installed`, `search`, `outdated`, `changelog_range`, `doctor`, `history`,
  `info`: reads, callable any time, from a worker thread.
- `install`, `upgrade`, `uninstall`, `pin`, `unpin`, `rollback`, `prune`,
  `doctor_fix`, `path_install`: mutating. One at a time per app (a second one
  in the same process gets `Busy` too, R10), each with its own reporter,
  decider and `CancelToken`.
- Errors: `Busy` shows a banner with Retry and never retries in a loop;
  `Cancelled` is logged, not alerted; `NotFound`, `Network`, `Verification`,
  `Other` are alerts with the core's text.
- Events: stage per package, progress per task, status/warn/note lines for
  the log; `Abandoned` removes whatever shows the task.
- Threading: every call blocks; callbacks arrive on the core's thread and the
  app hops to its UI thread (`DispatchQueue.main`, `DispatcherQueue`,
  `GLib.Idle`/`MainContext`).

UniFFI can also export `async fn` to Swift and others, but has "no builtin
way to cancel a future" [X10]; ketch's own `CancelToken` already does that, so
the contract stays synchronous.

### 3b. The common UI

From the Figma file and the macOS app:

- **Navigation:** a sidebar (or the platform's equivalent) with Installed,
  Discover, Updates, Activity, Doctor, Settings; a package opens a detail view
  with Overview, Changelog and Files.
- **Modal flows:** uninstall confirmation, binary choice, processes holding
  files, upgrade-all confirmation.
- **Ambient:** the busy banner, the running operation's progress, toasts for
  finished work, the tray/menu-bar panel (update count, Update all, the
  running operation, Open, Check now, Quit).
- **States per list screen:** loading, empty, content, partial (a source
  unreachable, warned), error with Retry, busy.
- **Operation states:** queued, running (stage, progress), waiting for a
  decision, cancelled, failed, done.
- **Components** (the Figma Components page): icons, app icon, badge, button,
  nav item, search field, progress, segment, swatch, package row.

| Component | macOS | Windows | Linux |
| --- | --- | --- | --- |
| Sidebar | `NavigationSplitView` [M6] | `NavigationView`, left mode [W9] | `AdwNavigationSplitView` (1.4) with `AdwBreakpoint` [L9] |
| Package row | Glass card | List item with Fluent styling | `AdwActionRow` in a boxed list [L11] |
| Update badge | `status.update` pill | Same token as `InfoBadge`-style pill | Same token as a pill label |
| Confirmation / choice | Sheet | `ContentDialog` [W10] | `AdwAlertDialog` (1.5) [L9] |
| Busy | Glass banner | `InfoBar` [W10] | `AdwBanner` (1.3) [L9] |
| Finished work | Inline status | `InfoBar` or app notification | `AdwToast` [L11] |
| Empty state | Placeholder view | Text and icon in the page | `AdwStatusPage` |
| Settings | Settings scene | Settings page in `NavigationView` | `AdwPreferencesDialog` (1.5) [L9] |
| Progress | Linear progress | `ProgressBar` [W10] | `GtkProgressBar` |

### 3c. Per-platform departures

- **macOS** (Apple HIG [M10]): sidebar plus toolbar, Liquid Glass on cards
  and controls (F12, F14), SF Pro and SF Symbols, sheets for confirmations,
  a menu-bar extra as a glass panel, Settings as its own scene.
- **Windows** (Windows design guidance [W9] [W14]): `NavigationView` on the
  left with Settings pinned at the bottom, which is where the control puts it;
  a custom `TitleBar` over Mica; Acrylic only for flyouts; Segoe UI Variable
  and Segoe Fluent Icons; `ContentDialog` for confirmations and `InfoBar` for
  the busy state; a notification-area icon whose flyout is the tray panel. No
  glass imitation: Fluent's materials replace it.
- **Linux** (GNOME HIG [L11]): `AdwNavigationSplitView` that collapses to one
  pane on narrow windows; header bars, boxed lists, `AdwAlertDialog`, toasts
  for finished work while the window is open and system notifications for
  what matters outside it; preferences in a
  dialog; symbolic icons; no tray in the HIG, so background checks rely on the
  Background portal, which GNOME Shell shows in its background-apps menu
  (`js/ui/status/backgroundApps.js`) [L15]. Adwaita's flat style replaces the
  glass; only brand tokens carry over.

## 4. C# over UniFFI 0.32: the generator gap

Facts:

- `uniffi-bindgen-cs` last released v0.11.0+v0.31.0 on 2026-06-23, which pins
  UniFFI 0.31.0; nothing has landed on its `main` since [W20]. Its README:
  "`uniffi-bindgen-cs` targets a specific `uniffi-rs` version." [W20]
- PR #176 "Upgrade to uniffi-rs 0.32.0" has been open since 2026-07-10, last
  updated 2026-09-04, head `0fc022aa1d73fb1dda91a778b63f2824d7dca58b` on
  `dennisameling/uniffi-bindgen-cs` [W20]. Its description reports the binding
  tests passing and the 0.32 changes handled (checksums, `ForeignBytes`);
  that is the author's claim, **unverified** here. Issue #183 asks for 0.32
  support [W20].
- UniFFI 0.32.0 broke external generators: `[ByRef] bytes` now cross as
  `ForeignBytes`, and the pipeline code was reworked [W21]. ketch-ffi is on
  0.32.2 (2026-09-23); the last 0.31 is 0.31.2 (2026-06-17) [W21].

Ways forward:

| Route | Cost | Risk |
| --- | --- | --- |
| A. Wait for a release | None now | Unknown date; three months without a merge |
| B. Build the generator from PR #176, pinned by commit, and run its output against `ketch-ffi` 0.32.2 | A `cargo install --git … --rev` pin in `mise.toml` or a script; a .NET test like the Swift one | The PR targets 0.32.0; 0.32.1/0.32.2 changes may or may not matter to it. Found out by the test, not guessed |
| C. Pin `ketch-ffi` to UniFFI 0.31.2 | A downgrade of a pinned version (creator approval) | Swift side loses nothing it uses today; a later bump repeats the problem |
| D. A C ABI shim for C# | csbindgen 1.9.8 or `LibraryImport` [W22] over the C ABI Linux needs anyway (section 5) | A second binding to keep in step; loses UniFFI's generated objects and errors |

Recommended: **B first**, as the first Windows task, with the result recorded.
If the pinned PR fails against 0.32.2, fall back to **D** when the Linux C ABI
exists, else **C** with the creator's approval. Upstreaming any fix to PR #176
is part of B.

**Result (D10, 2026-10-03): route B works.** The generator built from PR #176
at `0fc022aa1d73fb1dda91a778b63f2824d7dca58b` builds against UniFFI 0.32.0
and calls itself `0.12.0+v0.32.0` [W23], yet it reads the
metadata of `ketch-ffi` built against 0.32.2 and its C# runs against that
library: `desktop/windows/KetchCore.Tests` calls `ketch_version`,
`installed`, `doctor`, a cancelled install and an install with a C# reporter,
on macOS locally and on `windows-latest` in CI (the `ketch-cs` job). Three
things were needed, none of them a change to the generator:

- **One field renamed.** `TaskKind::Download { batch }` became `batch_id`.
  C# turns each field into a property of the variant's nested record, and a
  property named `Batch` inside `TaskKind.Download` collides with the inherited
  `TaskKind.Batch` variant (CS8866). Swift and Kotlin have no such clash; the
  rename is a breaking change to the binding, made in D10. Worth reporting to
  the generator upstream; not reported from here.
- **Public types.** The generator emits `internal` by default;
  `desktop/windows/uniffi.toml` sets `access_modifier = "public"` and the
  `Ketch.Ffi` namespace [W23].
- **The pinned binary, not the first on `PATH`.** `cargo install` from git
  (mise's `cargo:` backend) builds the pin, but an older
  `uniffi-bindgen-cs 0.11.0+v0.31.0` in `~/.cargo/bin` shadows it and reads
  0.32 metadata as garbage ("Invalid string data"). `scripts/csharp.sh` takes
  the mise install path and refuses any generator whose version is not
  `+v0.32.*`.

Routes C and D stay unused. When the generator publishes a 0.32 release, the
`mise.toml` pin moves from the commit to that version.

## 5. Linux in Vala

### Toolkit: GTK, not Qt

- Vala is built on GObject: its classes are GObject classes and its
  bindings are generated from GObject introspection data [L2]. It is maintained: 0.56.19 (LTS) on 2026-03-30, `main` active on
  2026-08-15 [L1]. elementary OS writes its documentation's examples in Vala
  [L18].
- No maintained Qt or QML binding for Vala was found: none in the GNOME
  bindings list or `vala-extra-vapis`, none on GitHub or GNOME GitLab
  [L3]. Absence is a search result, not proof.
- KDE documents Kirigami apps in C++, Python and Rust (via cxx-qt), not Vala
  [L4].

So Vala means GTK 4 + libadwaita, the GNOME look. A KDE/Qt app is possible
only in another language: Rust through cxx-qt (0.10.0) [L4], which R10
already weighed (C++ bridge and build, GPL-only Qt modules to keep out of
non-GPL builds), or C++/Python. Running a GTK app on KDE Plasma works but looks
like GNOME (R10).

### Markup: Blueprint, compiled to GtkBuilder

| Option | Status | Fit |
| --- | --- | --- |
| Blueprint | v0.22.2 (2026-07-10); "still experimental. Future versions may have breaking changes" [L5]; in the GNOME SDK [L7] | Readable in diffs; compiled at build time by Meson into the GtkBuilder `.ui` files a GResource carries [L5], so the app runs on plain GtkBuilder and Vala binds widgets the same way |
| GtkBuilder XML | Stable, part of GTK [L6]; GNOME Builder's Vala template uses it [L6] | Verbose; edited by hand or by a tool |
| UI in Vala code | Stable | No markup to review; layout mixed with logic |

Recommended: **Blueprint, pinned as a Meson subproject**. Its experimental
status is contained: the compiler is a build step, and the `.ui` files it
emits are ordinary GtkBuilder, so leaving Blueprint later means committing
those files.

### Reaching the core from Vala

UniFFI has no Vala or C generator: its third-party list names JavaScript,
Kotlin Multiplatform, Go, C#, Dart, Java, Node and Haskell [L21], and a search
for a Vala generator found none. `uniffi-bindgen-cpp` emits C++, which Vala
cannot call, and targets UniFFI 0.29.4 [L21].

So Vala needs a C ABI. Options:

| Option | How | Verdict |
| --- | --- | --- |
| A C ABI crate (`ketch-capi`) over `ketch-core`, header by cbindgen, VAPI by hand | cbindgen 0.29.4 (MPL-2.0) [L20]; the VAPI is small: an opaque `KetchCore` handle as a `[Compact]` class with a `free_function` [L2], callbacks as delegates whose target is the C `user_data` (Vala's default `has_target`) [L2] | **Recommended** |
| Records as JSON strings across that ABI | Records serialised with serde (already a dependency); parsed in Vala with json-glib 1.10.8, LGPL-2.1-or-later, GIR, in the GNOME SDK [L19] [L7] | Recommended with it: about a dozen functions, no struct layouts to keep in step, and the JSON schema can be generated like the config schemas |
| Typed C structs per record | One `#[repr(C)]` struct and accessor per field | More `unsafe` code and VAPI to maintain, for no user-visible gain |
| safer-ffi | Last stable 0.1.13 (2024-09-17); 0.2.0 still a release candidate (2026-01-16) [L20] | Not now |
| A GObject library with GIR, so `vapigen` makes the VAPI [L2] | Needs GObject types written from Rust and introspection data; no maintained Rust-to-GIR tool was checked | Not now |
| The CLI as a subprocess | `docs/research-desktop.md`, option C | Already rejected there |

The same C ABI is route D for C# (section 4). It is hand-written `extern "C"`
code, so that crate cannot keep `unsafe_code = "forbid"` the way `ketch-ffi`
does (its unsafe comes from macro expansions); it needs its own
`[lints.rust]` with `deny` and a scoped `allow`, which `ketch-ffi`'s header
already names as the pattern.

## Recommendation

- **macOS:** finish F12 on the real core after the contract fixes (D1, D2),
  then notifications, deep links and localisation (D7–D9).
- **Windows:** C# + WinUI 3 as decided; the binding through
  `uniffi-bindgen-cs` built from PR #176 pinned by commit (D10), falling back
  to the C ABI or UniFFI 0.31.2; an app shell on a fake core can start in
  parallel (D11).
- **Linux:** Vala + GTK 4 + libadwaita + Blueprint, following the GNOME HIG,
  over a `ketch-capi` C ABI with JSON records and a hand-written VAPI
  (D15, D16). KDE/Qt is not reachable from Vala.
- **Shared:** fix the contract once (D1–D3), share fixtures (D4), tokens
  (D5); string conventions wait with localisation (decision 7); no shared Rust view-model layer for now.

## Open decisions for the creator

1. **Linux toolkit, given Vala.** Decided by the creator (2026-10-01): Vala
   with GTK 4 + libadwaita, following the GNOME HIG; KDE/Qt is not pursued.
2. **Linux markup.** Decided by the creator (2026-10-01): Blueprint, pinned
   as a Meson subproject; GtkBuilder XML is not used.
3. **Linux core access.** Decided by the creator (2026-10-01): `ketch-capi`
   with JSON records and a hand-written VAPI. Hand-written `unsafe` is allowed
   in that crate only (`deny` with scoped `allow`s, each with a `SAFETY`
   comment); every other crate keeps `forbid`.
4. **C# binding route.** Decided by the creator (2026-10-01): B (PR #176
   pinned by commit) first; if it fails against UniFFI 0.32.2, D (the C ABI
   from `ketch-capi`). C, the downgrade to UniFFI 0.31.2, is not taken.
5. **Shared app layer.** Decided by the creator (2026-10-01): none now; thin
   view-models per app and D4's fixtures. Revisit if the same event-fold fix
   lands in two apps.
6. **Brand across platforms.** Decided by the creator (2026-10-01): accent,
   status colours, spacing, radii and type scale from `tokens.json` on
   Windows and Linux, with native surfaces; the token source moves from
   `desktop/macos/design/` to `desktop/design/`.
7. **Localisation.** Decided by the creator (2026-10-01): no localisation
   for now; the apps ship in English. D6 moved to `ideas.md`, and D9 is the
   VoiceOver pass only.
8. **Linux tray.** Deferred by the creator (2026-10-01): no tray, relying on
   the Background portal. StatusNotifierItem through
   `libayatana-appindicator-glib` (GPL-3.0, so only the GPL build of ketch's
   triple licence) is kept in `ideas.md`.
9. **Crash reporting.** Decided by the creator (2026-10-01): none for now;
   no crash-reporting SDK in any app.
10. **Deep links.** Decided by the creator (2026-10-01): a `ketch://` link
    only opens a package page; it never starts an install, even with a
    confirmation.

R10's open decisions on Windows distribution, the Windows App SDK licence
against the GPL build, and Flatpak against distribution packages still stand
(`docs/research-desktop-windows-linux.md`).

## Tasks

Added to `plan.md` as `todo`, with the prefix `D` (desktop): the `F` numbers
run to F17 across `plan.md` and `done.md`, and a separate prefix keeps the
desktop work readable as one group. F12, F13 and F14 keep their ids.

| Id | Group | Title |
| --- | --- | --- |
| D1 | Shared | `ketch-ffi`: foreign traits and per-operation callbacks |
| D2 | Shared | `ketch-ffi`: records and operations the apps need |
| D3 | Shared | `ketch-ffi`: the remaining CLI operations |
| D4 | Shared | Contract fixtures for every app's fake core |
| D5 | Shared | Design tokens for XAML and GTK |
| D6 | Shared | String keys and glossary for three apps (deferred to `ideas.md`, decision 7) |
| D7 | macOS | Update notifications |
| D8 | macOS | `ketch://` links |
| D9 | macOS | VoiceOver pass |
| D10 | Windows | C# binding for `ketch-ffi` |
| D11 | Windows | WinUI 3 app shell on a fake core |
| D12 | Windows | The app on the real core |
| D13 | Windows | Tray icon, notifications, start at login, links |
| D14 | Windows | Release pipeline |
| D15 | Linux | `ketch-capi`: a C ABI and VAPI for Vala |
| D16 | Linux | Vala + GTK 4 app shell on a fake core |
| D17 | Linux | The app on the real core |
| D18 | Linux | Notifications, background and autostart |
| D19 | Linux | Packaging and release |

## Unverified

- No Markdown rendering control was checked for WinUI 3 or GTK 4.
- That WinUI 3 has no notification-area control rests on open feature
  requests, not on documentation [W6].
- GNOME Shell showing no StatusNotifierItem icons without an extension: no
  primary source (as in R10).
- The `.desktop` `x-scheme-handler` mechanism for URL schemes on Linux was
  not checked against its specification.
- PR #176's claims (tests passing, checksums handled) are its author's.
- That macOS keeps one app instance by default was not checked.
- Which `blueprint-compiler` and `valac` versions the shipped GNOME 51 SDK
  contains was inferred from `gnome-build-meta` (`vala.bst` tracks `main`),
  not read from the built runtime [L7].
- No crash-reporting SDK for Vala was looked for.

## Sources

Checked 2026-10-01 unless a date is given.

macOS:

- **M1** `UNUserNotificationCenter.requestAuthorization`, macOS 10.14+:
  https://developer.apple.com/documentation/usernotifications/unusernotificationcenter/requestauthorization(options:completionhandler:)
- **M2** `MenuBarExtra`, macOS 13+:
  https://developer.apple.com/documentation/swiftui/menubarextra;
  `.window` style:
  https://developer.apple.com/documentation/swiftui/menubarextrastyle/window
- **M3** `SMAppService.mainApp`, macOS 13+:
  https://developer.apple.com/documentation/servicemanagement/smappservice/mainapp
- **M4** `onOpenURL`:
  https://developer.apple.com/documentation/swiftui/view/onopenurl(perform:);
  `CFBundleURLTypes`:
  https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleurltypes
- **M5** String Catalog:
  https://developer.apple.com/documentation/xcode/localizing-and-varying-text-with-a-string-catalog;
  export as XLIFF:
  https://developer.apple.com/documentation/xcode/exporting-localizations
- **M6** `NavigationSplitView`, macOS 13+:
  https://developer.apple.com/documentation/swiftui/navigationsplitview
- **M7** `accessibilityLabel`:
  https://developer.apple.com/documentation/swiftui/view/accessibilitylabel(_:);
  `colorSchemeContrast`:
  https://developer.apple.com/documentation/swiftui/environmentvalues/colorschemecontrast;
  `accessibilityReduceTransparency`:
  https://developer.apple.com/documentation/swiftui/environmentvalues/accessibilityreducetransparency
- **M8** `MXMetricManager`, macOS 12+, deprecated at 27.2:
  https://developer.apple.com/documentation/metrickit/mxmetricmanager;
  `MetricManager`, macOS 27+:
  https://developer.apple.com/documentation/metrickit/metricmanager
- **M9** Sparkle 2.10.0, 2026-09-13:
  https://github.com/sparkle-project/Sparkle/releases/tag/2.10.0
- **M10** Apple HIG: https://developer.apple.com/design/human-interface-guidelines/sidebars,
  https://developer.apple.com/design/human-interface-guidelines/the-menu-bar#Menu-bar-extras,
  https://developer.apple.com/design/human-interface-guidelines/materials,
  https://developer.apple.com/design/human-interface-guidelines/sheets,
  https://developer.apple.com/design/human-interface-guidelines/notifications,
  https://developer.apple.com/design/human-interface-guidelines/sf-symbols,
  https://developer.apple.com/design/human-interface-guidelines/typography
- **M12** Sentry Cocoa 9.30.0, 2026-09-30, MIT:
  https://github.com/getsentry/sentry-cocoa/releases/tag/9.30.0; with no DSN
  nothing is sent:
  https://docs.sentry.io/platforms/apple/configuration/options/#dsn

Windows:

- **W1** Windows App SDK 2.5.1, 2026-09-16:
  https://api.nuget.org/v3-flatcontainer/microsoft.windowsappsdk/index.json
- **W2** .NET 10.0.12 LTS, 2026-09-08, end of support 2028-11-14:
  https://builds.dotnet.microsoft.com/dotnet/release-metadata/releases-index.json
- **W3** App notifications (elevated apps cannot use them):
  https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/;
  unpackaged use:
  https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-dotnet
- **W4** `ActivationRegistrationManager` (`RegisterForProtocolActivation`,
  `RegisterForStartupActivation`; page updated 2026-07-28):
  https://learn.microsoft.com/en-us/windows/windows-app-sdk/api/winrt/microsoft.windows.applifecycle.activationregistrationmanager;
  rich activation:
  https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/applifecycle/applifecycle-rich-activation
- **W5** Run keys:
  https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys
- **W6** `Shell_NotifyIcon`:
  https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw;
  open requests for a WinUI tray icon:
  https://github.com/microsoft/microsoft-ui-xaml/issues/2020 (2020-02-24),
  https://github.com/microsoft/WindowsAppSDK/issues/713 (2021-04-08)
- **W7** System backdrops:
  https://learn.microsoft.com/en-us/windows/apps/develop/ui/system-backdrops;
  Mica: https://learn.microsoft.com/en-us/windows/apps/design/style/mica;
  Acrylic: https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic
- **W8** Title bar, `TitleBar` control since Windows App SDK 1.7:
  https://learn.microsoft.com/en-us/windows/apps/develop/title-bar
- **W9** `NavigationView`:
  https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/navigationview;
  navigation basics:
  https://learn.microsoft.com/en-us/windows/apps/design/basics/navigation-basics
- **W10** Dialogs:
  https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/dialogs-and-flyouts/dialogs;
  `InfoBar`: https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/infobar;
  progress:
  https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/progress-controls
- **W11** Contrast themes:
  https://learn.microsoft.com/en-us/windows/apps/design/accessibility/high-contrast-themes;
  accessibility overview:
  https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility-overview
- **W12** MRT Core strings:
  https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/mrtcore/localize-strings
- **W13** Single instancing:
  https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/applifecycle/applifecycle-instancing
- **W14** Typography, Segoe UI Variable:
  https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/typography;
  Segoe Fluent Icons:
  https://learn.microsoft.com/en-us/windows/apps/design/iconography/segoe-fluent-icons-font
- **W15** `ResourceDictionary` and `ThemeDictionaries`:
  https://learn.microsoft.com/en-us/windows/apps/develop/platform/xaml/xaml-resource-dictionary
- **W16** Velopack 1.2.161, 2026-09-29, MIT:
  https://api.nuget.org/v3/registration5-gz-semver2/velopack/index.json;
  `winget upgrade`:
  https://learn.microsoft.com/en-us/windows/package-manager/winget/upgrade
- **W17** Artifact Signing (formerly Trusted Signing):
  https://learn.microsoft.com/en-us/azure/artifact-signing/overview;
  SmartScreen reputation:
  https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- **W18** Command-line WinUI build:
  https://learn.microsoft.com/en-us/windows/apps/get-started/start-here;
  templates 0.0.7-alpha, 2026-09-23:
  https://api.nuget.org/v3-flatcontainer/microsoft.windowsappsdk.winui.csharp.templates/index.json
- **W19** WER LocalDumps:
  https://learn.microsoft.com/en-us/windows/win32/wer/collecting-user-mode-dumps;
  Sentry .NET 6.12.0, 2026-10-01, MIT:
  https://api.nuget.org/v3/registration5-gz-semver2/sentry/index.json
- **W20** `uniffi-bindgen-cs` releases:
  https://github.com/NordSecurity/uniffi-bindgen-cs/releases; versioning:
  https://github.com/NordSecurity/uniffi-bindgen-cs#versioning; PR #176:
  https://github.com/NordSecurity/uniffi-bindgen-cs/pull/176; issue #183:
  https://github.com/NordSecurity/uniffi-bindgen-cs/issues/183 (GitHub API)
- **W21** UniFFI changelog (0.32.0, 2026-06-30):
  https://github.com/mozilla/uniffi-rs/blob/main/CHANGELOG.md; versions:
  https://crates.io/api/v1/crates/uniffi
- **W22** csbindgen 1.9.8, 2026-05-20: https://crates.io/api/v1/crates/csbindgen;
  `LibraryImport`:
  https://learn.microsoft.com/en-us/dotnet/standard/native-interop/pinvoke-source-generation
- **W23** `uniffi-bindgen-cs` at PR #176's head, checked 2026-10-03:
  https://github.com/dennisameling/uniffi-bindgen-cs/blob/0fc022aa1d73fb1dda91a778b63f2824d7dca58b/bindgen/Cargo.toml
  (`0.12.0+v0.32.0`),
  https://github.com/dennisameling/uniffi-bindgen-cs/blob/0fc022aa1d73fb1dda91a778b63f2824d7dca58b/Cargo.toml
  (uniffi 0.32.0),
  https://github.com/dennisameling/uniffi-bindgen-cs/blob/0fc022aa1d73fb1dda91a778b63f2824d7dca58b/docs/CONFIGURATION.md
  (`access_modifier`, `namespace`)

Linux:

- **L1** Vala tags (0.56.19, 2026-03-30) and commits:
  https://gitlab.gnome.org/api/v4/projects/GNOME%2Fvala/repository/tags,
  https://gitlab.gnome.org/api/v4/projects/GNOME%2Fvala/repository/commits?ref_name=main
- **L2** Vala bindings: https://docs.vala.dev/guides/bindings.html; writing a
  VAPI by hand:
  https://docs.vala.dev/guides/bindings/writing-a-vapi-manually.html;
  delegates:
  https://docs.vala.dev/guides/bindings/writing-a-vapi-manually/04-00-recognizing-vala-semantics-in-c-code/04-07-delegates.html;
  compact classes:
  https://docs.vala.dev/guides/bindings/writing-a-vapi-manually/04-00-recognizing-vala-semantics-in-c-code/04-05-compact-classes.html;
  VAPI from GIR:
  https://docs.vala.dev/guides/bindings/generating-a-vapi-with-gobject-introspection.html
- **L3** https://wiki.gnome.org/Projects/Vala/ListOfBindings,
  https://gitlab.gnome.org/GNOME/vala-extra-vapis;
  GitHub repository search "vala qt", 2026-10-01: no binding
- **L4** Kirigami setup (C++, Python, Rust):
  https://develop.kde.org/docs/getting-started/; cxx-qt 0.10.0: https://crates.io/api/v1/crates/cxx-qt; PySide6 6.11.2,
  2026-08-18: https://pypi.org/pypi/PySide6/json
- **L5** Blueprint tags (v0.22.2, 2026-07-10):
  https://gitlab.gnome.org/api/v4/projects/GNOME%2Fblueprint-compiler/repository/tags;
  docs: https://gnome.pages.gitlab.gnome.org/blueprint-compiler/; Meson:
  https://gnome.pages.gitlab.gnome.org/blueprint-compiler/setup.html
- **L6** GtkBuilder: https://docs.gtk.org/gtk4/class.Builder.html; GNOME
  Builder's Vala GTK 4 template (`window-gtk4.ui`):
  https://gitlab.gnome.org/GNOME/gnome-builder/-/tree/main/src/plugins/meson-templates/resources/src
- **L7** GNOME SDK definition:
  https://gitlab.gnome.org/GNOME/gnome-build-meta/-/raw/master/elements/sdk.bst
  (`sdk/vala.bst`),
  https://gitlab.gnome.org/GNOME/gnome-build-meta/-/raw/master/elements/sdk-platform.bst
  (`sdk/blueprint-compiler.bst`, `sdk/json-glib.bst`)
- **L8** GTK 4.24.1 (2026-10-01):
  https://gitlab.gnome.org/api/v4/projects/GNOME%2Fgtk/repository/tags;
  libadwaita 1.10.0 (2026-09-14):
  https://gitlab.gnome.org/api/v4/projects/GNOME%2Flibadwaita/repository/tags
- **L9** libadwaita reference, "Available since"
  per class: https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/;
  CSS variables:
  https://gnome.pages.gitlab.gnome.org/libadwaita/doc/main/css-variables.html,
  introduced in 1.6 (`NEWS`, "Version 1.6.alpha"):
  https://gitlab.gnome.org/GNOME/libadwaita/-/raw/main/NEWS
- **L10** GTK custom properties, "Starting with 4.16":
  https://docs.gtk.org/gtk4/css-properties.html
- **L11** GNOME HIG: https://developer.gnome.org/hig/patterns/nav/sidebars.html,
  https://developer.gnome.org/hig/patterns/containers/header-bars.html,
  https://developer.gnome.org/hig/patterns/containers/boxed-lists.html,
  https://developer.gnome.org/hig/patterns/feedback/dialogs.html,
  https://developer.gnome.org/hig/patterns/feedback/notifications.html,
  https://developer.gnome.org/hig/patterns/feedback/toasts.html,
  https://developer.gnome.org/hig/patterns/feedback/banners.html,
  https://developer.gnome.org/hig/guidelines/accessibility.html; no page on
  status icons or a tray
- **L12** Background portal v2:
  https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Background.html;
  Notification portal v2:
  https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Notification.html
- **L13** libportal 0.11.0, 2026-09-12, Vala bindings on by default:
  https://github.com/flatpak/libportal/blob/main/meson_options.txt
- **L14** `GNotification`: https://docs.gtk.org/gio/class.Notification.html;
  `GApplication`: https://docs.gtk.org/gio/class.Application.html; XDG
  autostart 0.5: https://specifications.freedesktop.org/autostart-spec/latest/
- **L15** StatusNotifierItem:
  https://specifications.freedesktop.org/status-notifier-item/latest/;
  libayatana-appindicator 0.6.0 (GTK 2/3 only):
  https://github.com/AyatanaIndicators/libayatana-appindicator;
  libayatana-appindicator-glib 2.0.3, 2026-06-14, GPL-3.0:
  https://github.com/AyatanaIndicators/libayatana-appindicator-glib; GNOME
  Shell's background apps:
  https://gitlab.gnome.org/GNOME/gnome-shell/-/raw/main/js/ui/status/backgroundApps.js
- **L16** GTK accessibility: https://docs.gtk.org/gtk4/section-accessibility.html
- **L17** Meson and Vala: https://mesonbuild.com/Vala.html; i18n:
  https://mesonbuild.com/i18n-module.html; Meson 1.12.1:
  https://pypi.org/pypi/meson/json
- **L18** elementary developer docs: https://docs.elementary.io/develop
- **L19** json-glib 1.10.8, 2025-09-13, LGPL-2.1-or-later:
  https://gitlab.gnome.org/GNOME/json-glib/-/raw/main/meson.build
- **L20** cbindgen 0.29.4, 2026-06-09, MPL-2.0:
  https://crates.io/api/v1/crates/cbindgen; safer-ffi:
  https://crates.io/api/v1/crates/safer-ffi/versions
- **L21** UniFFI third-party bindings:
  https://github.com/mozilla/uniffi-rs/blob/main/README.md#third-party-foreign-language-bindings;
  uniffi-bindgen-cpp (tag v0.9.0+v0.29.4):
  https://github.com/NordSecurity/uniffi-bindgen-cpp

Shared:

- **X1** Callback interfaces "(soft) deprecated":
  https://github.com/mozilla/uniffi-rs/blob/main/docs/manual/src/types/callback_interfaces.md;
  foreign traits:
  https://github.com/mozilla/uniffi-rs/blob/main/docs/manual/src/foreign_traits.md
- **X2** Style Dictionary 5.5.5, Apache-2.0:
  https://registry.npmjs.org/style-dictionary/latest; built-in formats (none
  for XAML or GTK):
  https://styledictionary.com/reference/hooks/formats/predefined/
- **X4** Figma variable code syntax (`WEB`, `ANDROID`, `iOS`):
  https://developers.figma.com/docs/plugins/api/Variable/#codesyntax
- **X5** Weblate formats (weblate-2026.10):
  https://docs.weblate.org/en/latest/formats.html
- **X6** translate-toolkit 3.20.0:
  https://pypi.org/pypi/translate-toolkit/json; formats:
  https://docs.translatehouse.org/projects/translate-toolkit/en/latest/formats/
- **X8** `sentry` crate 0.49.3, MIT; no DSN, nothing sent
  (https://docs.sentry.io/platforms/rust/configuration/options/#dsn):
  https://crates.io/api/v1/crates/sentry
- **X10** UniFFI async functions:
  https://github.com/mozilla/uniffi-rs/blob/main/docs/manual/src/futures.md
