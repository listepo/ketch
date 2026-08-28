/**
 * Updating ketch with ketch.
 *
 * Deliberately stricter than a normal install: the running binary is the
 * thing that verifies every other download, so it is replaced only against a
 * published checksum — never on trust-on-first-use — and the previous binary
 * is kept until the new one has proven it can run.
 *
 * `./install.ts` and `./source/registry.ts` are concurrent sibling work,
 * reused rather than reimplemented: `scoreAssets`, `verifyChecksum`,
 * `uninstall` and the `ScoredAsset` type come from the former (ported from
 * `src/install.rs`'s `pub fn`s of the same names — `verify_checksum` is
 * `pub(crate)` in Rust, so exporting it from the module is enough, it does
 * not need to reach `index.ts`), and `builtinOnlySourceRegistry` from the
 * latter, standing in for `SourceRegistry::builtin_only` in
 * `src/source/mod.rs`. `verifyChecksum` and `uninstall` take `install.ts`'s
 * own `InstallReporter` in place of Rust's global `ui::`, so `update` and
 * `uninstallSelf` below accept one the same way `install.ts`'s own pipeline
 * does, rather than the ad hoc single-callback shape a standalone port of
 * this file would otherwise invent.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import fsp from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { sanitizeComponent } from "@ketch/schemas";
import type { Config } from "./config.ts";
import { KetchError } from "./errors.ts";
import { extractAuto } from "./extract/index.ts";
import type { InstallReporter, ScoredAsset } from "./install.ts";
import { scoreAssets, silentReporter, uninstall, verifyChecksum } from "./install.ts";
import { targetString, Version } from "./model.ts";
import { hostPlatform } from "./platform/platform.ts";
import type { ProgressSink } from "./progress.ts";
import { NullProgress } from "./progress.ts";
import { builtinOnlySourceRegistry } from "./source/registry.ts";
import { defaultListOpts, resolveRelease } from "./source/source.ts";
import { Lock, State } from "./state.ts";

/** Outcome of a self-update attempt. */
export interface SelfUpdate {
  from: Version;
  to: Version;
  /** False when already current, or when `dryRun` was set. */
  replaced: boolean;
  notes: string | null;
}

/**
 * The version this binary was built as.
 *
 * Rust gets this for free from `env!("CARGO_PKG_VERSION")`, baked in at
 * compile time. There is no TS equivalent — core does not get to decide
 * which `package.json` (or Perry build metadata) counts as "the running
 * application's own" — so the caller passes it in.
 */
export function currentVersion(raw: string): Version {
  return Version.parse(raw);
}

/**
 * Where the running binary lives, with symlinks resolved so we replace the
 * real file rather than the link pointing at it.
 */
export function currentExe(): string {
  const exe = process.execPath;
  try {
    return fs.realpathSync(exe);
  } catch {
    return exe;
  }
}

export interface UpdateOptions {
  /** The running binary's own version — see `currentVersion`. */
  version: string;
  force?: boolean;
  dryRun?: boolean;
  progress?: ProgressSink;
  /** Where the `checking` step and `verifyChecksum`'s debug line go — stands
   * in for the Rust tree's global `ui::`, the same seam `install.ts` uses. */
  reporter?: InstallReporter;
}

/** Fetch the latest ketch release and replace this binary. */
export async function update(cfg: Config, options: UpdateOptions): Promise<SelfUpdate> {
  const force = options.force ?? false;
  const dryRun = options.dryRun ?? false;
  const progress = options.progress ?? new NullProgress();
  const reporter = options.reporter ?? silentReporter;

  const lock = await Lock.acquire(cfg);
  try {
    const from = currentVersion(options.version);

    // Built-in sources only: a third-party plugin must never be in a
    // position to hand ketch its own replacement.
    const sources = builtinOnlySourceRegistry(cfg);
    const source = sources.get("github");
    reporter.step("checking", cfg.selfRepo);
    const release = await resolveRelease(
      source,
      cfg.selfRepo,
      { kind: "latest" },
      defaultListOpts(),
    );
    const to = release.version;

    if (to.compare(from) <= 0 && !force) {
      return { from, to, replaced: false, notes: null };
    }
    if (dryRun) {
      return { from, to, replaced: false, notes: release.notes };
    }

    const platform = await hostPlatform();
    const selector = { include: [], exclude: [], target: {} };
    const chosen: ScoredAsset | undefined = scoreAssets(cfg, platform, release, selector)[0];
    if (chosen === undefined) {
      throw new KetchError({
        kind: "no_compatible_asset",
        id: cfg.selfRepo,
        tag: release.tag,
        target: targetString(cfg.target),
      });
    }

    fs.mkdirSync(cfg.cacheDir, { recursive: true });
    const work = fs.mkdtempSync(path.join(cfg.cacheDir, "self-update-"));
    try {
      // The asset name is the release author's string, not ketch's. It
      // reaches a path here, so it goes through the same guard every other
      // asset name does.
      const download = path.join(work, sanitizeComponent(chosen.asset.name));
      const sha256 = await source.download(chosen.asset, download, progress);

      // `require` is hard-coded: for its own binary ketch does not accept
      // the trust-on-first-use path it allows for packages.
      await verifyChecksum(source, cfg.selfRepo, release, chosen.asset, sha256, true, reporter);

      const unpacked = path.join(work, "payload");
      await extractAuto(download, unpacked, platform.extractors());
      const fresh = findBinary(unpacked);

      const exe = currentExe();
      replaceBinary(exe, fresh);
      return { from, to, replaced: true, notes: release.notes };
    } finally {
      fs.rmSync(work, { recursive: true, force: true });
    }
  } finally {
    lock.release();
  }
}

/**
 * Swap `fresh` into `exe`, keeping the old binary until the new one has
 * shown it can run. A ketch that cannot start is a ketch that cannot fix
 * itself.
 */
export function replaceBinary(exe: string, fresh: string): void {
  const backup = path.join(path.dirname(exe), `${path.basename(exe)}.old`);

  // Rename rather than overwrite: the running image stays valid, and a
  // failed copy leaves something to put back.
  try {
    fs.renameSync(exe, backup);
  } catch (cause) {
    throw KetchError.io(exe, asError(cause));
  }

  const restore = (detail: KetchError): KetchError => {
    try {
      fs.rmSync(exe, { force: true });
    } catch {
      // best-effort, matches the Rust `let _ = std::fs::remove_file(exe)`
    }
    try {
      fs.renameSync(backup, exe);
      return detail;
    } catch (cause) {
      return KetchError.msg(
        `${detail.message}; could not restore the previous binary (${asError(cause).message}): ` +
          `move ${backup} back to ${exe} by hand`,
      );
    }
  };

  // Copy, not rename: the download lives in the cache dir, which may be on a
  // different filesystem.
  try {
    fs.copyFileSync(fresh, exe);
  } catch (cause) {
    throw restore(KetchError.io(exe, asError(cause)));
  }

  if (process.platform !== "win32") {
    try {
      fs.chmodSync(exe, 0o755);
    } catch (cause) {
      throw restore(KetchError.io(exe, asError(cause)));
    }
  }

  const check = spawnSync(exe, ["--version"], { encoding: "utf8" });
  if (check.error !== undefined) {
    throw restore(KetchError.io(exe, check.error));
  }
  if (check.status !== 0) {
    throw restore(
      new KetchError({
        kind: "command",
        cmd: `${exe} --version`,
        status:
          check.signal !== null ? `signal ${check.signal}` : `exit code ${String(check.status)}`,
        stderr: check.stderr ?? "",
      }),
    );
  }
  try {
    fs.rmSync(backup, { force: true });
  } catch {
    // best-effort, matches the Rust `let _ = std::fs::remove_file(&backup)`
  }
}

/** The one file in an unpacked ketch release that is ketch. */
export function findBinary(payload: string): string {
  const wanted = process.platform === "win32" ? "ketch.exe" : "ketch";
  const found = walk(payload).find((file) => path.basename(file) === wanted);
  if (found === undefined) {
    throw new KetchError({ kind: "empty_payload", path: payload });
  }
  return found;
}

/**
 * Every regular file under `dir`, depth-first.
 *
 * Symlinks are skipped rather than followed, matching walkdir's
 * `follow_links(false)`. An unreadable subdirectory is skipped rather than
 * failing the whole scan — this is a best-effort look through someone else's
 * release archive, the same stance `extract/index.ts` takes.
 */
function walk(dir: string): string[] {
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
  const files: string[] = [];
  for (const entry of entries) {
    if (entry.isSymbolicLink()) {
      continue;
    }
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walk(full));
    } else if (entry.isFile()) {
      files.push(full);
    }
  }
  return files;
}

export interface UninstallOptions {
  /** Where a per-package uninstall failure is reported (never fatal to the
   * rest of the purge), and where `uninstall`'s own best-effort warnings —
   * e.g. a store directory that would not remove — surface too. Stands in
   * for the Rust tree's global `ui::warn`. */
  reporter?: InstallReporter;
}

/** Remove ketch itself. With `purge`, also removes the store, cache and state. */
export async function uninstallSelf(
  cfg: Config,
  purge: boolean,
  options: UninstallOptions = {},
): Promise<string[]> {
  const reporter = options.reporter ?? silentReporter;
  const removed: string[] = [];

  if (purge) {
    // Uninstall properly rather than deleting the root: links and copied
    // app bundles live outside it and would otherwise be left dangling.
    const lock = await Lock.acquire(cfg);
    try {
      const state = await State.load(cfg);
      /* oxlint-disable no-await-in-loop -- one at a time is the point here */
      for (const name of state.names()) {
        try {
          const pkg = await uninstall(cfg, state, name, reporter);
          removed.push(pkg.prefix);
        } catch (cause) {
          reporter.warn(`${name}: ${asError(cause).message}`);
        }
      }
      /* oxlint-enable no-await-in-loop */
      // Save first: if removing the tree fails, state still matches reality.
      await state.save(cfg);
    } finally {
      lock.release();
    }

    if (isDirectory(cfg.root)) {
      await fsp.rm(cfg.root, { recursive: true, force: true });
      removed.push(cfg.root);
    }
  }

  // Under --purge the binary may already have gone with the root.
  const exe = currentExe();
  if (fs.existsSync(exe)) {
    await fsp.rm(exe, { force: true });
    removed.push(exe);
  }
  return removed;
}

function isDirectory(target: string): boolean {
  const stat = fs.statSync(target, { throwIfNoEntry: false });
  return stat !== undefined && stat.isDirectory();
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}
