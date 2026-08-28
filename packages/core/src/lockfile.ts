/**
 * `ketch.lock`: one machine's set of tools, pinned to exact releases.
 *
 * Not the lock in `state.ts` — that one is a mutex over the install tree,
 * held for the length of a command. This is a file the user commits next to
 * their dotfiles. `ketch lock` writes it from what is installed, `ketch sync`
 * makes another machine match, and `ketch lock --check` says whether the two
 * have drifted apart.
 *
 * What is reproducible here, and what is not:
 *
 * - The **tag** is. Every machine resolving the same tag gets the same
 *   release, which is the whole point of writing one down.
 * - The **asset and its hash** are reproducible only on the same target. A
 *   lock written on Apple Silicon names an `aarch64` tarball an Intel machine
 *   cannot run, so `sync` re-selects the asset there and verifies against the
 *   source's own checksum instead. Claiming the recorded hash still applied
 *   would be a reproducibility guarantee that quietly is not one.
 *
 * A lockfile is a file somebody else may have written — that is what sharing
 * a dotfiles repository means — so nothing in it is allowed to choose a
 * filesystem path. `sync` asks for a source at a tag and lets the usual
 * manifest resolution decide the install name, the binaries, and where they
 * land. The lock pins *which release*, never *where it goes*.
 *
 * Port of `Lockfile`/`LockedPackage`/`plan` from the Rust `lockfile.rs`. The
 * on-disk shape and its validation live in @ketch/schemas (`validateLockfile`
 * is the full port of Rust's `Lockfile::validate`); this module owns the
 * behavior around them — converting to and from `InstalledPackage`, the
 * atomic JSON write, and comparing a lockfile against a `State`. The format
 * changed from TOML to JSON in the port, so the file gained a `$schema`
 * pointer where the Rust one had a leading comment; nothing else moved.
 */

import fsp from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import type {
  Lockfile as LockfileRecord,
  LockedPackage as LockedPackageRecord,
} from "@ketch/schemas";
import {
  LOCK_FILE,
  LOCK_VERSION,
  lockfileSchema,
  schemaUrl,
  validateLockfile,
} from "@ketch/schemas";
import { KetchError } from "./errors.ts";
import type { InstalledPackage } from "./model.ts";
import { PackageRef, targetString } from "./model.ts";
import type { State } from "./state.ts";

export { LOCK_FILE, LOCK_VERSION } from "@ketch/schemas";

/** One package, pinned. */
export class LockedPackage {
  private constructor(
    /** The install name. Used to find the same manifest again, never a path. */
    readonly name: string,
    readonly source: PackageRef,
    /** Human-readable version. `tag` is what actually gets resolved. */
    readonly version: string,
    readonly tag: string,
    /** The target this entry was captured on, as `<os>-<arch>`. */
    readonly target: string,
    readonly asset: string,
    readonly sha256: string,
    readonly pinned: boolean,
  ) {}

  static fromInstalled(pkg: InstalledPackage): LockedPackage {
    return new LockedPackage(
      pkg.name,
      pkg.source,
      pkg.version.toString(),
      pkg.tag,
      targetString(pkg.target),
      pkg.asset_name,
      pkg.sha256,
      pkg.pinned,
    );
  }

  /** Rebuild from the validated on-disk record, parsing `source` back to a `PackageRef`. */
  static fromRecord(record: LockedPackageRecord, where: string): LockedPackage {
    const source = PackageRef.parse(record.source);
    if (source === null) {
      // The schema already guarantees a parseable reference; re-checked here
      // so a hand-constructed record gets the same refusal.
      throw KetchError.parse(where, `\`${record.source}\` is not a package reference`);
    }
    return new LockedPackage(
      record.name,
      source,
      record.version,
      record.tag,
      record.target,
      record.asset,
      record.sha256,
      record.pinned,
    );
  }

  toRecord(): LockedPackageRecord {
    return {
      name: this.name,
      source: this.source.toString(),
      version: this.version,
      tag: this.tag,
      target: this.target,
      asset: this.asset,
      sha256: this.sha256,
      pinned: this.pinned,
    };
  }

  /**
   * True when the recorded asset and hash describe a machine like this one.
   *
   * Only then is the hash something to hold a download to: on a different
   * target the asset is a different file, and a mismatch would be correct
   * rather than suspicious.
   */
  matchesTarget(target: string): boolean {
    return this.target === target;
  }
}

/** A whole lockfile. */
export class Lockfile {
  private constructor(
    readonly version: number,
    /** Sorted by name, so the file is stable and its diffs are readable. */
    readonly packages: readonly LockedPackage[],
  ) {}

  /** Capture what is installed right now. */
  static fromState(state: State): Lockfile {
    const packages = state.all().map((pkg) => LockedPackage.fromInstalled(pkg));
    packages.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
    return new Lockfile(LOCK_VERSION, packages);
  }

  /**
   * Read and check a lockfile.
   *
   * A missing one is not an empty one: `sync` with nothing to sync from is a
   * mistake worth naming, not a no-op to report as success.
   */
  static async load(file: string): Promise<Lockfile> {
    let text: string;
    try {
      text = await fsp.readFile(file, "utf8");
    } catch (cause) {
      if (errno(cause) === "ENOENT") {
        throw KetchError.msg(
          `no lockfile at ${file}. Run \`ketch lock\` to write one, or pass ` +
            "--file to point at another.",
        );
      }
      throw KetchError.io(file, asError(cause));
    }

    let data: unknown;
    try {
      data = JSON.parse(text);
    } catch (cause) {
      throw KetchError.parse(file, asError(cause).message);
    }
    const parsed = lockfileSchema.safeParse(data);
    if (!parsed.success) {
      throw KetchError.parse(file, zodMessage(parsed.error));
    }
    // Every check that has to pass before a single entry is acted on. One bad
    // entry fails the file rather than being skipped: unlike the registry,
    // where a partial answer beats none, a lockfile that installed most of
    // itself would not be a lock at all.
    try {
      validateLockfile(parsed.data, file);
    } catch (cause) {
      throw KetchError.msg(asError(cause).message);
    }

    const packages = parsed.data.package.map((record) => LockedPackage.fromRecord(record, file));
    return new Lockfile(parsed.data.version, packages);
  }

  /**
   * Write it out, atomically, so an interrupted write cannot leave a
   * half-parsed lockfile where a whole one used to be.
   */
  async save(file: string): Promise<void> {
    const parent = path.dirname(file);
    try {
      await fsp.mkdir(parent, { recursive: true });
    } catch (cause) {
      throw KetchError.io(parent, asError(cause));
    }

    const record: LockfileRecord = {
      $schema: schemaUrl("lockfile"),
      version: this.version,
      package: this.packages.map((pkg) => pkg.toRecord()),
    };
    const json = `${JSON.stringify(record, null, 2)}\n`;

    const staged = path.join(parent, `.${path.basename(file)}.${process.pid}.tmp`);
    let persisted = false;
    try {
      const handle = await fsp.open(staged, "wx", 0o600);
      try {
        await handle.writeFile(json, "utf8");
        // The rename is atomic, but only over whatever the file actually
        // contains. Without this the kernel is free to record the rename and
        // lose the bytes, leaving a zero-length lockfile — which is to say,
        // an empty list of pinned packages.
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
      const parentHandle = await fsp.open(parent, "r");
      try {
        await parentHandle.sync();
      } finally {
        await parentHandle.close();
      }
    } catch {
      // Nothing to do; the bytes are already on disk.
    }
  }
}

/** What `sync` would do, and what `--check` reports. */
export class Plan {
  constructor(
    /** Not installed at all. */
    readonly missing: readonly LockedPackage[],
    /** Installed, at a different tag. Carries the tag actually present. */
    readonly changed: ReadonlyArray<readonly [entry: LockedPackage, installedTag: string]>,
    /** Installed, and the lockfile says nothing about it. */
    readonly extra: readonly string[],
    /** Installed at exactly the locked tag. */
    readonly matched: number,
  ) {}

  /**
   * True when the tree already is what the lockfile describes. `--prune`
   * decides whether extras count against that, because a lockfile is a
   * record of what you want, not necessarily of all you have.
   */
  isClean(includingExtras: boolean): boolean {
    return (
      this.missing.length === 0 &&
      this.changed.length === 0 &&
      (!includingExtras || this.extra.length === 0)
    );
  }
}

/** Compare a lockfile against what is installed. */
export function plan(lock: Lockfile, state: State): Plan {
  const installed = state.all();
  const missing: LockedPackage[] = [];
  const changed: Array<[LockedPackage, string]> = [];
  let matched = 0;
  for (const entry of lock.packages) {
    const match = installed.find((pkg) => pkg.source.toString() === entry.source.toString());
    if (match === undefined) {
      missing.push(entry);
    } else if (match.tag === entry.tag) {
      matched += 1;
    } else {
      changed.push([entry, match.tag]);
    }
  }

  // Matched on source rather than name: a package can be renamed upstream, or
  // installed under an alias, and still be the same thing to update.
  const extra: string[] = [];
  for (const pkg of installed) {
    const found = lock.packages.some((entry) => entry.source.toString() === pkg.source.toString());
    if (!found) {
      extra.push(pkg.name);
    }
  }

  return new Plan(missing, changed, extra, matched);
}

/** Where the lockfile lives: what was asked for, or `ketch.lock` here. */
export function lockfilePath(explicit?: string): string {
  return explicit ?? LOCK_FILE;
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
