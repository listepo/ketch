/**
 * The fetched package registry: one folder per package, each holding a
 * `ketch.json`.
 *
 * Port of the entry-reading rules from the Rust `registry.rs`. The folder
 * already names the package, so `name` in the file is optional — and when it
 * is present it must agree with the folder, or the package would be
 * unreachable under the name its folder advertises. Fetching, collision
 * warnings and the swap-in live in @ketch/core; this module owns only the
 * shape and the folder rule.
 */

import { z } from "zod";
import { type Manifest, manifestFields, validateManifest } from "./manifest.ts";
import { asciiLowercase } from "./util.ts";

/** The file inside a package folder. */
export const PACKAGE_FILE = "ketch.json";

/** Lowercase a package name and strip decoration people put in repo names. */
export function normalizeName(raw: string): string {
  const lower = asciiLowercase(raw.trim());
  if (lower.endsWith(".rs")) {
    return lower.slice(0, -".rs".length);
  }
  if (lower.endsWith(".git")) {
    return lower.slice(0, -".git".length);
  }
  return lower;
}

/** A manifest as it sits in a registry folder: `name` may come from outside. */
export const registryPackageSchema = z.strictObject({
  ...manifestFields,
  name: z.string().optional(),
});
export type RegistryPackage = z.infer<typeof registryPackageSchema>;

/**
 * Parse one package folder's file into a full, validated manifest.
 *
 * `where` names the file in error messages so a broken entry can be reported
 * (and skipped by the caller — one broken entry must not hide the rest of
 * the registry).
 */
export function parseRegistryPackage(data: unknown, folder: string, where: string): Manifest {
  const entry = registryPackageSchema.parse(data);
  if (entry.name !== undefined && normalizeName(entry.name) !== normalizeName(folder)) {
    throw new Error(`${where}: declares name \`${entry.name}\` but sits in folder \`${folder}\``);
  }
  const manifest: Manifest = { ...entry, name: entry.name ?? folder };
  validateManifest(manifest);
  return manifest;
}
