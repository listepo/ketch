/**
 * What is installed, on disk.
 *
 * `state.json` is the only durable record ketch keeps. It is rewritten
 * atomically — staged next to the real file and renamed — so an interrupted
 * write can never leave a half-parsed state file behind, which would look
 * exactly like "nothing is installed".
 *
 * The on-disk shape belongs to @ketch/schemas, which stores versions and
 * package references as the strings serde wrote. This module converts to and
 * from the parsed forms in `model.ts`, so nothing downstream re-parses them.
 */

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import fsp from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import type { InstalledPackage as InstalledRecord } from "@ketch/schemas";
import { asciiLowercase, STATE_VERSION, stateSchema, validateState } from "@ketch/schemas";
import type { Config } from "./config.ts";
import { KetchError } from "./errors.ts";
import type { InstalledPackage } from "./model.ts";
import { installedBinaries, PackageRef, Version } from "./model.ts";

export { STATE_VERSION } from "@ketch/schemas";

/** The installed-package record, as commands work with it. */
export class State {
  /** Keyed by package name, which is unique across sources by construction. */
  private readonly byName = new Map<string, InstalledPackage>();

  /**
   * Always the version this binary writes. Loading refuses anything newer, so
   * a file that got here is either current or older and about to be rewritten
   * in the current shape.
   */
  readonly version: number = STATE_VERSION;

  /** Read the state file. A missing file is an empty state, not an error. */
  static async load(cfg: Config): Promise<State> {
    return State.loadPath(cfg.stateFile);
  }

  static async loadPath(file: string): Promise<State> {
    let text: string;
    try {
      text = await fsp.readFile(file, "utf8");
    } catch (cause) {
      if (errno(cause) === "ENOENT") {
        return new State();
      }
      throw KetchError.io(file, asError(cause));
    }

    // An absent file means nothing has been installed yet. An empty one means
    // a write was lost, and every package on disk is about to be forgotten —
    // say so rather than quietly starting over.
    if (text.trim() === "") {
      throw KetchError.msg(
        `${file} is empty, which usually means an interrupted write. Anything ` +
          "already installed is still in the store; remove the file to start " +
          "a fresh record, then `ketch relink` each package.",
      );
    }

    let data: unknown;
    try {
      data = JSON.parse(text);
    } catch (cause) {
      throw KetchError.parse(file, asError(cause).message);
    }
    const parsed = stateSchema.safeParse(data);
    if (!parsed.success) {
      throw KetchError.parse(file, zodMessage(parsed.error));
    }
    try {
      validateState(parsed.data, file);
    } catch (cause) {
      throw KetchError.msg(asError(cause).message);
    }

    const state = new State();
    for (const [name, record] of Object.entries(parsed.data.packages)) {
      state.byName.set(name, fromRecord(record, file));
    }
    return state;
  }

  async save(cfg: Config): Promise<void> {
    await this.savePath(cfg.stateFile);
  }

  async savePath(file: string): Promise<void> {
    const parent = path.dirname(file);
    try {
      await fsp.mkdir(parent, { recursive: true });
    } catch (cause) {
      throw KetchError.io(parent, asError(cause));
    }

    // Sorted, matching the BTreeMap the Rust tree wrote: a state file that
    // reorders itself on every save is unreadable in a diff.
    const packages: Record<string, InstalledRecord> = {};
    for (const name of [...this.byName.keys()].toSorted()) {
      const pkg = this.byName.get(name);
      if (pkg !== undefined) {
        packages[name] = installedRecord(pkg);
      }
    }
    const json = `${JSON.stringify({ version: this.version, packages }, null, 2)}\n`;

    const staged = path.join(parent, `.${path.basename(file)}.${process.pid}.tmp`);
    let persisted = false;
    try {
      const handle = await fsp.open(staged, "wx", 0o600);
      try {
        await handle.writeFile(json, "utf8");
        // The rename is atomic, but only over whatever the file actually
        // contains. Without this the kernel is free to record the rename and
        // lose the bytes, leaving a zero-length state file — which is to say,
        // an empty list of installed packages.
        await handle.sync();
      } finally {
        await handle.close();
      }
      await fsp.rename(staged, file);
      persisted = true;
    } catch (cause) {
      throw KetchError.io(persisted ? file : staged, asError(cause));
    } finally {
      if (!persisted) {
        await fsp.rm(staged, { force: true }).catch(() => {});
      }
    }

    // Then make the rename itself durable. Best effort: some filesystems
    // refuse to fsync a directory, and that is not a reason to fail a save.
    try {
      const dir = await fsp.open(parent, "r");
      try {
        await dir.sync();
      } finally {
        await dir.close();
      }
    } catch {
      // Nothing to do; the bytes are already on disk.
    }
  }

  get(name: string): InstalledPackage | undefined {
    return this.byName.get(name);
  }

  insert(pkg: InstalledPackage): void {
    this.byName.set(pkg.name, pkg);
  }

  remove(name: string): InstalledPackage | undefined {
    const gone = this.byName.get(name);
    this.byName.delete(name);
    return gone;
  }

  /** Install names, sorted, matching the order they are stored in. */
  names(): string[] {
    return [...this.byName.keys()].toSorted();
  }

  /** Every installed package, in name order. */
  all(): InstalledPackage[] {
    return this.names()
      .map((name) => this.byName.get(name))
      .filter(isPresent);
  }

  get size(): number {
    return this.byName.size;
  }

  /**
   * Look a package up the way a user would name it: by install name, by a
   * binary it provides, or by its source id (`owner/repo`).
   */
  find(query: string): InstalledPackage | undefined {
    const direct = this.byName.get(query);
    if (direct !== undefined) {
      return direct;
    }
    const wanted = asciiLowercase(query);
    return this.all().find(
      (pkg) =>
        asciiLowercase(pkg.source.id) === wanted ||
        asciiLowercase(pkg.source.toString()) === wanted ||
        installedBinaries(pkg).some((link) => path.basename(link.link) === query),
    );
  }
}

// ---------------------------------------------------------------------------
// Locking
// ---------------------------------------------------------------------------

/**
 * Exclusive access to the install tree, released by `release`.
 *
 * Two `ketch install` runs writing the same `state.json` would each save a
 * view that omits the other's package, silently losing an install. The lock is
 * advisory between ketch processes only — nothing else writes this tree.
 */
export class Lock {
  private readonly file: string;
  /**
   * False when we adopted our own process's existing lock (re-entrancy), in
   * which case releasing must not delete it.
   */
  private readonly owned: boolean;

  private constructor(file: string, owned: boolean) {
    this.file = file;
    this.owned = owned;
  }

  /** Take the lock, or fail with the pid currently holding it. */
  static async acquire(cfg: Config, debug?: (message: string) => void): Promise<Lock> {
    return Lock.acquirePath(cfg.lockFile, debug);
  }

  static async acquirePath(file: string, debug?: (message: string) => void): Promise<Lock> {
    const parent = path.dirname(file);
    try {
      await fsp.mkdir(parent, { recursive: true });
    } catch (cause) {
      throw KetchError.io(parent, asError(cause));
    }
    const me = process.pid;

    // Sequential on purpose: the second attempt exists only to see what the
    // first one's reclaim left behind. Nothing here can be run in parallel.
    /* oxlint-disable no-await-in-loop */
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        const handle = await fsp.open(file, "wx");
        try {
          await handle.writeFile(String(me), "utf8");
        } finally {
          await handle.close();
        }
        return new Lock(file, true);
      } catch (cause) {
        if (errno(cause) !== "EEXIST") {
          throw KetchError.io(file, asError(cause));
        }
      }

      const holder = await readPid(file);
      if (holder === me) {
        return new Lock(file, false);
      }
      if (holder !== undefined && processAlive(holder)) {
        throw new KetchError({ kind: "locked", detail: `pid ${holder}` });
      }
      if (attempt > 0) {
        throw new KetchError({ kind: "locked", detail: file });
      }

      // A crashed run left the file behind. Reclaim it by renaming rather than
      // unlinking: `rename` fails if the file is already gone, so of two
      // processes that both judge the lock stale exactly one can claim it.
      // Plain `unlink` succeeds for both — including for the one that would
      // delete the winner's fresh lock — and they would then both proceed.
      debug?.(
        `clearing stale lock ${file} (${holder === undefined ? "unreadable" : `pid ${holder} is gone`})`,
      );
      const reclaimed = `${file}.stale.${me}`;
      try {
        await fsp.rename(file, reclaimed);
        await fsp.rm(reclaimed, { force: true });
      } catch {
        // Someone else won the reclaim; the retry will see their lock.
      }
    }
    /* oxlint-enable no-await-in-loop */
    throw new KetchError({ kind: "locked", detail: file });
  }

  /** Give the lock up. Best effort: a failure here cannot fail the command. */
  release(): void {
    if (this.owned) {
      try {
        fs.rmSync(this.file, { force: true });
      } catch {
        // Nothing useful to do — the run is over either way.
      }
    }
  }
}

/**
 * Is that pid still running? Only consulted when a lock file already exists,
 * so spawning costs nothing on the normal path.
 *
 * `ps` rather than `kill -0`: signalling a process owned by another user fails
 * with EPERM, which is indistinguishable from "no such process" through an
 * exit status alone — and reading it as "gone" steals a lock that is very much
 * still held.
 */
function processAlive(pid: number): boolean {
  const result = spawnSync("/bin/ps", ["-p", String(pid), "-o", "pid="], { stdio: "ignore" });
  // Unsure means "assume held" — never steal on doubt.
  return result.error !== undefined || result.status === null ? true : result.status === 0;
}

async function readPid(file: string): Promise<number | undefined> {
  try {
    const text = (await fsp.readFile(file, "utf8")).trim();
    return /^\d+$/.test(text) ? Number(text) : undefined;
  } catch {
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// Record conversion
// ---------------------------------------------------------------------------

/**
 * The on-disk form of an installed package.
 *
 * Exported because `ketch list --json` must print the same field names the
 * state file uses: two shapes for one record would mean a JSON consumer and a
 * state reader disagreeing about what a package is.
 */
export function installedRecord(pkg: InstalledPackage): InstalledRecord {
  return {
    name: pkg.name,
    version: pkg.version.toString(),
    source: pkg.source.toString(),
    tag: pkg.tag,
    target: pkg.target,
    asset_name: pkg.asset_name,
    sha256: pkg.sha256,
    checksum_verified: pkg.checksum_verified,
    installed_at: pkg.installed_at,
    prefix: pkg.prefix,
    links: pkg.links,
    pinned: pkg.pinned,
    origin: pkg.origin,
    manifest: pkg.manifest,
  };
}

function fromRecord(record: InstalledRecord, where: string): InstalledPackage {
  const source = PackageRef.parse(record.source);
  if (source === null) {
    throw KetchError.parse(where, `\`${record.source}\` is not a package reference`);
  }
  return {
    name: record.name,
    version: Version.parse(record.version),
    source,
    tag: record.tag,
    target: record.target,
    asset_name: record.asset_name,
    sha256: record.sha256,
    checksum_verified: record.checksum_verified,
    installed_at: record.installed_at,
    prefix: record.prefix,
    links: record.links,
    pinned: record.pinned,
    origin: record.origin,
    manifest: record.manifest ?? null,
  };
}

function isPresent<T>(value: T | undefined): value is T {
  return value !== undefined;
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

function errno(cause: unknown): string | undefined {
  return typeof cause === "object" && cause !== null && "code" in cause
    ? String((cause as { code: unknown }).code)
    : undefined;
}

function zodMessage(error: { issues: ReadonlyArray<{ path: PropertyKey[]; message: string }> }) {
  return error.issues
    .map((issue) => (issue.path.length > 0 ? `${issue.path.join(".")}: ` : "") + issue.message)
    .join("; ");
}
