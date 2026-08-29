/**
 * Composing every source available this run.
 *
 * Port of the `SourceRegistry` parts of the Rust `source/mod.rs`: the
 * `Source` interface and the release-picking logic every source shares
 * (`optsFor`/`pick`/`resolveRelease`) live in `./source.ts`, which also
 * declares the `SourceRegistry` type surface itself — "every source
 * available this run, resolved by scheme." This module owns only
 * construction: the built-in GitHub source plus whatever plugins were
 * discovered, and the case-insensitive scheme routing between them.
 */

import { asciiLowercase } from "@ketch/schemas";
import type { Config } from "../config.ts";
import { KetchError } from "../errors.ts";
import { Http } from "../http.ts";
import type { PackageRef } from "../model.ts";
import { GitHubSource } from "./github.ts";
import { discoverPlugins } from "./plugin.ts";
import type { Source, SourceRegistry } from "./source.ts";

/**
 * Where `loadSourceRegistry` reports what it found, in place of the Rust
 * build's global `crate::ui::debug`/`crate::ui::warn` calls.
 */
export interface RegistryLoadOptions {
  /** One found plugin per call, before it is added to the registry. */
  debug?: (message: string) => void;
  /** One broken plugin per call; discovery keeps going regardless. */
  warn?: (message: string) => void;
}

/**
 * The concrete `SourceRegistry`. Exported (but not from the package's
 * `index.ts` barrel — see this file's return notes) so the colocated test
 * can build one from fake sources without going through `loadSourceRegistry`
 * and its hard dependency on `GitHubSource`/plugin discovery.
 */
export class LiveSourceRegistry implements SourceRegistry {
  private readonly sources: readonly Source[];

  constructor(sources: readonly Source[]) {
    this.sources = sources;
  }

  /** Throws `unknown_scheme` when nothing answers to `scheme`. */
  get(scheme: string): Source {
    const found = this.sources.find((s) => asciiLowercase(s.scheme) === asciiLowercase(scheme));
    if (found === undefined) {
      throw new KetchError({ kind: "unknown_scheme", scheme });
    }
    return found;
  }

  forRef(reference: PackageRef): Source {
    return this.get(reference.scheme);
  }

  schemes(): string[] {
    return this.sources.map((s) => s.scheme);
  }

  all(): readonly Source[] {
    return this.sources;
  }
}

/**
 * Built-in sources plus every discovered plugin. Plugin discovery failures
 * are reported rather than aborting the command: a broken third-party plugin
 * must not make `ketch install owner/repo` fail.
 */
export async function loadSourceRegistry(
  cfg: Config,
  options: RegistryLoadOptions = {},
): Promise<SourceRegistry> {
  const sources: Source[] = [new GitHubSource(new Http(cfg))];

  for (const found of await discoverPlugins(cfg)) {
    if (found.ok) {
      options.debug?.(`plugin \`${found.plugin.name}\` provides \`${found.plugin.scheme}\``);
      sources.push(found.plugin);
    } else {
      options.warn?.(`ignoring plugin: ${found.error.message}`);
    }
  }
  return new LiveSourceRegistry(sources);
}

/**
 * Only the built-in GitHub source. Used by self-update, which must not
 * depend on third-party plugins.
 */
export function builtinOnlySourceRegistry(cfg: Config): SourceRegistry {
  return new LiveSourceRegistry([new GitHubSource(new Http(cfg))]);
}
