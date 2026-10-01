# Figma file

The Liquid glass design lives in Figma:
[ketch for macOS — Liquid glass](https://www.figma.com/design/v7OJLmQEyCFbJ63uSYpJ9g).

`tokens.json` is the source of truth. The Figma variables mirror it, and each
one carries its Swift name as iOS code syntax, so Dev Mode shows
`Tokens.Colors.Glass.window` rather than a hex value. When a token changes,
change `tokens.json` first, then the variable in Figma.

## Pages

| Page | What it holds |
| --- | --- |
| Components | Icons, app icon, badge, button, nav item, search field, progress, segment, swatch, package row |
| Light | Nine screens: Installed, Discover, Updates, Package detail, Activity, Doctor, Settings · Appearance, Uninstall sheet, Menu bar extra |
| Dark | The same nine screens with the `Color` collection set to Dark |

The prototype starts at Installed. The sidebar items link the screens, a row
opens its package detail, and Uninstall opens the sheet.

## Colour variables

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
