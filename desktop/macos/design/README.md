# Design tokens for the macOS app

`tokens.json` is the one source of every colour, size, radius, font, shadow
and animation the app uses. Everything else is generated from it:

| Output | What it is | Who reads it |
| --- | --- | --- |
| `generated/Tokens.swift` | `Tokens.Colors…`, `Tokens.Space…`, `Tokens.Typography…` as SwiftUI values; colours follow light, dark and Increase Contrast without an asset catalog | the app (`desktop/macos/`) |
| `../DESIGN.md` front matter | the YAML block at the top, in the [DESIGN.md format](https://github.com/google-labs-code/design.md) | coding agents, the DESIGN.md linter |
| `preview.html`, between `BEGIN/END GENERATED TOKENS` | CSS variables and a token index | the review page |

The prose of `DESIGN.md` and the rest of `preview.html` are hand-written.

## Change a token

1. Edit `tokens.json`. It is [W3C DTCG 2025.10](https://www.designtokens.org/tr/2025.10/format/):
   - A colour is `{"colorSpace": "srgb", "components": [r, g, b], "alpha": a, "hex": "#rrggbb"}`
     with components in 0–1. `hex` is optional, but when present it must match
     the components — the generator refuses a mismatch, so change both.
   - Appearance variants go in
     `"$extensions": {"com.github.pyrlyn.ketch": {"dark": …, "highContrast": …, "highContrastDark": …}}`.
     A missing one falls back: high-contrast dark → dark → light,
     high-contrast → light.
   - An alias (`"{color.accent.default}"`) resolves in the same appearance it is
     read in, so a component that points at the accent gets the dark accent in
     dark mode.
   - Dimensions are `{"value": 16, "unit": "px"}`; durations
     `{"value": 180, "unit": "ms"}`.
2. Regenerate: `just design-tokens` (or `node desktop/macos/design/build.mjs`
   from the repository root after `just deps`).
3. Open `preview.html` in a browser and check both themes. The switches at the
   top toggle Light / Dark, Increase contrast, Reduce transparency and Reduce
   motion; a query string opens it in a given state, e.g.
   `preview.html?theme=dark&contrast=more`.
4. Run the checks: `just design-check`.
5. Commit `tokens.json` together with everything it regenerated.

A new token group needs no generator change as long as it uses a type the
generator knows (`color`, `dimension`, `number`, `typography`, `shadow`,
`transition`); another type fails the build with the token's path.

## Checks

`just design-check` (also part of `just check`, and the `design` job in CI):

- **Drift**: regenerates and fails if any generated file changed, i.e. someone
  edited a generated file or forgot to regenerate. Running the generator twice
  never produces a diff.
- **Contrast**: `contrast.mjs` checks every text/background pair the
  components use, in light, dark and both high-contrast appearances, against
  WCAG 2.2 AA (4.5:1 text, 3:1 large text and UI glyphs). The background is
  what the text really sits on: material tints composited over the canvas base,
  alone and under the wash at `opacity.washMax` (each gradient stop, plain and
  under each hill), worst case kept. `node desktop/macos/design/contrast.mjs --verbose`
  prints every pair. Add a pair to `PAIRS` there when a component puts text on
  a new surface.
- **DESIGN.md lint**: `designmd lint` from `@google/design.md`; errors fail,
  warnings do not.

Everything runs offline after `npm ci`.

## Token names

The names below are the contract with the app and with the Figma library that
mirrors it: keep a Figma variable's path the same as the token's. A path
`color.accent.ink` is `Tokens.Colors.Accent.ink` in Swift and
`--color-accent-ink` in CSS. Values live in `tokens.json`; the token index at
the bottom of `preview.html` and the `DESIGN.md` front matter list them too.
Every colour has light, dark, high-contrast and high-contrast dark values
(a missing one falls back as described above).

| Group | Tokens |
| --- | --- |
| `color.accent.*` | `default`, `pressed`, `subtle`, `on`, `ink` |
| `color.text.*` | `primary`, `secondary`, `tertiary` |
| `color.status.*` | `installed`, `installedSubtle`, `update`, `updateSubtle`, `busy`, `busySubtle`, `warning`, `warningSubtle`, `error`, `errorSubtle` |
| `color.background.*` | `base`, `washStart`, `washEnd`, `washHill`, `washDeep` |
| `color.glass.*` | `window`, `regular`, `frost`, `control`, `elevated`, `clear`, `stroke`, `highlight`, `shade`, `solidRegular`, `solidElevated` |
| `color.shadow.*` | `drop`, `window` |
| `color.*` | `scrim`, `separator`, `focusRing` |
| `color.fill.*` | `control`, `hover`, `track` |
| `color.preset.tint.*` | `sky`, `mint`, `sand`, `rose`, `lilac`, `smoke` |
| `color.preset.accent.*` | `blue`, `purple`, `pink`, `red`, `orange`, `yellow`, `green`, `graphite` |
| `opacity.*` | `wash`, `washMax`, `tint`, `tintDark` |
| `space.*` | `xxs`, `xs`, `sm`, `md`, `lg`, `xl`, `xxl`, `xxxl`, `huge` |
| `radius.*` | `xs`, `sm`, `md`, `lg`, `xl`, `full` |
| `size.*` | `sidebarWidth`, `rowHeight`, `toolbarHeight`, `iconSm`, `iconMd`, `iconLg`, `appIcon`, `badgeHeight`, `progressHeight`, `controlHeight`, `menuBarWidth`, `contentMaxWidth`, `hairline` |
| `blur.*` | `regular`, `elevated`, `clear` |
| `typography.*` | `display`, `title`, `title2`, `headline`, `body`, `callout`, `caption`, `overline`, `badge`, `mono` |
| `elevation.*` | `level0`, `level1`, `level2`, `level3`, `level4`, `lift`, `window` |
| `motion.*` | `instant`, `quick`, `standard`, `gentle`, `emphasized` |
| `component.*` | `buttonPrimary`, `buttonSecondary`, `packageRow`, `card`, `sheet`, `sidebar`, `menuBarExtra`, `badge`, `progress` |

`color.preset.*` are the swatches in Settings → Appearance, not colours for
views. The Liquid glass palette renamed the canvas mesh `meshTide`, `meshIris`,
`meshDawn` and `meshMist` to `washStart`, `washEnd`, `washHill` and
`washDeep`; nothing else was renamed. Update this table in the same change
as `tokens.json`.

## Files

| File | Role |
| --- | --- |
| `tokens.json` | the source |
| `build.mjs` | Style Dictionary with ketch's three output formats |
| `lib.mjs` | appearance modes and DTCG value parsing, shared by the generator and the contrast check |
| `contrast.mjs` | the WCAG check |
| `preview.html` | the review page; opens from disk, no network |
| `generated/Tokens.swift` | generated, do not edit |

Why this format and this generator: `docs/research-design-system.md`.
