/**
 * The source-plugin wire protocol: what a `ketch-source-<scheme>` executable
 * writes to stdout, one JSON document per subcommand.
 *
 * Port of the message shapes from the Rust `source/plugin.rs` and
 * `docs/PLUGINS.md`. A plugin is a third-party executable, so every document
 * is untrusted input; process handling (deadline, output cap, closed stdin)
 * lives with the plugin runner in @ketch/core.
 */

import { z } from "zod";

/** The protocol this build speaks; a plugin must report exactly this. */
export const PROTOCOL_VERSION = 1;

/** Executable prefix a plugin must use to be discovered. */
export const PLUGIN_PREFIX = "ketch-source-";

/**
 * The scheme ends up in user input and in recorded state, so it has to be
 * something that can be typed and round-tripped unambiguously.
 */
export function usableScheme(scheme: string): boolean {
  return /^[A-Za-z0-9_-]+$/.test(scheme);
}

/** Answer to `capabilities`. */
export const pluginCapabilitiesSchema = z.object({
  protocol: z.int().nonnegative(),
  scheme: z.string(),
  /** The plugin fetches assets itself, e.g. because they need credentials. */
  download: z.boolean().default(false),
  search: z.boolean().default(false),
});
export type PluginCapabilities = z.infer<typeof pluginCapabilitiesSchema>;

export const checksumSchema = z.object({
  /** Lowercase algorithm name, currently always `sha256`. */
  algo: z.string(),
  hex: z.string(),
});
export type Checksum = z.infer<typeof checksumSchema>;

export const releaseAssetSchema = z.object({
  name: z.string(),
  url: z.string(),
  size: z.int().nonnegative().default(0),
  content_type: z.string().nullish(),
  /** Checksum published by the source itself, when it offers one. */
  digest: checksumSchema.nullish(),
  /** Extra headers a source needs for the download. */
  headers: z.record(z.string(), z.string()).default({}),
});
export type PluginReleaseAsset = z.infer<typeof releaseAssetSchema>;

export const releaseSchema = z.object({
  version: z.string(),
  tag: z.string(),
  prerelease: z.boolean().default(false),
  draft: z.boolean().default(false),
  published_at: z.string().nullish(),
  notes: z.string().nullish(),
  assets: z.array(releaseAssetSchema).default([]),
});
export type PluginRelease = z.infer<typeof releaseSchema>;

/** Repository-level metadata, returned by `describe` and `search`. */
export const sourceInfoSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullish(),
  homepage: z.string().nullish(),
  stars: z.int().nonnegative().nullish(),
  license: z.string().nullish(),
  archived: z.boolean().default(false),
});
export type SourceInfo = z.infer<typeof sourceInfoSchema>;

/** Answer to `releases <id>`: newest first. */
export const pluginReleasesSchema = z.array(releaseSchema);

/** Answer to `describe <id>`: one object, or null when unknown. */
export const pluginDescribeSchema = z.union([sourceInfoSchema, z.null()]);

/** Answer to `search <query>`: empty when the plugin does not search. */
export const pluginSearchSchema = z.array(sourceInfoSchema);
