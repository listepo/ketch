# Ketch for macOS

A native SwiftUI app for ketch: installed and registry packages, install,
upgrade, uninstall, changelogs, doctor, and a menu-bar extra with pending
upgrades. It manages the same ketch root as the CLI (`~/.ketch`, or
`KETCH_ROOT`).

**Status:** the app runs on `FakeKetchCore`, canned data with simulated
progress. The real core arrives with `ketch-ffi` (R9); see
[Wiring the real core](#wiring-the-real-core).

![The Installed section](screenshot.jpg)

## Requirements

- macOS 26 or later (the deployment target), Xcode 26 or later.
- XcodeGen, pinned in the repository's `mise.toml`: `mise install`.

## Build and test

From the repository root:

```bash
just macos-app    # generate Ketch.xcodeproj, build a universal (arm64 + x86_64) Debug app
just macos-test   # Swift Testing unit tests, then the UI smoke test
open desktop/macos/build/Build/Products/Debug/Ketch.app
```

`project.yml` is the project; `Ketch.xcodeproj` and `build/` are generated
and gitignored. To work in Xcode, run `just macos-project` and open the
generated project. Local builds are unsigned (ad-hoc); signed, notarised
builds come only from the release workflow, see [Releases](#releases).

To run against a throwaway root, start the binary directly (`open` does not
pass environment variables to the app):

```bash
KETCH_ROOT=/tmp/ketch-scratch desktop/macos/build/Build/Products/Debug/Ketch.app/Contents/MacOS/Ketch
```

Sources are formatted with `swift format` (in the Xcode toolchain) using
`.swift-format`: `xcrun swift-format format -i -r Ketch KetchTests KetchUITests`
from this directory; CI runs `lint --strict`.

## Layout

| Path | Owns |
| --- | --- |
| `project.yml` | the XcodeGen spec: targets, Swift 6 strict concurrency, deployment target 26.0, the Sparkle package |
| `Ketch/Info.plist` | Sparkle's keys, merged into the generated Info.plist: the feed URL and the EdDSA public key |
| `ExportOptions.plist` | `xcodebuild -exportArchive` options for a Developer ID release |
| `Ketch/Store/AppUpdater.swift` | the app's own updates through Sparkle, behind "Check for Updates…" |
| `Ketch/Core/KetchCoreProtocol.swift` | the core's API as the app sees it: records, events, `Reporter`, `Decider`, `CancelToken`, `KetchError` — mirroring R9 |
| `Ketch/Core/FakeKetchCore.swift` | the stand-in core for previews, tests and, until R9, the app |
| `Ketch/Core/CoreFactory.swift` | the one place that picks the core |
| `Ketch/Core/KetchRoot.swift` | the root, resolved like the CLI (`KETCH_ROOT`, empty means unset) |
| `Ketch/Store/KetchStore.swift` | the `@Observable` main-actor store: state, operations, callbacks, update checks |
| `Ketch/Store/AppSettings.swift` | the app's own preferences (UserDefaults) and Open at login |
| `Ketch/Store/Appearance.swift` | Settings -> Appearance as values: tint, glass style, accent, wash, and how Reduce Transparency and Increase Contrast override them |
| `Ketch/Views/` | the window sections, the menu-bar panel, Settings, About, the glass styling |
| `Ketch/Views/Theme.swift` | the views' spacing, radius, shadow and status-colour roles, mapped onto the generated tokens |
| `Ketch/Views/Glass.swift` | the `appearance` environment value, `ketchAppearance()`, glass cards and the backdrop wash |
| `design/generated/Tokens.swift` | generated from `design/tokens.json` (`just design-tokens`) and compiled into the app; see `DESIGN.md` |
| `KetchTests/` | Swift Testing tests of the store, settings and root on the fake core |
| `KetchUITests/` | one XCUITest smoke test: launch against a scratch `KETCH_ROOT` |

## Architecture

- **One store.** `KetchStore` is `@MainActor @Observable` and owns
  everything the UI shows. The window, the menu bar and Settings share it.
- **Core calls off the main actor.** The core is synchronous (UniFFI
  exports sync methods), so each call runs on a global dispatch queue and is
  bridged back with a continuation — not in Swift's cooperative pool, which a
  call blocked on the network or on the user would starve.
- **Reporter.** Core events hop to the main actor through
  `DispatchQueue.main`, which keeps their order; the store turns them into
  per-package stage and progress and the Activity log.
- **Decider.** `chooseBinary` blocks the core's thread on a semaphore while
  the main actor shows a sheet; Cancel declines the open question, so the
  core is never left waiting.
- **Errors.** `busy(pid:)` shows "ketch is running in another process" with
  Retry, which re-runs the refused operation; `cancelled` is logged, not
  alerted; anything else is an alert.
- **Shared root.** The store re-reads installed and outdated packages when
  the window becomes active, so CLI changes show up, and on a timer
  (Settings → Check for updates) for the menu-bar count.
- **Not sandboxed.** The app manages `~/.ketch`, links into `/Applications`
  and shares the CLI's lock, none of which an App Sandbox container allows.

### Look: Liquid Glass

The UI uses macOS 26's system Liquid Glass, never a drawn imitation, so
Reduce Transparency and Increase Contrast apply without app code: cards and
badges use `glassEffect(_:in:)` with `Glass.regular` (tinted or `interactive`
where it helps), grouped inside `GlassEffectContainer`; actions use the
`.glass` and `.glassProminent` button styles; the sidebar and toolbars get
glass from the system. Depth comes from a soft accent backdrop behind the
glass and a shadow under each card. The menu-bar extra uses
`.menuBarExtraStyle(.window)` so it can be the same glass panel.

API names were checked against the macOS 27.0 SDK's
`SwiftUICore.swiftmodule/arm64e-apple-macos.swiftinterface` and
`SwiftUI.swiftmodule/arm64e-apple-macos.swiftinterface` (Xcode 27.0,
27A266a, 2026-10-01): `Glass`, `glassEffect(_:in:)`, `GlassEffectContainer`,
`Glass.interactive(_:)` and `Glass.tint(_:)` are `@available(macOS 26.0)`;
`GlassButtonStyle` and `GlassProminentButtonStyle` back `.glass` and
`.glassProminent`. Apple's documentation:
[glassEffect(_:in:)](https://developer.apple.com/documentation/swiftui/view/glasseffect(_:in:)),
[GlassEffectContainer](https://developer.apple.com/documentation/swiftui/glasseffectcontainer),
[Applying Liquid Glass to custom views](https://developer.apple.com/documentation/swiftui/applying-liquid-glass-to-custom-views),
[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice).

## Releases

Releases are made by `.github/workflows/desktop-release.yml`, dispatched by
hand with a version:

```bash
gh workflow run desktop-release.yml --ref main -f version=0.1.0
```

It builds a universal Release app, signs it with the Developer ID
certificate and the hardened runtime, notarises and staples it, packs it into
`Ketch-X.Y.Z.dmg` (hdiutil, with an `/Applications` link), signs, notarises
and staples the image, checks both with `spctl`, and publishes the image, its
`.sha256` and the updated appcast as the GitHub release `desktop-vX.Y.Z`. The
release notes are the commits under `desktop/` and `crates/ketch-ffi/` since
the last app release (`desktop/cliff.toml`). App releases are never marked
latest, because the CLI's installers follow `/releases/latest`; the
repository's `AGENTS.md` ("macOS app releases") has why, the secrets it needs,
and what to do when a run fails half-way.

The app version is the workflow input, set as both `CFBundleShortVersionString`
and `CFBundleVersion`; `MARKETING_VERSION` in `project.yml` only labels local
builds.

Before the first release, the creator generates the update key once with the
`generate_keys` tool that ships with Sparkle (after a build it is in
`build/SourcePackages/artifacts/sparkle/Sparkle/bin/`): commit the printed
public key as `SUPublicEDKey` in `Ketch/Info.plist`, and store
`generate_keys -x <file>`'s output as the `SPARKLE_ED_PRIVATE_KEY` secret.

Until R9 ships the ketch-ffi XCFramework, the workflow's XCFramework step is
switched off (`TODO(R9)`), and a release carries the app on `FakeKetchCore`.

### Updates

Ketch.app updates itself with [Sparkle](https://sparkle-project.org) 2.10.0
(SwiftPM, pinned exactly in `project.yml`). The feed is
`https://github.com/pyrlyn/ketch/releases/download/desktop-appcast/appcast.xml`:
one file on the `desktop-appcast` prerelease that every app release replaces,
so the URL never changes and never depends on which release is latest. The
feed and every archive are EdDSA-signed (`SURequireSignedFeed`,
`SUVerifyUpdateBeforeExtraction`). Only a Release build with a real public
key starts the updater; Debug builds and test runs leave "Check for Updates…"
disabled.

`just macos-appcast` builds the app and runs the release's disk-image and
appcast scripts on it with a throwaway key: two releases in a row, a
tampered signature and a mismatched key.

## Wiring the real core

When R9 lands, `desktop/macos/KetchCore/` is a local Swift package with the
XCFramework and the UniFFI bindings. Then:

1. Add it to `project.yml` (`packages: KetchCore: { path: KetchCore }` and a
   `package: KetchCore` dependency on the `Ketch` target).
2. Add `Ketch/Core/LiveKetchCore.swift`: a `KetchCoreProtocol` over the
   generated `KetchCore` object that converts its records and events to the
   types in `KetchCoreProtocol.swift`, maps its `KetchError`, wraps the
   app's `Reporter`/`Decider` in the generated foreign traits per call, and
   forwards `CancelToken.onCancel` to the FFI token.
3. Return it from `CoreFactory.make`.

Nothing in the store, the views or the tests changes; the fake stays for
previews and tests. Two things to recheck against the generated API: the
record fields (`InstalledPackage`, `Finding`), and whether doctor exposes fix
actions (the Doctor view names fixes but cannot run them yet).
