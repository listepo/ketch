//! Pieces every command body needs, so no two of them grow their own.
//!
//! The commands themselves stay thin — arguments, output, confirmations — and
//! everything below this line is either a seam onto `@ketch/core` or a piece of
//! reporting that would otherwise be copied into four files.

import {
  binDirOnPath,
  type Config,
  type Installed,
  installedBinaries,
  type InstalledPackage,
  KetchError,
  Lock,
  loadSourceRegistry,
  type ManifestResolver,
  Resolver,
  shell,
  type SourceRegistry,
  type State,
} from "@ketch/core";
import * as ui from "../ui.ts";

/** The live source registry, with plugin discovery reported through `ui`. */
export async function sources(cfg: Config): Promise<SourceRegistry> {
  return await loadSourceRegistry(cfg, { debug: ui.debug, warn: ui.warn });
}

/**
 * The manifest resolver, adapted to the async seam the install pipeline takes.
 *
 * Resolution itself is synchronous — it reads an already-loaded registry — but
 * `install` is written against a `Promise`, so a future source that has to ask
 * the network for a manifest can be dropped in without touching the pipeline.
 */
export function manifests(cfg: Config): ManifestResolver {
  const resolver = Resolver.create(cfg, ui.warn);
  return {
    resolve(spec) {
      const [manifest, origin] = resolver.resolve(spec);
      return Promise.resolve({ manifest, origin });
    },
  };
}

/** Run `body` holding the install lock, releasing it however it ends. */
export async function locked<T>(cfg: Config, body: () => Promise<T>): Promise<T> {
  const lock = await Lock.acquire(cfg, ui.debug);
  try {
    return await body();
  } finally {
    lock.release();
  }
}

/**
 * The packages a command was pointed at: everything installed when no name was
 * given, otherwise exactly the named ones.
 *
 * Every name is resolved before any work starts, so a typo in the third of four
 * names fails before the first one has been touched.
 */
export function select(state: State, names: readonly string[]): InstalledPackage[] {
  if (names.length === 0) {
    return state.all();
  }
  return names.map((name) => {
    const found = state.find(name);
    if (found === undefined) {
      throw new KetchError({ kind: "not_installed", name });
    }
    return found;
  });
}

/** Drop repeats while keeping the order the user wrote. */
export function dedupe(names: readonly string[]): string[] {
  return [...new Set(names)];
}

/** What the user sees after one package lands. */
export function report(result: Installed): void {
  const pkg = result.package;
  const detail =
    result.replaced !== null && result.replaced.raw !== pkg.version.raw
      ? `${pkg.name} ${pkg.version.raw} (was ${result.replaced.raw})`
      : `${pkg.name} ${pkg.version.raw}`;
  ui.success("installed", detail);

  for (const record of installedBinaries(pkg)) {
    ui.debug(`linked ${record.link}`);
  }

  // Said once, at the moment the hash is first recorded: from here on it is
  // pinned, and a later download that disagrees will fail loudly.
  if (!pkg.checksum_verified) {
    ui.warn(`${pkg.name} published no checksum; trusting ${pkg.sha256.slice(0, 12)} on first use`);
  }

  const notes = pkg.manifest?.notes ?? null;
  if (notes !== null && notes.trim() !== "") {
    ui.out(notes);
  }
}

/**
 * Say something once when binaries were linked somewhere the shell cannot see.
 *
 * Silent when the bin dir is already on `PATH`, and silent when nothing that is
 * installed put a binary there in the first place — an `.app`-only tree has
 * nothing to find.
 */
export function pathHint(cfg: Config, state: State): void {
  if (binDirOnPath(cfg)) {
    return;
  }
  if (!state.all().some((pkg) => installedBinaries(pkg).length > 0)) {
    return;
  }
  ui.warn(`${cfg.binDir} is not on your PATH`);
  if (shell.configuredIn(cfg).length > 0) {
    ui.out("It is in your shell config already — open a new shell.");
  } else {
    ui.out("Run `ketch path install` to add it.");
  }
}
