/**
 * The package registry: a GitHub repository laid out one folder per package.
 *
 * Every top-level folder names a package and holds a `ketch.json` describing
 * it. Anything else in the repository — README, licence, CI config — has no
 * `ketch.json` and is simply not a package, so the registry needs no index
 * file that could drift out of step with its contents.
 *
 * `ketch update` downloads the repository and replaces the local copy under
 * `<root>/registry`. Nothing fetches it implicitly: a package that resolves
 * today must keep resolving offline tomorrow.
 *
 * The entry shape and the folder-name rule live in `@ketch/schemas`
 * (`parseRegistryPackage`); this module owns fetching, the folder walk, name
 * collisions and the atomic swap-in.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { PACKAGE_FILE, parseRegistryPackage } from "@ketch/schemas";
import type { Config } from "./config.ts";
import { KetchError } from "./errors.ts";
import { TarGzExtractor, unwrapSingleDir } from "./extract/index.ts";
import { Http } from "./http.ts";
import { normalizeName } from "./model.ts";
import type { Manifest } from "./model.ts";
import { NullProgress } from "./progress.ts";
import type { ProgressSink } from "./progress.ts";

export { PACKAGE_FILE } from "@ketch/schemas";

/** True once a copy has been fetched. */
export function exists(cfg: Config): boolean {
  try {
    return fs.statSync(cfg.registryDir).isDirectory();
  } catch {
    return false;
  }
}

/** Every package in the local copy, each paired with the file it came from. */
export function load(cfg: Config, warn?: (message: string) => void): [Manifest, string][] {
  return loadDir(cfg.registryDir, warn);
}

export interface UpdateOptions {
  progress?: ProgressSink | undefined;
  warn?: ((message: string) => void) | undefined;
}

/**
 * Fetch the registry and swap it in, returning how many packages it holds.
 *
 * The download is staged and only moved into place once it parses, so a bad
 * or truncated fetch leaves the working copy alone.
 */
export async function update(cfg: Config, options: UpdateOptions = {}): Promise<number> {
  const progress = options.progress ?? new NullProgress();
  const repo = cfg.registry;

  const staging = fs.mkdtempSync(path.join(cfg.root, ".registry-update-"));
  try {
    const tarball = path.join(staging, "registry.tar.gz");
    // The API tarball endpoint follows the default branch and honours the
    // token, which keeps unauthenticated rate limits out of the way. It
    // answers 415 to the octet-stream `Accept` that asset downloads use, so
    // ask for the API media type and let it redirect to the gzip.
    const url = `https://api.github.com/repos/${repo}/tarball`;
    await new Http(cfg).download(
      url,
      tarball,
      { Accept: "application/vnd.github+json" },
      true,
      progress,
    );

    const unpacked = path.join(staging, "tree");
    fs.mkdirSync(unpacked, { recursive: true });
    await new TarGzExtractor().extract(tarball, unpacked);
    // GitHub wraps the tree in one `owner-repo-<sha>` directory.
    const root = unwrapSingleDir(unpacked);

    return swapIn(cfg, root, repo, options.warn);
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}

/**
 * Move a freshly-unpacked tree into place, returning its package count.
 *
 * A tree with no packages is refused: a repository that moved, emptied or
 * answered with something unexpected must not wipe a working registry.
 */
export function swapIn(
  cfg: Config,
  tree: string,
  repo: string,
  warn?: (message: string) => void,
): number {
  const packages = loadDir(tree, warn);
  for (const problem of collisions(packages)) {
    warn?.(problem);
  }
  const count = packages.length;
  if (count === 0) {
    throw KetchError.msg(
      `${repo} has no package folders containing \`${PACKAGE_FILE}\` ` +
        "— leaving the current registry in place",
    );
  }
  if (fs.existsSync(cfg.registryDir)) {
    try {
      fs.rmSync(cfg.registryDir, { recursive: true, force: true });
    } catch (cause) {
      throw KetchError.io(cfg.registryDir, asError(cause));
    }
  }
  try {
    fs.renameSync(tree, cfg.registryDir);
  } catch (cause) {
    throw KetchError.io(cfg.registryDir, asError(cause));
  }
  return count;
}

/**
 * Names that two packages both answer to.
 *
 * Nothing else can catch this: each folder is valid on its own, the loser is
 * shadowed silently, and which one loses depends on sort order. Reported as
 * warnings rather than errors so one careless entry cannot block an update
 * for everybody.
 */
export function collisions(packages: ReadonlyArray<readonly [Manifest, string]>): string[] {
  const claimed = new Map<string, string>();
  const out: string[] = [];
  for (const [manifest] of packages) {
    for (const name of [manifest.name, ...manifest.provides]) {
      const key = normalizeName(name);
      let first = claimed.get(key);
      if (first === undefined) {
        first = manifest.name;
        claimed.set(key, first);
      }
      if (first !== manifest.name) {
        out.push(
          `\`${name}\` is claimed by both \`${first}\` and \`${manifest.name}\`; ` +
            `only \`${first}\` will resolve`,
        );
      }
    }
  }
  return out;
}

export function loadDir(dir: string, warn?: (message: string) => void): [Manifest, string][] {
  let names: string[];
  try {
    names = fs.readdirSync(dir);
  } catch {
    return [];
  }
  const folders = names.filter((name) => isFile(path.join(dir, name, PACKAGE_FILE))).toSorted();

  const out: [Manifest, string][] = [];
  for (const name of folders) {
    const file = path.join(dir, name, PACKAGE_FILE);
    try {
      out.push([readPackage(file, name), file]);
    } catch (cause) {
      // One broken entry must not hide the rest of the registry.
      const message = cause instanceof KetchError ? cause.message : asError(cause).message;
      warn?.(`ignoring registry package \`${name}\`: ${message}`);
    }
  }
  return out;
}

function isFile(candidate: string): boolean {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
}

/**
 * Parse one package folder's file into a full, validated manifest.
 *
 * The folder is the package name, so `name` in the file is optional — and
 * when it is present it must agree, or the package would be unreachable
 * under the name its folder advertises. `parseRegistryPackage` (schemas)
 * owns that rule and the field-level validation; this just reads the file
 * and turns whatever it throws into a `KetchError` that names the file.
 */
function readPackage(file: string, folder: string): Manifest {
  let text: string;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch (cause) {
    throw KetchError.io(file, asError(cause));
  }
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch (cause) {
    throw KetchError.parse(file, asError(cause).message);
  }
  try {
    return parseRegistryPackage(data, folder, file);
  } catch (cause) {
    const message = detailMessage(cause);
    const prefix = `${file}: `;
    throw KetchError.parse(
      file,
      message.startsWith(prefix) ? message.slice(prefix.length) : message,
    );
  }
}

/** A zod parse failure's issues, formatted `path: message`, or a plain message. */
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
