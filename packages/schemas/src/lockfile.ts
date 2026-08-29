/**
 * `ketch.lock`: one machine's set of tools, pinned to exact releases.
 *
 * Port of `Lockfile`/`LockedPackage` and their validation from the Rust
 * `lockfile.rs`. A lockfile is a file somebody else may have written — that
 * is what sharing a dotfiles repository means — so nothing in it is allowed
 * to choose a filesystem path: the lock pins *which release*, never *where
 * it goes*. One bad entry fails the whole file rather than being skipped; a
 * lockfile that installed most of itself would not be a lock at all.
 */

import { z } from "zod";
import { packageRefStringSchema } from "./manifest.ts";
import { asciiLowercase, parsePackageRef, sanitizeComponent, validateRepo } from "./util.ts";

/** Bumped only when the on-disk shape changes incompatibly. */
export const LOCK_VERSION = 1;

/** The name looked for when `--file` is not given. */
export const LOCK_FILE = "ketch.lock";

/** One package, pinned. */
export const lockedPackageSchema = z.strictObject({
  /** The install name. Used to find the same manifest again, never a path. */
  name: z.string(),
  source: packageRefStringSchema,
  /** Human-readable version. `tag` is what actually gets resolved. */
  version: z.string(),
  tag: z.string(),
  /** The target this entry was captured on, as `<os>-<arch>`. */
  target: z.string(),
  asset: z.string(),
  sha256: z.string(),
  pinned: z.boolean().default(false),
});
export type LockedPackage = z.infer<typeof lockedPackageSchema>;

/** A whole lockfile. The entry list keeps serde's `package` key. */
export const lockfileSchema = z.strictObject({
  $schema: z.string().optional(),
  version: z.int().nonnegative(),
  /** Sorted by name, so the file is stable and its diffs are readable. */
  package: z.array(lockedPackageSchema).default([]),
});
export type Lockfile = z.infer<typeof lockfileSchema>;

function isSha256(text: string): boolean {
  return /^[0-9a-fA-F]{64}$/.test(text);
}

/**
 * Every check that has to pass before a single entry is acted on. `where`
 * names the file in error messages, so the user knows which one to fix.
 */
export function validateLockfile(lock: Lockfile, where: string = LOCK_FILE): void {
  if (lock.version > LOCK_VERSION) {
    throw new Error(
      `${where} was written by a newer ketch (lock version ${lock.version}); ` +
        "upgrade with `ketch self update`",
    );
  }
  const seen = new Set<string>();
  for (const pkg of lock.package) {
    const named = `${where}: package \`${pkg.name}\``;
    if (pkg.name.trim() === "") {
      throw new Error(`${where}: a package has no name`);
    }
    // The name is matched against installed packages and shown to the user.
    // A name that would have to be rewritten to be usable is a name that
    // does not mean what it says.
    if (sanitizeComponent(pkg.name) !== pkg.name) {
      throw new Error(`${named} is not a usable package name`);
    }
    const lower = asciiLowercase(pkg.name);
    if (seen.has(lower)) {
      throw new Error(`${named} is listed twice`);
    }
    seen.add(lower);
    if (pkg.tag.trim() === "") {
      throw new Error(`${named} has no tag to resolve`);
    }
    const source = parsePackageRef(pkg.source);
    if (source === null) {
      // The schema already guarantees a parseable reference; repeated here so
      // hand-constructed values get the same refusal.
      throw new Error(`${named} has an unreadable source \`${pkg.source}\``);
    }
    if (source.scheme === "github") {
      validateRepo("source", source.id);
    } else if (source.id.trim() === "") {
      throw new Error(`${named} has no source id`);
    }
    if (!isSha256(pkg.sha256)) {
      throw new Error(`${named} has \`${pkg.sha256}\` where a sha256 belongs`);
    }
  }
}

/** Parse and fully validate one lockfile in a single call. */
export function parseLockfile(data: unknown, where: string = LOCK_FILE): Lockfile {
  const lock = lockfileSchema.parse(data);
  validateLockfile(lock, where);
  return lock;
}
