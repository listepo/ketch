# Research: the macOS app's design-system format and token generator

Task F14. Sources checked 2026-10-01 unless a date says otherwise. This is a
companion to the desktop research (`docs/research-desktop.md`, still in review
on its own branch when this was written), kept in a separate file so the two
reviews do not conflict.

## DESIGN.md: a maintained format exists — follow it

- **What it is.** "A format specification for describing a visual identity to
  coding agents": YAML front matter holding machine-readable tokens (`colors`,
  `typography`, `rounded`, `spacing`, `components`) plus a Markdown body with
  `##` sections in a fixed order (Overview, Colors, Typography, Layout,
  Elevation & Depth, Shapes, Components, Do's and Don'ts). Unknown sections are
  preserved, not rejected. Source: the spec,
  https://github.com/google-labs-code/design.md/blob/main/docs/spec.md
  (format version `alpha`).
- **Who maintains it.** Google Labs, Apache-2.0, repository not archived, last
  push 2026-09-14 (GitHub API, `repos/google-labs-code/design.md`). Releases
  0.2.0 (2026-05-26), 0.3.0 (2026-06-15), 0.4.0 (2026-07-27) — GitHub releases.
- **Tooling.** The npm package `@google/design.md` 0.4.0
  (https://www.npmjs.com/package/@google/design.md, registry checked
  2026-10-01) ships `lint` (broken references, WCAG contrast on component
  pairs, section order; exit 1 on errors), `diff`, and `export` to DTCG and
  Tailwind. Documented in the repository README.
- **Caveat.** The README states the format is `alpha` and "under active
  development"; expect changes. That is why the front matter is generated
  rather than hand-written: a schema change is one generator edit.

Decision: `desktop/macos/DESIGN.md` follows the spec. Its front matter is
generated from `tokens.json`; the prose is hand-written. Sections the spec does
not name (Motion, Iconography, Accessibility) are added, which the spec
allows. The schema has no place for dark-mode values, elevation or motion, so
dark colours are emitted as `<name>-dark` entries and the rest stays in
`tokens.json` only, as the front matter's own header comment says.
`just design-check` runs the linter and fails on errors; its
`orphaned-tokens` warnings (colours no component references) are expected for
a system whose components are described in prose.

## Tokens: W3C DTCG Format Module 2025.10

- The Design Tokens Community Group's Format Module 2025.10 is a Final
  Community Group Report of 2025-10-28, "considered stable":
  https://www.designtokens.org/tr/2025.10/format/.
- It defines the value shapes `tokens.json` uses: colours as objects
  (`colorSpace`, `components`, optional `alpha` and `hex`), dimensions as
  `{value, unit}` with `px` or `rem`, durations with `ms` or `s`,
  `cubicBezier`, `shadow` (arrays of layers, `inset` supported),
  `typography`, `transition`, and `{group.token}` aliases.
- It has no notion of appearance modes. It does allow vendor data in
  `$extensions` and recommends reverse-domain keys, which tools must preserve.
  ketch keeps its light/dark/high-contrast values there, under
  `com.github.pyrlyn.ketch`.

## Generator: Style Dictionary, with our own formats

- **Style Dictionary** 5.5.5 (npm, published 2026-09-20; repository
  https://github.com/style-dictionary/style-dictionary, Apache-2.0, not
  archived, last push 2026-09-30). Its docs say DTCG has had "first-class
  support" since v4 and that "the latest format 2025.10 does not have full
  support yet" (https://styledictionary.com/info/dtcg/). Its built-in Swift
  transforms (`color/ColorSwiftUI` and friends,
  https://styledictionary.com/reference/hooks/transforms/predefined/) emit
  one static colour per token: no dark or high-contrast variant.
- **Terrazzo** (`@terrazzo/cli` 2.7.1, npm) is DTCG-native; its Swift plugin
  `@terrazzo/plugin-swift` 0.3.3 (npm, 2026-07-26) is pre-1.0 and, per its
  README, generates a `Tokens.xcassets` asset catalog rather than Swift
  source.

Decision: Style Dictionary, maintained and DTCG-aware, for
reading, merging and reference-checking `tokens.json` and for writing the
outputs; the three output formats (SwiftUI, CSS, DESIGN.md YAML) are ours,
registered as Style Dictionary format hooks in
`desktop/design/build.mjs`. That keeps colours asset-free and dynamic:
`NSColor(name:dynamicProvider:)` (macOS 10.15+,
https://developer.apple.com/documentation/appkit/nscolor/init(name:dynamicprovider:))
resolved against `.aqua`, `.darkAqua`, `.accessibilityHighContrastAqua` and
`.accessibilityHighContrastDarkAqua` — the last two are what Increase Contrast
selects (https://developer.apple.com/documentation/appkit/nsappearance/name-swift.struct/accessibilityhighcontrastaqua).
Terrazzo would have meant an asset catalog and a pre-1.0 plugin.

## Liquid Glass facts the design relies on

All from Apple's documentation JSON for each symbol, checked 2026-10-01:

- `Glass` ("the configuration of the Liquid Glass material"), with `regular`,
  `clear`, `identity`, `tint(_:)` and `interactive(_:)`; `glassEffect(_:in:)`;
  `GlassEffectContainer`; `glassEffectID(_:in:)`; the `glass` and
  `glassProminent` button styles; `ConcentricRectangle`;
  `scrollEdgeEffectStyle(_:for:)` — all macOS 26.0.
  https://developer.apple.com/documentation/swiftui/glass
- `accessibilityReduceTransparency`, `colorSchemeContrast`,
  `accessibilityReduceMotion` environment values (macOS 10.15);
  `ShadowStyle.inner(color:radius:x:y:)` (macOS 13).
- HIG, Materials: Liquid Glass "forms a distinct functional layer for controls
  and navigation elements"; "Don't use Liquid Glass in the content layer";
  "Use Liquid Glass effects sparingly".
  https://developer.apple.com/design/human-interface-guidelines/materials
  This is why DESIGN.md splits glossy glass (controls) from frosted standard
  material (content).
- WCAG 2.2 contrast ratio and relative luminance, which `contrast.mjs`
  implements: https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio.

## Unverified

- How close the preview's CSS (`backdrop-filter`, tints, inset shadows) comes
  to real Liquid Glass. It is an approximation for reviewing colour, type,
  spacing and hierarchy, not a rendering of the material.
- The contrast check composites flat tints; real glass blurs and adapts its
  luminance, which may raise or lower the true ratio. It needs a pass with
  Accessibility Inspector once the app runs.
- `Tokens.swift` type-checks with Swift 6.4 against the macOS 27 SDK
  (`swiftc -typecheck -swift-version 6 -target arm64-apple-macos26.0`) but has
  not been used by an app build yet.
