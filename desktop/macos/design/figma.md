# Figma file

The desktop design for all three apps lives in one Figma file:
[ketch for macOS — Liquid glass](https://www.figma.com/design/v7OJLmQEyCFbJ63uSYpJ9g).
The file keeps its old name so existing links still work; it now holds macOS
(SwiftUI, Liquid Glass), Windows (C# + WinUI 3, Fluent) and Linux (Vala +
GTK 4 + libadwaita, GNOME HIG).

`tokens.json` is the source of truth. The Figma variables mirror it, and each
one carries its Swift name as iOS code syntax, so Dev Mode shows
`Tokens.Colors.Glass.window` rather than a hex value. When a token changes,
change `tokens.json` first, then the variable in Figma.

This file stays in `desktop/macos/design/` for now. Task D5 moves the token
source and this note to `desktop/design/`, since the tokens now serve every
platform; moving them here would collide with that task.

## Pages

| Page | Id | What it holds |
| --- | --- | --- |
| Shared · Foundations | `16:65` | Read me board (`19:3`), the platform token matrix (`19:69`), and the components every platform draws the same way: stand-in glyphs, the monogram app icon (`4:48`), the status badge (`4:68`) |
| Shared · Screen specs | `16:66` | Wireframe boards for each screen, flow and state (`21:11` … `23:8`), and the component mapping table (`23:61`) from `docs/research-desktop-platforms.md` §3b–3c |
| macOS · Components | `0:1` | Icons, button, nav item, search field, progress, segment, swatch, package row |
| macOS · Light | `4:2` | Twenty frames, `01`–`19` (see below) |
| macOS · Dark | `4:3` | The same twenty frames with `Color` set to Dark |
| Windows · Components | `16:67` | `Win/Button` `42:15`, `Win/NavItem` `42:32`, `Win/ListRow` `42:33`, `Win/InfoBar` `42:66`, `Win/TextBox` `42:67`, `Win/ToggleSwitch` `42:77`, `Win/ProgressBar` `42:78` |
| Windows · Light | `16:68` | `W01`–`W08` |
| Windows · Dark | `16:69` | The same frames with `Color` set to Dark |
| Linux · Components | `16:70` | `Adw/Button` `51:17`, `Adw/SidebarRow` `51:32`, `Adw/ActionRow` `51:33`, `Adw/Banner` `51:42`, `Adw/Toast` `51:46`, `Adw/Switch` `51:57`, `Adw/SwitchRow` `51:58`, `Adw/SearchEntry` `51:64` |
| Linux · Light | `16:71` | `L01`–`L07` |
| Linux · Dark | `16:72` | The same frames with `Color` set to Dark |

Each page sets its `Platform` mode explicitly, and each Dark page also sets
`Color` to Dark. A frame dropped onto another page therefore takes that page's
theme and idiom without being rebuilt.

### macOS frames

Synced with the app as built in F17: progress runs in a bottom bar without
switching to Activity; Discover opens on a "Not installed yet" hero, quick
search chips and an "In the registry" list; Doctor names the fix as text,
with no Fix buttons; no sizes and no changelog headlines; two-letter
monograms (ketch is "ke"); the native sidebar toggle and back button; the busy
message ends "Retry when it finishes"; Settings holds General and Appearance;
the terminal block is smoke-tinted; update badges are orange.

| Frame | Light | Dark |
| --- | --- | --- |
| 01 Installed | `6:10` | `41:513` |
| 02 Discover | `6:162` | `41:555` |
| 03 Updates | `6:309` | `41:612` |
| 04 Package detail | `7:219` | `41:653` |
| 05 Activity | `7:334` | `41:719` |
| 06 Doctor | `7:459` | `41:793` |
| 07 Settings · Appearance | `8:332` | `41:875` |
| 07b Settings · General | `30:119` | `41:1060` |
| 08 Uninstall sheet | `8:486` | `41:963` |
| 09 Menu bar extra | `8:537` | `41:1013` |
| 10 Binary choice | `33:8` | `41:1116` |
| 11 Stop running processes | `33:174` | `41:1194` |
| 12 Ambiguous bin glob | `33:294` | `41:1256` |
| 13 Busy · locked | `36:8` | `41:1315` |
| 14 Activity detail | `36:158` | `41:1363` |
| 15 Package detail · More menu | `37:8` | `41:1480` |
| 16 Rollback confirm | `37:139` | `41:1566` |
| 17 Package detail · Pinned | `37:259` | `41:1641` |
| 18 Update notification | `37:374` | `41:1716` |
| 19 Update all confirm | `40:85` | `41:1778` |

Prototype flows: `ketch` (Installed), `Pin and rollback`, `Ambiguous bin glob`,
`Busy` and `Update notification`, each with a `(dark)` twin.

### Windows frames

Fluent idiom: a custom TitleBar with back button and search box, NavigationView
with Settings pinned at the bottom, content on the layer fill with an 8 px
top-left corner, cards, InfoBar, SettingsCard rows, ContentDialog over smoke,
an Acrylic tray flyout and an AppNotification toast.

| Frame | Light | Dark |
| --- | --- | --- |
| W01 Installed | `44:8` | `47:1578` |
| W02 Discover | `44:187` | `47:1626` |
| W03 Updates (InfoBar for Busy) | `44:339` | `47:1681` |
| W04 Package detail | `45:263` | `47:1730` |
| W05 Settings | `45:407` | `47:1807` |
| W06 Binary choice (ContentDialog) | `46:368` | `47:1887` |
| W07 Tray flyout | `46:529` | `47:1968` |
| W08 Update notification | `46:601` | `47:2028` |

Flows: `Windows`, `Windows tray`, `Windows notification`, plus `(dark)` twins.

### Linux frames

Adwaita idiom, per the R11 decision (GNOME HIG; KDE/Qt is out):
AdwNavigationSplitView with a header bar on each pane, AdwClamp, boxed lists
of AdwActionRows, AdwBanner, AdwToast, AdwPreferencesDialog and AdwAlertDialog.
GNOME has no tray, so L07 shows ketch as a background app (XDG Background
portal) and a GNotification instead.

| Frame | Light | Dark |
| --- | --- | --- |
| L01 Installed | `53:8` | `56:1359` |
| L02 Discover | `53:189` | `56:1418` |
| L03 Updates (AdwBanner for Busy) | `53:336` | `56:1477` |
| L04 Package detail | `54:248` | `56:1526` |
| L05 Preferences (AdwPreferencesDialog) | `54:385` | `56:1605` |
| L06 Binary choice (AdwAlertDialog) | `54:529` | `56:1683` |
| L07 Background app and notification | `54:687` | `56:1769` |

Flows: `Linux`, `Linux notification`, plus `(dark)` twins.

## needs core

A frame that shows something ketch-ffi does not expose yet carries a red
caption 36 px above it, `… · needs core <gap>: <what>`, and the same text as a
Dev Mode annotation on the frame. The gap ids are those of
`docs/research-desktop-platforms.md` (R11, PR #211): G1 `stop_processes` outside the app
protocol (and no process name in `Holder`), G2 changelog range, G3 held
packages missing from `outdated()`, G4 latest version in search, G5 uninstall
without a cancel token, G6 history, pin, unpin, rollback, doctor fix and
config not exported. Notifications, the tray and the background app are the
app's own work (D7), so their captions say "app-side" in the neutral colour.

## Variable collections and modes

| Collection | Modes | Holds |
| --- | --- | --- |
| `Color` | Light, Dark | Brand tokens (`accent/*`, `status/*`), the macOS glass family, `win/*` and `adw/*` |
| `Platform` | macOS, Windows, Linux | `surface/*`, `stroke/*`, `ink/*` aliasing into the right `Color` family; `radius/*`, `space/*`, `size/*`, `font/*` per platform |
| `Dimension` | Default | The macOS dimensions from `tokens.json` |

Two independent axes need 2 + 3 modes, and every brand value is written once.
One combined collection (macOS Light … Linux Dark) would need six modes; that
fits the plan's limit of 10 modes per collection, but would repeat every
brand value three times and grow with each new platform.

Per R11, Windows and Linux take only the brand from `tokens.json` — accent,
status colours, spacing, radii and type scale — on native surfaces: Mica and
Acrylic on Windows, flat Adwaita on Linux. Glass, wash and elevation stay
macOS-only.

### Code syntax

- **iOS** — the Swift token name (`Tokens.*`), as before.
- **WEB** — the libadwaita CSS variable where one exists, so Dev Mode shows
  the GTK name: `var(--accent-bg-color)`, `var(--card-bg-color)`. Brand values
  with no libadwaita variable get a `--ketch-` name the GTK app defines in its
  stylesheet (`var(--ketch-update-color)`).
- Figma has no XAML slot, so the WinUI resource key is in each `win/*` and
  `Platform` variable's description.

### Platform variables

| Variable | macOS | Windows (XAML) | Linux (GTK, WEB syntax) |
| --- | --- | --- | --- |
| `surface/window` | `glass/base` | `win/mica` — Mica over `SolidBackgroundFillColorBaseBrush` | `var(--window-bg-color)` |
| `surface/sidebar` | `glass/hi` | NavigationView pane over Mica | `var(--sidebar-bg-color)` |
| `surface/content` | `glass/base` | `win/layer` — `LayerFillColorDefaultBrush` | `var(--window-bg-color)` |
| `surface/header` | `glass/hi` | TitleBar over Mica | `var(--headerbar-bg-color)` |
| `surface/card` | `glass/lo` | `CardBackgroundFillColorDefaultBrush` | `var(--card-bg-color)` |
| `surface/control` | `glass/top` | `ControlFillColorDefaultBrush` | button background, alpha 0.1 |
| `surface/selected` | `glass/top` | `SubtleFillColorSecondaryBrush` | button background |
| `surface/dialog` | `glass/sheet` | `ContentDialogBackground` | `var(--dialog-bg-color)` |
| `surface/flyout` | `glass/sheet` | `AcrylicInAppFillColorDefaultBrush` | `var(--popover-bg-color)` |
| `surface/scrim` | `scrim` | `SmokeFillColorDefaultBrush` | `scrim` |
| `stroke/card` | `line/edge` | `CardStrokeColorDefaultBrush` | `var(--border-color)` |
| `stroke/divider` | `line/hair` | `DividerStrokeColorDefaultBrush` | `var(--border-color)` |
| `ink/text` | `ink/primary` | `TextFillColorPrimaryBrush` | `var(--window-fg-color)` |
| `ink/dim` | `ink/secondary` | `TextFillColorSecondaryBrush` | `.dim-label` |
| `radius/window`, `card`, `control`, `dialog` | 26, 20, 999, 26 | 8, 8 (`OverlayCornerRadius`), 4 (`ControlCornerRadius`), 8 | 12, 12, 6, 18 |
| `space/page`, `space/rows` | 20, 8 | 24, 4 | 24, 0 (one boxed list) |
| `size/row`, `control`, `sidebar` | 62, 28, 232 | 64, 32, 320 (`OpenPaneLength`) | 64, 34, 260 |
| `font/ui`, `font/mono` | SF Pro, SF Mono | Segoe UI Variable, Cascadia Mono | Adwaita Sans, Adwaita Mono |

### Fonts

Inter stands in for SF Pro, Segoe UI Variable and Adwaita Sans: Figma's
renderer cannot measure SF Pro, and the other two are not available to it
(Adwaita Sans is derived from Inter). Mono: JetBrains Mono for SF Mono and
Adwaita Mono, Cascadia Mono on Windows. The glyphs are stand-ins for SF
Symbols, Segoe Fluent Icons and Adwaita symbolic icons.

## Colour variables (macOS and brand)

Collection `Color`, modes Light and Dark.

| Figma variable | Token | Swift |
| --- | --- | --- |
| `glass/base` | `color.glass.window` | `Tokens.Colors.Glass.window` |
| `glass/hi` | `color.glass.regular` | `Tokens.Colors.Glass.regular` |
| `glass/lo` | `color.glass.frost` | `Tokens.Colors.Glass.frost` |
| `glass/top` | `color.glass.control` | `Tokens.Colors.Glass.control` |
| `glass/sheet` | `color.glass.elevated` | `Tokens.Colors.Glass.elevated` |
| `ink/primary` | `color.text.primary` | `Tokens.Colors.Text.primary` |
| `ink/secondary` | `color.text.secondary` | `Tokens.Colors.Text.secondary` |
| `line/hair` | `color.separator` | `Tokens.Colors.separator` |
| `line/edge` | `color.glass.stroke` | `Tokens.Colors.Glass.stroke` |
| `rim/hi` | `color.glass.highlight` | `Tokens.Colors.Glass.highlight` |
| `rim/lo` | `color.glass.shade` | `Tokens.Colors.Glass.shade` |
| `shadow/drop` | `color.shadow.drop` | `Tokens.Colors.Shadow.drop` |
| `shadow/ambient` | `color.shadow.window` | `Tokens.Colors.Shadow.window` |
| `scrim` | `color.scrim` | `Tokens.Colors.scrim` |
| `accent/default` | `color.accent.default` | `Tokens.Colors.Accent.default` |
| `accent/soft` | `color.accent.subtle` | `Tokens.Colors.Accent.subtle` |
| `accent/ink` | `color.accent.ink` | `Tokens.Colors.Accent.ink` |
| `accent/on` | `color.accent.on` | `Tokens.Colors.Accent.on` |
| `status/ok`, `status/ok-soft` | `color.status.installed`, `installedSubtle` | `Tokens.Colors.Status.installed`, `installedSubtle` |
| `status/update`, `status/update-soft` | `color.status.update`, `updateSubtle` | `Tokens.Colors.Status.update`, `updateSubtle` |
| `status/warning`, `status/warning-soft` | `color.status.warning`, `warningSubtle` | `Tokens.Colors.Status.warning`, `warningSubtle` |
| `status/error`, `status/error-soft` | `color.status.error`, `errorSubtle` | `Tokens.Colors.Status.error`, `errorSubtle` |
| `status/busy`, `status/busy-soft` | `color.status.busy`, `busySubtle` | `Tokens.Colors.Status.busy`, `busySubtle` |
| `wall/from` | `color.background.washStart` | `Tokens.Colors.Background.washStart` |
| `wall/to` | `color.background.washEnd` | `Tokens.Colors.Background.washEnd` |
| `wall/hill-near` | `color.background.washDeep` | `Tokens.Colors.Background.washDeep` |
| `wall/hill-far` | `color.background.washHill` | `Tokens.Colors.Background.washHill` |

The `wall/*` variables paint the desktop behind the window at full strength,
so the glass has something to refract. The app draws the same colours as a
wash at `opacity.wash` (0.14 by default, at most `opacity.washMax`) over the
user's real wallpaper.

Tokens with no Figma variable: `accent.pressed`, `text.tertiary`,
`background.base`, `glass.clear`, `glass.solidRegular`, `glass.solidElevated`,
`fill.*`, `focusRing` and the presets. Pressed, focus and Reduce Transparency
states are not drawn in Figma; the presets appear only as swatches on the
Appearance screen.

## Dimension variables

Collection `Dimension`, one mode.

| Figma variable | Token | Swift |
| --- | --- | --- |
| `radius/window` | `radius.xl` | `Tokens.Radius.xl` |
| `radius/panel` | `radius.lg` | `Tokens.Radius.lg` |
| `radius/card` | `radius.md` | `Tokens.Radius.md` |
| `radius/control` | `radius.sm` | `Tokens.Radius.sm` |
| `radius/capsule` | `radius.full` | `Tokens.Radius.full` |
| `space/xs` … `space/xxl` | `space.xs`, `sm`, `md`, `lg`, `xl`, `xxl` | `Tokens.Space.xs` … `Tokens.Space.xxl` |
| `pad/card` | `space.md` | `Tokens.Space.md` |
| `pad/window` | `space.xl` | `Tokens.Space.xl` |

## Styles

Text styles use Inter and JetBrains Mono, because Figma's cloud renderer cannot
measure SF Pro and SF Mono. The sizes and weights match `typography.*`; the app
uses the system fonts.

The effect styles `Glass/Window`, `Glass/Raised`, `Glass/Capsule` and
`Glass/Sheet` stack Figma's Glass effect, a drop shadow and two rim inner
shadows. In the app this is `.glassEffect` plus `elevation.*`; the Glass effect
only approximates the system material. `Glass/Reduced` is the drop shadow alone,
for Reduce Transparency.

## Badges

An update badge uses the `status.update` pair (orange), not the accent, so a
pending update reads differently from a selection or a primary button.
