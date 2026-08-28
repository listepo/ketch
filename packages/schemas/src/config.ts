/**
 * On-disk settings (`config.json`): the file tier of runtime configuration.
 *
 * Port of `ConfigFile` from the Rust `config.rs`. Every field is optional so
 * a partial file is valid; unknown keys are refused so a misspelt setting is
 * an error rather than a silently ignored one. Value parsing that carries a
 * "which file to fix" error message — log level, log format, jobs bounds —
 * stays with the config loader in @ketch/core, as it stayed out of serde.
 */

import { z } from "zod";

/** The upstream repository ketch updates itself from. */
export const SELF_REPO = "listepo/ketch";

/** The package registry ketch resolves names against. */
export const REGISTRY_REPO = "listepo/ketch-registry";

export const configFileSchema = z.strictObject({
  $schema: z.string().optional(),
  /** Has no effect — the file lives inside the root, so it cannot choose it. */
  root: z.string().nullish(),
  apps_dir: z.string().nullish(),
  github_token: z.string().nullish(),
  prerelease: z.boolean().nullish(),
  /** Allow installing x86_64 assets on Apple Silicon (via Rosetta). */
  allow_emulation: z.boolean().nullish(),
  /** Symlink `.app` bundles instead of copying them. */
  link_apps: z.boolean().nullish(),
  /** Refuse to install when the release publishes no checksum. */
  require_checksums: z.boolean().nullish(),
  /** Remove the quarantine flag from code that passes signature checks. */
  strip_quarantine: z.boolean().nullish(),
  self_repo: z.string().nullish(),
  /** `owner/repo` of the package registry. */
  registry: z.string().nullish(),
  /** How many packages a batch install works on at once. */
  jobs: z.int().nonnegative().nullish(),
  /** `off`, `error`, `warn`, `info` or `debug`. */
  log_level: z.string().nullish(),
  /** `text` or `json`. */
  log_format: z.string().nullish(),
});

export type ConfigFile = z.infer<typeof configFileSchema>;
