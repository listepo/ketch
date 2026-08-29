/**
 * The manifest: how to install one package (`ketch.json`).
 *
 * Port of the `Manifest` shape and `Manifest::validate` from the Rust
 * `model.rs`. The schema checks what serde checked; `validateManifest` checks
 * the values it cannot judge. A manifest is untrusted input — a colleague or
 * the registry wrote it — and two of its names become filesystem paths, so
 * names that would need sanitising are refused rather than rewritten: a
 * package that installs somewhere other than where it says is worse than one
 * that refuses to install.
 */

import { z } from "zod";
import { parsePackageRef, safeMemberPath, sanitizeComponent } from "./util.ts";

/** How many wrapper directories a manifest may ask to strip. */
export const MAX_STRIP_PREFIX = 8;

/** A `scheme:id` or `owner/repo` string, as `PackageRef` serializes. */
export const packageRefStringSchema = z.string().refine((text) => parsePackageRef(text) !== null, {
  error: (issue) =>
    `\`${String(issue.input)}\` is not a package reference; expected \`scheme:id\` or \`owner/repo\``,
});

export const packageKindSchema = z.enum(["auto", "binary", "app"]);
export type PackageKind = z.infer<typeof packageKindSchema>;

/** Which release asset to pick. Empty means "let the platform decide". */
export const assetSelectorSchema = z.object({
  include: z.array(z.string()).default([]),
  exclude: z.array(z.string()).default([]),
  /** Per-target override, keyed by `<os>-<arch>`, e.g. `macos-aarch64`. */
  target: z.record(z.string(), z.string()).default({}),
});
export type AssetSelector = z.infer<typeof assetSelectorSchema>;

/** One executable to expose on PATH. */
export const binSpecSchema = z.object({
  /** Path inside the extracted payload. Globs allowed. */
  path: z.string().nullish(),
  /** Name of the symlink. Defaults to the file name of `path`. */
  name: z.string().nullish(),
});
export type BinSpec = z.infer<typeof binSpecSchema>;

// Strict rather than permissive, matching Rust's deny_unknown_fields: a
// manifest is hand-written, often by someone else, and a misspelt key that is
// silently ignored produces a package that installs the wrong thing with no
// complaint anywhere.
const manifestFields = {
  $schema: z.string().optional(),
  name: z.string(),
  source: packageRefStringSchema,
  description: z.string().nullish(),
  homepage: z.string().nullish(),
  kind: packageKindSchema.default("auto"),
  asset: assetSelectorSchema.default({ include: [], exclude: [], target: {} }),
  bin: z.array(binSpecSchema).default([]),
  /** Leading path components to drop when extracting. */
  strip_prefix: z.int().nonnegative().max(MAX_STRIP_PREFIX).nullish(),
  /** Consider prereleases when resolving `latest`. */
  prerelease: z.boolean().default(false),
  /** Alternate names this package answers to. */
  provides: z.array(z.string()).default([]),
  /** Printed after a successful install. */
  notes: z.string().nullish(),
  /** Man pages / completions, recorded now so manifests stay valid. */
  extra_paths: z.array(z.string()).default([]),
};

export const manifestSchema = z.strictObject(manifestFields);
export type Manifest = z.infer<typeof manifestSchema>;

/** Reject a name that could not be used verbatim as one path component. */
function usableFileName(what: string, value: string): void {
  // sanitizeComponent already knows every character that is unsafe here, so
  // asking whether it would change the value is the whole check.
  if (sanitizeComponent(value) !== value) {
    throw new Error(`${what} \`${value}\` is not usable as a file name`);
  }
}

/** Reject a path that would reach outside the payload it is relative to. */
function containedPath(what: string, value: string): void {
  try {
    safeMemberPath(value);
  } catch {
    throw new Error(`${what} \`${value}\` must stay inside the package`);
  }
}

/**
 * Check what the schema cannot: that the names in this manifest are usable.
 *
 * `name` is a directory in the store and each `bin.name` is a link in the bin
 * directory, so this is the trust boundary between a manifest ketch did not
 * write and the user's disk.
 */
export function validateManifest(manifest: Manifest): void {
  usableFileName("package name", manifest.name);
  for (const spec of manifest.bin) {
    if (spec.name != null) {
      usableFileName("binary name", spec.name);
    }
    if (spec.path != null) {
      containedPath("binary path", spec.path);
    }
    if (spec.name == null && spec.path == null) {
      throw new Error("a `bin` entry needs `name`, `path`, or both");
    }
  }
  for (const path of manifest.extra_paths) {
    containedPath("extra path", path);
  }
  // Each level costs a directory listing of the payload, and no real archive
  // nests its wrapper directories this deep.
  if (manifest.strip_prefix != null && manifest.strip_prefix > MAX_STRIP_PREFIX) {
    throw new Error(`\`strip_prefix\` must be at most ${MAX_STRIP_PREFIX}`);
  }
  for (const alias of manifest.provides) {
    // U+0085 (NEL) is whitespace to Rust's char::is_whitespace but not to
    // JS's \s; included so both implementations refuse the same aliases.
    if (alias.trim() === "" || /[\s\u0085]/.test(alias)) {
      throw new Error(`\`${alias}\` cannot be an alias: it is not something anyone can type`);
    }
  }
}

/** Parse and fully validate one manifest in a single call. */
export function parseManifest(data: unknown): Manifest {
  const manifest = manifestSchema.parse(data);
  validateManifest(manifest);
  return manifest;
}

export { manifestFields };
