// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// WCAG 2.2 contrast check for the text/background pairs the design uses, in
// every appearance mode, on the colour the text actually sits on.
//
// Glass is translucent, so a pair's background is not a token: it is the glass
// tint composited over what is behind it. Each surface below lists its layers
// bottom-up; the check composites them over every backdrop and keeps the worst
// ratio. A backdrop is the canvas base alone, or the base with the backdrop
// wash drawn over it at `opacity.washMax`, the strongest the Appearance setting
// allows: each gradient stop, plain and under each hill. Luminance moves
// monotonically between no wash and the strongest, so the two ends bound every
// strength in between. The system material also blurs and adapts; tints are our
// stand-in for it, which DESIGN.md -> Accessibility states as the
// approximation it is.
//
// Offline and dependency-free: `node desktop/design/contrast.mjs`, exit 1
// on any failure. WCAG formulas: https://www.w3.org/TR/WCAG22/#dfn-contrast-ratio
// and #dfn-relative-luminance (W3C Recommendation, checked 2026-10-01).

import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { MODES, flatten, indexTokens, modeValue, rgba } from "./lib.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const byPath = indexTokens(flatten(JSON.parse(await readFile(join(here, "tokens.json"), "utf8"))));

// Normal text needs 4.5:1; large text (18pt, or 14pt bold) and non-text UI
// such as glyphs, progress fills and focus rings need 3:1 (WCAG 1.4.3, 1.4.11).
const TEXT = 4.5;
const LARGE = 3;

// The wash: two gradient stops, each alone and under each hill.
const WASH = [
  ["washStart"],
  ["washEnd"],
  ["washStart", "washHill"],
  ["washEnd", "washHill"],
  ["washStart", "washDeep"],
  ["washEnd", "washDeep"],
];

// Surfaces, bottom-up. The backdrop is added underneath each one.
const SURFACES = {
  canvas: [],
  window: ["color.glass.window"],
  sidebar: ["color.glass.regular"],
  card: ["color.glass.frost"],
  cardOnWindow: ["color.glass.window", "color.glass.frost"],
  control: ["color.glass.control"],
  controlOnSidebar: ["color.glass.regular", "color.glass.control"],
  selectedRow: ["color.glass.frost", "color.accent.subtle"],
  tintedControl: ["color.glass.frost", "color.accent.subtle"],
  sheet: ["color.scrim", "color.glass.elevated"],
  menuBarExtra: ["color.glass.elevated"],
  clearControl: ["color.glass.clear"],
  solidRegular: ["color.glass.solidRegular"],
  solidElevated: ["color.glass.solidElevated"],
  accentFill: ["color.accent.default"],
  badgeInstalled: ["color.glass.regular", "color.status.installedSubtle"],
  badgeUpdate: ["color.glass.regular", "color.status.updateSubtle"],
  badgeBusy: ["color.glass.regular", "color.status.busySubtle"],
  badgeWarning: ["color.glass.regular", "color.status.warningSubtle"],
  badgeError: ["color.glass.regular", "color.status.errorSubtle"],
};

const GLASS = [
  "window", "sidebar", "card", "cardOnWindow", "control", "controlOnSidebar",
  "sheet", "menuBarExtra", "solidRegular", "solidElevated",
];
const STATUS = ["installed", "update", "busy", "warning", "error"];

/** [foreground token, surface, minimum ratio] */
const PAIRS = [
  ...GLASS.flatMap((s) => [
    ["color.text.primary", s, TEXT],
    ["color.text.secondary", s, TEXT],
    ["color.text.tertiary", s, TEXT],
    // Accent as text or a glyph on glass is always accent.ink; accent.default
    // is a fill and carries accent.on.
    ["color.accent.ink", s, TEXT],
    ...STATUS.map((st) => [`color.status.${st}`, s, TEXT]),
  ]),
  ["color.text.primary", "canvas", TEXT],
  ["color.text.secondary", "canvas", TEXT],
  ["color.text.primary", "selectedRow", TEXT],
  ["color.text.secondary", "selectedRow", TEXT],
  ["color.accent.ink", "selectedRow", TEXT],
  ["color.accent.ink", "tintedControl", TEXT],
  ["color.text.primary", "clearControl", LARGE],
  ["color.accent.on", "accentFill", TEXT],
  ...STATUS.map((st) => [`color.status.${st}`, `badge${st[0].toUpperCase()}${st.slice(1)}`, TEXT]),
  ["color.accent.ink", "canvas", LARGE],
  ["color.focusRing", "sidebar", LARGE],
  ["color.focusRing", "card", LARGE],
];

const over = (top, bottom) => {
  const a = top.a + bottom.a * (1 - top.a);
  const mix = (t, b) => (t * top.a + b * bottom.a * (1 - top.a)) / a;
  return { r: mix(top.r, bottom.r), g: mix(top.g, bottom.g), b: mix(top.b, bottom.b), a };
};

const lin = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const luminance = ({ r, g, b }) => 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
const ratio = (x, y) => {
  const [hi, lo] = [luminance(x), luminance(y)].sort((a, b) => b - a);
  return (hi + 0.05) / (lo + 0.05);
};

const token = (path) => {
  const t = byPath.get(path);
  if (!t) throw new Error(`contrast check names unknown token ${path}`);
  return t;
};
const color = (path, mode) => rgba(modeValue(byPath, token(path), mode), `${path} ${mode}`);

const washMax = modeValue(byPath, token("opacity.washMax"), "light");
if (typeof washMax !== "number" || washMax < 0 || washMax > 1) {
  throw new Error("opacity.washMax must be a number in 0...1");
}

/** Every backdrop in `mode` as [name, opaque colour]. */
function backdrops(mode) {
  const base = { ...color("color.background.base", mode), a: 1 };
  const out = [["base", base]];
  for (const layers of WASH) {
    let wash = { ...color(`color.background.${layers[0]}`, mode), a: 1 };
    for (const hill of layers.slice(1)) wash = over(color(`color.background.${hill}`, mode), wash);
    out.push([layers.join("+"), over({ ...wash, a: washMax }, base)]);
  }
  return out;
}

let failures = 0;
const rows = [];
for (const mode of MODES) {
  const grounds = backdrops(mode);
  for (const [fg, surface, min] of PAIRS) {
    let worst = Infinity;
    let worstBackdrop = "";
    for (const [name, ground] of grounds) {
      let bg = ground;
      for (const layer of SURFACES[surface]) bg = over(color(layer, mode), bg);
      const text = over(color(fg, mode), bg);
      const r = ratio(text, bg);
      if (r < worst) [worst, worstBackdrop] = [r, name];
    }
    const ok = worst >= min;
    if (!ok) failures++;
    rows.push(`${ok ? "pass" : "FAIL"}  ${worst.toFixed(2).padStart(5)}:1 >= ${min}  ${mode.padEnd(16)} ${fg} on ${surface} (worst over ${worstBackdrop})`);
  }
}

const verbose = process.argv.includes("--verbose");
for (const row of rows) if (verbose || row.startsWith("FAIL")) console.log(row);
console.log(`contrast: ${rows.length - failures}/${rows.length} pairs meet WCAG AA`);
process.exit(failures ? 1 : 0);
