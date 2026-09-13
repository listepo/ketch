# ketch design system

Nerd + flat + material + glass. One stylesheet, no build step, no framework.
Brand metaphor: **catch releases from GitHub** — a hook catching a release-tag chip.

## Overview

| Pillar | What it means here |
| --- | --- |
| **Nerd** | JetBrains Mono / IBM Plex Mono accents, release-tag chips, terminal install strip, monospace brand word |
| **Flat** | Crisp geometry, hairline borders, no skeuomorphic textures, no AI-purple gradients |
| **Material** | Surface ladder (`canvas` → `surface-1` → `surface-2`), soft elevation, theme-aware buttons |
| **Glass** | Sticky header + feature cards use `backdrop-filter` + translucent fill; opaque fallback when `prefers-reduced-transparency` |

Dual theme is first-class: light default tokens, dark tokens, and `data-theme="system"` that follows `prefers-color-scheme`. An explicit toggle cycles **system → light → dark** and persists to `localStorage` key `ketch-theme`.

## Colors

### Light (`[data-theme="light"]`)

| Token | Value | Role |
| --- | --- | --- |
| `--canvas` | `#F4F8F7` | Page ground |
| `--surface-1` | `#FFFFFF` | Raised panels, cards fallback |
| `--surface-2` | `#EAF1EF` | Sunken / chip / code bg |
| `--ink` | `#0B1A18` | Primary text |
| `--ink-muted` | `#5A7380` | Secondary text |
| `--ink-faint` | `#7A929C` | Meta / captions |
| `--accent` | `#0F6F5C` | Links, primary actions |
| `--accent-bright` | `#1FA88A` | Hover accent |
| `--accent-soft` | `#D8F0E8` | Soft accent wash |
| `--accent-on` | `#FFFFFF` | Text on accent fill |
| `--hairline` | `#D5E3DF` | Borders |
| `--glass-fill` | `rgba(255,255,255,.72)` | Glass panels |
| `--mark-tile` | `#0A3D36` | Logo tile (always dark) |
| `--tag-coral` | `#FF6B5B` | Release-tag chip |

### Dark (`[data-theme="dark"]`)

| Token | Value | Role |
| --- | --- | --- |
| `--canvas` | `#0A1214` | Page ground |
| `--surface-1` | `#101A1C` | Raised panels |
| `--surface-2` | `#162226` | Sunken / chip |
| `--ink` | `#E8F4F1` | Primary text |
| `--ink-muted` | `#8AA8A0` | Secondary text |
| `--ink-faint` | `#6A8680` | Meta |
| `--accent` | `#3DDCB0` | Links, primary actions |
| `--accent-bright` | `#5EE8C4` | Hover accent |
| `--accent-soft` | `#0F6F5C` | Soft accent wash |
| `--accent-on` | `#0A1214` | Text on accent fill |
| `--hairline` | `#1E3330` | Borders |
| `--glass-fill` | `rgba(16,26,28,.72)` | Glass panels |
| `--mark-tile` | `#0A3D36` | Logo tile (always dark) |
| `--tag-coral` | `#FF6B5B` | Release-tag chip |

Terminal chrome (`--term-*`) stays dark in both themes so the install strip and demo terminal read as a real shell.

### System

When `data-theme="system"` (or unset before boot), the matching light/dark block above is applied via `@media (prefers-color-scheme)`.

## Typography

| Role | Stack |
| --- | --- |
| UI / body | `ui-sans-serif, -apple-system, "SF Pro Text", "Segoe UI", system-ui, sans-serif` |
| Mono / nerd | `"JetBrains Mono", "IBM Plex Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace` |

- Body ~17px / 1.65; hero title clamps ~2.1–3.4rem with tight tracking.
- Brand wordmark uses mono + weight 600.
- Inline code and the install strip use mono; measure ~68ch for prose.

## Layout / breakpoints

| Breakpoint | Intent |
| --- | --- |
| `< 640` | Single-column hero, stacked header actions, docs nav wraps |
| `640–1023` | Single-column hero, compact docs sidebar |
| `≥ 1024` | Two-column hero, full docs layout |
| `≥ 1440` | Wider content max (~1280) |
| `≥ 1920` | Ultrawide max (~1400), slightly larger body |

Page gutters: `clamp(1rem, 4vw, 3rem)`. Content max defaults to ~1180px.

## Elevation / glass

1. **Hairline** — `1px solid var(--hairline)` for structure.
2. **Elev-1 / elev-2 / elev-3** — soft layered shadows (theme-aware opacity).
3. **Glass** — `background: var(--glass-fill)` + `backdrop-filter: blur(12–14px) saturate(1.15–1.2)` on sticky header, feature cards, doc cards.
4. **Reduced transparency** — `@media (prefers-reduced-transparency: reduce)` swaps glass to opaque `--surface-1` and disables blur.

## Components

- **Header** — sticky glass bar; SVG mark (`logo.svg`) + mono word; main nav; theme toggle.
- **Theme toggle** — cycles `system → light → dark`; persists `ketch-theme`; icons for sun / moon / display; 44px min hit target.
- **Install strip** — dark terminal bar with copy button (always dark chrome).
- **Buttons** — `.button` (surface + hairline) and `.button.primary` (accent fill, `--accent-on` text).
- **Feature / doc cards** — glass panels with hover hairline → accent.
- **Release chip** — pill with coral dot; release-tag metaphor.
- **Docs nav** — sticky under header; current page uses `--accent-soft` wash.
- **Skip link** — visible on focus.

## Logos

| File | Use |
| --- | --- |
| `static/img/logo.svg` | Mark 64×64 — dark teal tile, mint hook, coral release chip |
| `static/img/logo-wordmark.svg` | Mark + `ketch` mono word |
| `static/img/favicon.svg` | Simplified 32×32 mark |
| `static/img/favicon.legacy.svg` | Previous anchor-style favicon |

Mark tile is **always** dark teal (`#0A3D36`) so the mint hook stays legible on light and dark canvases.

## Accessibility

- Contrast aimed at WCAG AA for ink/accent on canvas and surfaces.
- `:focus-visible` uses a themed soft ring (`--focus-ring`).
- Coarse pointers get ≥44px hit targets on nav and toggle.
- `prefers-reduced-motion: reduce` kills transitions/animations and smooth scroll.
- `prefers-reduced-transparency: reduce` disables glass blur.
- Early theme boot script in `<head>` avoids FOUC; `color-scheme` meta + CSS `color-scheme` keep native controls in sync.

## Do

- Use semantic tokens (`--canvas`, `--ink`, `--accent`, …), never hard-coded hex in new UI.
- Keep the mark tile dark; keep terminal chrome dark.
- Prefer hairlines + elevation over heavy fills.
- Honor reduced-motion and reduced-transparency.

## Don't

- Don't introduce purple/violet “AI” gradients or skeuomorphic textures.
- Don't rely only on `prefers-color-scheme` — always keep the explicit toggle.
- Don't put light ink on the mark tile or mint strokes on a light tile.
- Don't add Three.js / WebGL hero backgrounds.
- Don't restyle form controls without checking both themes and focus rings.
