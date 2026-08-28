/**
 * Turning what the user typed into a `Manifest`.
 *
 * Four tiers, in order: a user manifest in `<root>/manifests/<name>.json`,
 * the fetched package registry, the built-in registry (`@ketch/schemas`'s
 * `builtinPackages`), then inference from the source reference itself.
 * Inference is what lets `ketch install owner/repo` work for a repository
 * nobody has curated.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import {
  builtinPackages,
  manifestSchema,
  sanitizeComponent,
  schemaUrl,
  validateManifest,
} from "@ketch/schemas";
import type { Config } from "./config.ts";
import { KetchError } from "./errors.ts";
import { inferredManifest, normalizeName, PackageRef, PackageSpec } from "./model.ts";
import type { Manifest, ManifestOrigin } from "./model.ts";
import { load as loadRegistry } from "./registry.ts";
import { asciiLowercase } from "@ketch/schemas";

/**
 * Resolves specs to manifests. Built once per command.
 *
 * The three tiers below `Resolver.create`'s disk read are stored as plain
 * arrays — user and registry entries paired with the file they came from, so
 * an origin can point at something the user can edit — in precedence order:
 * user shadows registry shadows built-in.
 */
export class Resolver {
  constructor(
    private readonly builtin: readonly Manifest[],
    /** `[manifest, file]` pairs, from the fetched registry. */
    private readonly registry: ReadonlyArray<readonly [Manifest, string]>,
    /** `[manifest, file]` pairs, from `<root>/manifests/*.json`. */
    private readonly user: ReadonlyArray<readonly [Manifest, string]>,
  ) {}

  /** Load every tier from disk. A malformed entry is warned about and skipped. */
  static create(cfg: Config, warn?: (message: string) => void): Resolver {
    return new Resolver(
      builtinPackages,
      loadRegistry(cfg, warn),
      loadUserManifests(cfg.manifestDir, warn),
    );
  }

  /** Resolve a spec, reporting where the manifest came from. */
  resolve(spec: PackageSpec): [Manifest, ManifestOrigin] {
    // An explicit reference still gets a curated manifest when one exists:
    // `ketch install BurntSushi/ripgrep` should link `rg`, not `ripgrep`.
    if (spec.reference !== null) {
      const reference = spec.reference;
      const found = this.find((m) => refMatches(m, reference));
      if (found !== null) {
        return found;
      }
      return [inferredManifest(reference), "inferred"];
    }

    const alias = spec.alias !== null ? normalizeName(spec.alias) : "";
    const found = this.find((m) => answersTo(m, alias));
    if (found === null) {
      throw KetchError.msg(
        `no package named \`${alias}\`; run \`ketch update\` to refresh the registry, ` +
          `\`ketch search ${alias}\` to look on GitHub, or give an \`owner/repo\` reference`,
      );
    }
    return found;
  }

  /** Every alias the registry knows, for completion and `ketch search`. */
  aliases(): string[] {
    const out: string[] = [];
    for (const m of this.manifests()) {
      out.push(m.name, ...m.provides);
    }
    return [...new Set(out)].toSorted();
  }

  /**
   * Known packages matching a free-text query, highest tier first. A name is
   * listed once, from whichever tier would actually install it.
   */
  search(query: string): Manifest[] {
    const needle = asciiLowercase(query.trim());
    const seen = new Set<string>();
    const out: Manifest[] = [];
    for (const m of this.manifests()) {
      if (needle !== "" && !matchesQuery(m, needle)) {
        continue;
      }
      const key = normalizeName(m.name);
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      out.push(m);
    }
    return out;
  }

  /**
   * Precedence order: user manifests shadow the registry, which shadows the
   * built-ins. Everything that reads the tiers goes through this.
   */
  private manifests(): Manifest[] {
    return [...this.user.map(([m]) => m), ...this.registry.map(([m]) => m), ...this.builtin];
  }

  private find(pred: (m: Manifest) => boolean): [Manifest, ManifestOrigin] | null {
    const fromUser = this.user.find(([m]) => pred(m));
    if (fromUser !== undefined) {
      return [fromUser[0], { user: fromUser[1] }];
    }
    const fromRegistry = this.registry.find(([m]) => pred(m));
    if (fromRegistry !== undefined) {
      return [fromRegistry[0], { registry: fromRegistry[1] }];
    }
    const fromBuiltin = this.builtin.find((m) => pred(m));
    if (fromBuiltin !== undefined) {
      return [fromBuiltin, "builtin"];
    }
    return null;
  }
}

function matchesQuery(manifest: Manifest, needle: string): boolean {
  return (
    asciiLowercase(manifest.name).includes(needle) ||
    asciiLowercase(sourceRef(manifest).id).includes(needle) ||
    manifest.provides.some((p) => asciiLowercase(p).includes(needle)) ||
    (manifest.description != null && asciiLowercase(manifest.description).includes(needle))
  );
}

function refMatches(manifest: Manifest, reference: PackageRef): boolean {
  const ref = sourceRef(manifest);
  return ref.scheme === reference.scheme && ref.id === reference.id;
}

function answersTo(manifest: Manifest, alias: string): boolean {
  return (
    normalizeName(manifest.name) === alias ||
    manifest.provides.some((p) => normalizeName(p) === alias)
  );
}

/** `manifest.source` is always parseable — every tier passes `validateManifest` first. */
function sourceRef(manifest: Manifest): PackageRef {
  return PackageRef.tryFrom(manifest.source);
}

/**
 * Parse a file holding either one manifest or a `{"package": [...]}` array of
 * them — the shape `builtinPackages` and each file in the user manifest
 * directory take.
 *
 * Which one is decided from the parsed shape, not the source text: a single
 * manifest that merely mentions "package" — in a note, in a description — is
 * still a manifest, and sniffing the raw text for the word would make that
 * file disappear from the registry without a word.
 */
export function parseManifestFile(text: string, what: string): Manifest[] {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch (cause) {
    throw KetchError.parse(what, asError(cause).message);
  }
  const entries = isPackageArray(data) ? data.package : [data];
  let manifests: Manifest[];
  try {
    manifests = entries.map((entry) => manifestSchema.parse(entry));
  } catch (cause) {
    throw KetchError.parse(what, detailMessage(cause));
  }
  // The schema has checked the shape; this checks the values it cannot judge
  // — one package at a time, so a failure among several names which entry is
  // broken instead of leaving the reader to guess.
  for (const manifest of manifests) {
    try {
      validateManifest(manifest);
    } catch (cause) {
      throw KetchError.parse(what, `package \`${manifest.name}\`: ${asError(cause).message}`);
    }
  }
  return manifests;
}

function isPackageArray(value: unknown): value is { package: unknown[] } {
  return (
    typeof value === "object" &&
    value !== null &&
    "package" in value &&
    Array.isArray((value as { package: unknown }).package)
  );
}

/**
 * Read every `.json` in the manifest directory.
 *
 * One unreadable file must not take down every command, so failures are
 * reported and skipped rather than propagated.
 */
function loadUserManifests(dir: string, warn?: (message: string) => void): [Manifest, string][] {
  let names: string[];
  try {
    names = fs.readdirSync(dir);
  } catch {
    return [];
  }
  const paths = names
    .filter((name) => name.endsWith(".json"))
    .map((name) => path.join(dir, name))
    .toSorted();

  const out: [Manifest, string][] = [];
  for (const file of paths) {
    try {
      const text = fs.readFileSync(file, "utf8");
      const manifests = parseManifestFile(text, file);
      out.push(...manifests.map((m): [Manifest, string] => [m, file]));
    } catch (cause) {
      // Both error kinds already name the file, so the warning does not.
      const message =
        cause instanceof KetchError ? cause.message : KetchError.io(file, asError(cause)).message;
      warn?.(`ignoring manifest: ${message}`);
    }
  }
  return out;
}

/** Where `ketch edit`/`ketch pin` should write a manifest for this package. */
export function userManifestPath(cfg: Config, name: string): string {
  return path.join(cfg.manifestDir, `${sanitizeComponent(name)}.json`);
}

/** Serialise a manifest for a user manifest file, with its `$schema` stamped. */
export function manifestToJson(manifest: Manifest): string {
  const { $schema: _schema, ...rest } = manifest;
  return `${JSON.stringify({ $schema: schemaUrl("manifest"), ...rest }, null, 2)}\n`;
}

function detailMessage(cause: unknown): string {
  if (
    typeof cause === "object" &&
    cause !== null &&
    "issues" in cause &&
    Array.isArray((cause as { issues: unknown }).issues)
  ) {
    const issues = (cause as { issues: ReadonlyArray<{ path: PropertyKey[]; message: string }> })
      .issues;
    return issues
      .map((issue) => (issue.path.length > 0 ? `${issue.path.join(".")}: ` : "") + issue.message)
      .join("; ");
  }
  return asError(cause).message;
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}
