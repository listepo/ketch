// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

// Token resolution shared by build.mjs and contrast.mjs: appearance modes,
// per-mode references, and the DTCG 2025.10 value shapes this design uses
// (sRGB colour objects, px/rem dimensions, ms/s durations, shadow layers).

export const EXT = "com.github.pyrlyn.ketch";
export const MODES = ["light", "dark", "highContrast", "highContrastDark"];

/**
 * Tokens of a DTCG document as {path, $type, $description, original}, in source
 * order, the same shape Style Dictionary hands a format. A group's `$type` is
 * inherited by the tokens under it.
 */
export function flatten(doc) {
  const out = [];
  const walk = (node, path, inherited) => {
    const type = node.$type ?? inherited;
    if ("$value" in node) {
      out.push({ path, $type: type, $description: node.$description, original: node });
      return;
    }
    for (const [k, v] of Object.entries(node)) {
      if (!k.startsWith("$") && v && typeof v === "object") walk(v, [...path, k], type);
    }
  };
  walk(doc, [], undefined);
  return out;
}


/** Index of every token by dotted path, built from Style Dictionary's tokens. */
export function indexTokens(allTokens) {
  const byPath = new Map();
  for (const t of allTokens) byPath.set(t.path.join("."), t);
  return byPath;
}

export const REF = /^\{([^}]+)\}$/;

/** The raw (unresolved) value of `token` in `mode`, following the fallbacks. */
export function rawModeValue(token, mode) {
  const modes = token.original.$extensions?.[EXT] ?? {};
  if (mode === "light") return token.original.$value;
  if (mode === "dark") return modes.dark ?? token.original.$value;
  if (mode === "highContrast") return modes.highContrast ?? token.original.$value;
  return modes.highContrastDark ?? modes.dark ?? token.original.$value;
}

/** Fully resolved value of `token` in `mode`; references resolve per mode. */
export function modeValue(byPath, token, mode, seen = new Set()) {
  const raw = rawModeValue(token, mode);
  return resolveDeep(byPath, raw, mode, seen, token.path.join("."));
}

export function resolveDeep(byPath, value, mode, seen, where) {
  if (typeof value === "string") {
    const m = REF.exec(value);
    if (!m) return value;
    const target = byPath.get(m[1]);
    if (!target) throw new Error(`${where}: reference ${value} does not resolve`);
    if (seen.has(m[1])) throw new Error(`${where}: circular reference through ${value}`);
    return modeValue(byPath, target, mode, new Set([...seen, m[1]]));
  }
  if (Array.isArray(value)) return value.map((v) => resolveDeep(byPath, v, mode, seen, where));
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [k, resolveDeep(byPath, v, mode, seen, where)]),
    );
  }
  return value;
}

/** A DTCG 2025.10 sRGB colour as {r, g, b, a} in 0...1, validated. */
export function rgba(color, where) {
  if (!color || color.colorSpace !== "srgb" || !Array.isArray(color.components)) {
    throw new Error(`${where}: colour must be a DTCG object with colorSpace "srgb"`);
  }
  const [r, g, b] = color.components;
  const a = color.alpha ?? 1;
  for (const c of [r, g, b, a]) {
    if (typeof c !== "number" || c < 0 || c > 1) throw new Error(`${where}: component out of 0...1`);
  }
  // `hex` is optional in DTCG; when present it must agree with the components,
  // so an edit to one and not the other fails here instead of drifting.
  if (color.hex !== undefined && color.hex.toLowerCase() !== toHex({ r, g, b })) {
    throw new Error(`${where}: hex ${color.hex} disagrees with components (${toHex({ r, g, b })})`);
  }
  return { r, g, b, a };
}

export const byte = (c) => Math.round(c * 255);
export const toHex = ({ r, g, b }) =>
  "#" + [r, g, b].map((c) => byte(c).toString(16).padStart(2, "0")).join("");
export const toHex8 = (c) =>
  c.a === 1 ? toHex(c) : toHex(c) + byte(c.a).toString(16).padStart(2, "0");
export const num = (n) => String(Math.round(n * 10000) / 10000);

/** Pixels from a DTCG dimension object ({value, unit: px|rem}). */
export function px(dim, where) {
  if (!dim || typeof dim.value !== "number") throw new Error(`${where}: not a dimension`);
  if (dim.unit === "px") return dim.value;
  if (dim.unit === "rem") return dim.value * 16;
  throw new Error(`${where}: unsupported unit ${dim.unit}`);
}

/** Milliseconds from a DTCG duration object. */
export function ms(d, where) {
  if (!d || typeof d.value !== "number") throw new Error(`${where}: not a duration`);
  if (d.unit === "ms") return d.value;
  if (d.unit === "s") return d.value * 1000;
  throw new Error(`${where}: unsupported unit ${d.unit}`);
}

/** Normalised shadow list: one entry per layer, colours per mode. */
export function shadowLayers(byPath, token) {
  const where = token.path.join(".");
  const perMode = MODES.map((m) => {
    const v = modeValue(byPath, token, m);
    return Array.isArray(v) ? v : [v];
  });
  const [light] = perMode;
  return light.map((layer, i) => {
    const geometry = (l) =>
      [l.offsetX, l.offsetY, l.blur, l.spread ?? { value: 0, unit: "px" }].map((d) => px(d, where));
    const g = geometry(layer);
    for (const other of perMode) {
      if (!other[i] || geometry(other[i]).join() !== g.join() || !!other[i].inset !== !!layer.inset) {
        throw new Error(`${where}: shadow layer ${i} must keep its geometry in every mode; only colour varies`);
      }
    }
    return {
      colors: perMode.map((p, m) => rgba(p[i].color, `${where}[${i}] ${MODES[m]}`)),
      x: g[0],
      y: g[1],
      blur: g[2],
      spread: g[3],
      inset: !!layer.inset,
    };
  });
}
