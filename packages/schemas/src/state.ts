/**
 * `state.json`: the installed-package record, the only durable record ketch
 * keeps.
 *
 * Port of `State`/`InstalledPackage` and the shapes they embed from the Rust
 * `state.rs` and `model.rs`. Field names and enum spellings match the Rust
 * serde output byte-for-byte, so this schema reads a state file the Rust
 * binary wrote — including serde's externally-tagged `origin` and the `null`s
 * it writes for absent options.
 */

import { z } from "zod";
import { manifestSchema, packageRefStringSchema } from "./manifest.ts";

/** Bumped only when the on-disk shape changes incompatibly. */
export const STATE_VERSION = 1;

export const osSchema = z.enum(["macos", "linux", "windows"]);
export type Os = z.infer<typeof osSchema>;

export const archSchema = z.enum(["aarch64", "x86_64", "universal"]);
export type Arch = z.infer<typeof archSchema>;

export const targetSpecSchema = z.object({
  os: osSchema,
  arch: archSchema,
});
export type TargetSpec = z.infer<typeof targetSpecSchema>;

export const linkKindSchema = z.enum(["symlink", "copied_app", "linked_app"]);
export type LinkKind = z.infer<typeof linkKindSchema>;

export const linkRecordSchema = z.object({
  /** The path ketch created and is responsible for removing. */
  link: z.string(),
  /** What it points at inside the store. */
  target: z.string(),
  kind: linkKindSchema,
});
export type LinkRecord = z.infer<typeof linkRecordSchema>;

/**
 * Where a manifest came from. Serde's external tagging writes unit variants
 * as strings and path-carrying variants as one-key objects.
 */
export const manifestOriginSchema = z.union([
  z.literal("builtin"),
  z.literal("inferred"),
  z.strictObject({ registry: z.string() }),
  z.strictObject({ user: z.string() }),
]);
export type ManifestOrigin = z.infer<typeof manifestOriginSchema>;

export const installedPackageSchema = z.object({
  name: z.string(),
  version: z.string(),
  source: packageRefStringSchema,
  tag: z.string(),
  target: targetSpecSchema,
  asset_name: z.string(),
  /** SHA-256 of the downloaded asset, always recorded. */
  sha256: z.string(),
  /** True when the checksum was published by the source rather than TOFU. */
  checksum_verified: z.boolean().default(false),
  installed_at: z.int().nonnegative(),
  /** Store directory holding the extracted payload. */
  prefix: z.string(),
  links: z.array(linkRecordSchema).default([]),
  pinned: z.boolean().default(false),
  origin: manifestOriginSchema,
  /** Kept so `upgrade` reuses the same selection rules as `install`. */
  manifest: manifestSchema.nullish(),
});
export type InstalledPackage = z.infer<typeof installedPackageSchema>;

export const stateSchema = z.object({
  $schema: z.string().optional(),
  version: z.int().nonnegative(),
  /** Keyed by package name, which is unique across sources by construction. */
  packages: z.record(z.string(), installedPackageSchema).default({}),
});
export type State = z.infer<typeof stateSchema>;

/** Refuse a state file written by a newer ketch than this one. */
export function validateState(state: State, where: string): void {
  if (state.version > STATE_VERSION) {
    throw new Error(
      `${where} was written by a newer ketch (state version ${state.version}); ` +
        "upgrade with `ketch self update`",
    );
  }
}

/** Parse and fully validate one state file in a single call. */
export function parseState(data: unknown, where: string): State {
  const state = stateSchema.parse(data);
  validateState(state, where);
  return state;
}
