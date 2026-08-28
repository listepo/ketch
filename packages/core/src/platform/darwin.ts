/**
 * macOS.
 *
 * Asset scoring understands Apple's naming conventions (`darwin`, `apple`,
 * `universal`, `arm64` vs `aarch64`), placement knows the difference between a
 * CLI binary and a `.app` bundle, and trust checks run `codesign`/`spctl`
 * before any quarantine flag is cleared.
 */

import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { asciiLowercase } from "@ketch/schemas";
import type { Config } from "../config.ts";
import { KetchError } from "../errors.ts";
import {
  copyTree,
  DmgExtractor,
  GzFileExtractor,
  isBundleName,
  isProgramHead,
  PkgExtractor,
  RawBinaryExtractor,
  readHead,
  TarBz2Extractor,
  TarExtractor,
  TarGzExtractor,
  TarXzExtractor,
  ZipExtractor,
} from "../extract/index.ts";
import type { Extractor } from "../extract/extractor.ts";
import type { Arch, BinSpec, LinkRecord, TargetSpec } from "../model.ts";
import { ARCH_TOKENS, globMatch, hostTarget, OS_TOKENS } from "../model.ts";
import type { AssetScore, DoctorCheck, Placement, Platform, TrustVerdict } from "./platform.ts";
import {
  doctorFail,
  doctorOk,
  doctorWarn,
  isSidecar,
  NON_BINARY_TOKENS,
  REJECTED_EXTENSIONS,
} from "./platform.ts";

/** Directories inside a payload that never hold the program itself. */
const NOISE_DIRS: ReadonlySet<string> = new Set([
  "share",
  "doc",
  "docs",
  "man",
  "completions",
  "complete",
  "etc",
  "lib",
  "include",
  "licenses",
  "_internal",
  "resources",
]);

/** Extra tokens that mark a macOS asset as a build by-product. */
const BYPRODUCT_TOKENS: readonly string[] = ["dsym", "debuginfo", "symbols"];

// ---------------------------------------------------------------------------
// Asset scoring
// ---------------------------------------------------------------------------

function isAsciiAlphanumeric(code: number): boolean {
  return (
    (code >= 0x30 && code <= 0x39) ||
    (code >= 0x41 && code <= 0x5a) ||
    (code >= 0x61 && code <= 0x7a)
  );
}

/**
 * Does `needle` appear in `haystack` as a whole token?
 *
 * Plain `includes` is not usable here: `darwin` contains `win`, `install`
 * contains `all`, and either would misroute an asset to the wrong platform.
 */
export function tokenAt(haystack: string, needle: string): boolean {
  if (needle === "") {
    return false;
  }
  let from = 0;
  for (;;) {
    const start = haystack.indexOf(needle, from);
    if (start === -1) {
      return false;
    }
    const end = start + needle.length;
    const left = start === 0 || !isAsciiAlphanumeric(haystack.charCodeAt(start - 1));
    const right = end === haystack.length || !isAsciiAlphanumeric(haystack.charCodeAt(end));
    if (left && right) {
      return true;
    }
    from = start + 1;
  }
}

function findToken(haystack: string, tokens: readonly string[]): string | null {
  return tokens.find((t) => tokenAt(haystack, t)) ?? null;
}

/**
 * Bonus and label for the container format, read off the file name.
 *
 * The spread is deliberately small: it only breaks ties between assets that
 * already agree on OS and architecture.
 */
const KNOWN_CONTAINERS: readonly [string, number, string][] = [
  [".tar.gz", 8, "tar.gz"],
  [".tgz", 8, "tar.gz"],
  [".tar.xz", 7, "tar.xz"],
  [".txz", 7, "tar.xz"],
  [".tar.bz2", 5, "tar.bz2"],
  [".tar", 6, "tar"],
  [".zip", 6, "zip"],
  [".gz", 5, "gz"],
  [".dmg", 3, "dmg"],
  [".pkg", 2, "pkg"],
];

function containerBonus(lower: string): [number, string] {
  for (const [suffix, bonus, label] of KNOWN_CONTAINERS) {
    if (lower.endsWith(suffix)) {
      return [bonus, label];
    }
  }
  // No recognised container: most likely the bare executable.
  return [4, "raw"];
}

function isRejected(lower: string): boolean {
  return (
    isSidecar(lower) ||
    NON_BINARY_TOKENS.some((t) => lower.includes(t)) ||
    REJECTED_EXTENSIONS.some((e) => lower.endsWith(e)) ||
    BYPRODUCT_TOKENS.some((t) => tokenAt(lower, t))
  );
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/** What running a system tool produced: success flag and combined output. */
export interface RunResult {
  ok: boolean;
  output: string;
}

/**
 * The seam between trust checks and the real system tools, so tests can
 * substitute canned `codesign`/`spctl` transcripts for the binaries.
 */
export type ToolRunner = (program: string, args: readonly string[]) => Promise<RunResult>;

/**
 * Run a tool and capture stdout and stderr together.
 *
 * `codesign` reports everything interesting on stderr, so splitting the two
 * would just mean reassembling them at every call site. Arguments are passed
 * as a vector — never a shell string — and stdin is closed so a tool that
 * decides to prompt fails fast instead of hanging.
 */
function systemRunner(program: string, args: readonly string[]): Promise<RunResult> {
  return new Promise((resolve) => {
    const child = spawn(program, [...args], { stdio: ["ignore", "pipe", "pipe"] });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", (cause) => resolve({ ok: false, output: cause.message }));
    child.on("close", (code) =>
      resolve({
        ok: code === 0,
        output: Buffer.concat(stdout).toString("utf8") + Buffer.concat(stderr).toString("utf8"),
      }),
    );
  });
}

function firstLine(text: string): string {
  return (
    text
      .split(/\r?\n/)
      .map((l) => l.trim())
      .find((l) => l !== "") ?? "no output"
  );
}

// ---------------------------------------------------------------------------
// Placement helpers
// ---------------------------------------------------------------------------

function toError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

/**
 * Remove whatever is at `target` — file, symlink or directory — treating "it
 * was not there" as success.
 */
function removeAny(target: string): void {
  let meta: fs.Stats;
  try {
    meta = fs.lstatSync(target);
  } catch (cause) {
    if ((cause as NodeJS.ErrnoException).code === "ENOENT") {
      return;
    }
    throw cause;
  }
  if (meta.isDirectory()) {
    fs.rmSync(target, { recursive: true, force: true });
  } else {
    fs.unlinkSync(target);
  }
}

/**
 * A path next to `original`, so swapping the two is a rename that never
 * crosses a filesystem.
 */
function sibling(original: string, suffix: string): string {
  return path.join(path.dirname(original), path.basename(original) + suffix);
}

/**
 * Move the extracted payload to its final home, falling back to a copy when
 * the cache and the store are on different filesystems.
 *
 * The replacement is assembled beside the destination and swapped in last.
 * Deleting the old directory first — as the obvious version does — means an
 * upgrade that fails while copying leaves the user with no working version of
 * a package they already had installed.
 */
export async function moveIntoStore(payload: string, store: string): Promise<void> {
  // `relink` re-runs placement over a payload that is already in the store;
  // without this the swap below would move it out from under itself.
  if (payload === store) {
    return;
  }
  const parent = path.dirname(store);
  try {
    fs.mkdirSync(parent, { recursive: true });
  } catch (cause) {
    throw KetchError.io(parent, toError(cause));
  }

  const staged = sibling(store, ".incoming");
  try {
    removeAny(staged);
  } catch {
    // Best effort: a stale stage is rebuilt below anyway.
  }
  let renamed = true;
  try {
    fs.renameSync(payload, staged);
  } catch {
    renamed = false;
  }
  if (!renamed) {
    await copyTree(payload, staged);
  }

  let storeOccupied = true;
  try {
    fs.lstatSync(store);
  } catch {
    storeOccupied = false;
  }
  if (!storeOccupied) {
    try {
      fs.renameSync(staged, store);
    } catch (cause) {
      throw KetchError.io(store, toError(cause));
    }
    return;
  }
  const retired = sibling(store, ".old");
  try {
    removeAny(retired);
  } catch {
    // Best effort, as above.
  }
  try {
    fs.renameSync(store, retired);
  } catch (cause) {
    throw KetchError.io(store, toError(cause));
  }
  try {
    fs.renameSync(staged, store);
  } catch (cause) {
    // Put back the version that was working before reporting the failure.
    try {
      fs.renameSync(retired, store);
    } catch {
      // The original rename out succeeded, so this one is expected to too.
    }
    try {
      removeAny(staged);
    } catch {
      // Leftover stage; the next install clears it.
    }
    throw KetchError.io(store, toError(cause));
  }
  try {
    removeAny(retired);
  } catch {
    // A lingering `.old` is untidy, not a failed install.
  }
}

/** Grant execute permission, changing nothing else about the mode. */
export function ensureExecutable(file: string): void {
  let meta: fs.Stats;
  try {
    meta = fs.statSync(file);
  } catch (cause) {
    throw KetchError.io(file, toError(cause));
  }
  const mode = meta.mode & 0o7777;
  if ((mode & 0o111) !== 0) {
    return;
  }
  try {
    fs.chmodSync(file, mode | 0o111);
  } catch (cause) {
    throw KetchError.io(file, toError(cause));
  }
}

/**
 * True when `file` sits inside *another* bundle — a helper app nested in the
 * one being installed, which must not be placed in the applications directory
 * on its own.
 *
 * Only the ancestors below `root` count. Testing the whole relative path
 * includes the leaf, and since every `.app` ends in `.app`, that answered
 * "inside a bundle" for every bundle there is.
 */
function isInsideBundle(file: string, root: string): boolean {
  const rel = path.relative(root, file);
  if (rel === "" || rel.startsWith("..") || path.isAbsolute(rel)) {
    return false;
  }
  return rel.split(path.sep).slice(0, -1).some(isBundleName);
}

/**
 * A raw binary asset lands under the asset's own file name — `jq-macos-arm64`
 * — which is not what anyone wants on PATH. Rename to the package name only
 * when the discovered name plainly carries build metadata and there is no
 * second binary that the rename could collide with.
 */
export function looksLikeBuildArtifact(name: string): boolean {
  const lower = asciiLowercase(name);
  const platformToken = [
    OS_TOKENS.macos,
    ARCH_TOKENS.aarch64,
    ARCH_TOKENS.x86_64,
    ARCH_TOKENS.universal,
  ].some((set) => set.some((t) => tokenAt(lower, t)));

  return platformToken || hasVersionRun(lower);
}

/** True for names carrying something like `1.2` or `v3`. */
function hasVersionRun(lower: string): boolean {
  for (let i = 0; i < lower.length - 1; i += 1) {
    const a = lower.charCodeAt(i);
    const b = lower.charCodeAt(i + 1);
    if (lower.charAt(i) === "v" && b >= 0x30 && b <= 0x39) {
      return true;
    }
    if (i + 2 < lower.length) {
      const c = lower.charCodeAt(i + 2);
      if (a >= 0x30 && a <= 0x39 && lower.charAt(i + 1) === "." && c >= 0x30 && c <= 0x39) {
        return true;
      }
    }
  }
  return false;
}

function isExecutableSync(file: string): boolean {
  let meta: fs.Stats;
  try {
    meta = fs.statSync(file);
  } catch {
    return false;
  }
  if (!meta.isFile() || (meta.mode & 0o111) === 0) {
    return false;
  }
  // The bit alone is not enough: tarballs routinely ship +x READMEs.
  try {
    return isProgramHead(readHead(file));
  } catch {
    return false;
  }
}

/**
 * Walk regular entries below `root`, depth-first, never following symlinks.
 * `maxDepth` counts levels below `root`; unreadable directories are skipped.
 */
function walk(
  root: string,
  maxDepth: number,
  prune: (name: string) => boolean,
  visit: (entryPath: string, isDir: boolean) => void,
): void {
  const recurse = (dir: string, depth: number): void => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (prune(entry.name)) {
        continue;
      }
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        visit(full, true);
        if (depth < maxDepth) {
          recurse(full, depth + 1);
        }
      } else if (entry.isFile()) {
        visit(full, false);
      }
    }
  };
  recurse(root, 1);
}

/** Every executable file in the payload that is a plausible entry point. */
function discoverExecutables(root: string): string[] {
  const found: string[] = [];
  walk(
    root,
    4,
    // Never descend into a bundle or a docs tree looking for a CLI.
    (name) => {
      const lower = asciiLowercase(name);
      return lower.endsWith(".app") || lower.endsWith(".framework") || NOISE_DIRS.has(lower);
    },
    (entryPath, isDir) => {
      if (!isDir && isExecutableSync(entryPath)) {
        found.push(entryPath);
      }
    },
  );

  // A `bin/` directory is an explicit statement about what to expose.
  const inBin = found.filter((p) => path.basename(path.dirname(p)) === "bin");
  const result = inBin.length > 0 ? inBin : found;
  result.sort();
  return result;
}

/** Top-level `.app` bundles in the payload, helpers inside them excluded. */
export function findAppBundles(root: string): string[] {
  const found: string[] = [];
  if (path.extname(root) === ".app") {
    found.push(root);
  }
  walk(
    root,
    3,
    () => false,
    (entryPath, isDir) => {
      if (isDir && path.extname(entryPath) === ".app" && !isInsideBundle(entryPath, root)) {
        found.push(entryPath);
      }
    },
  );
  return found;
}

/** Resolve the manifest's explicit binary list against the extracted payload. */
function resolveBinSpecs(root: string, specs: readonly BinSpec[]): [string, string][] {
  const candidates: string[] = [];
  walk(
    root,
    6,
    () => false,
    (entryPath, isDir) => {
      if (!isDir) {
        candidates.push(entryPath);
      }
    },
  );

  const out: [string, string][] = [];
  for (const spec of specs) {
    const pattern = spec.path;
    const matched =
      pattern !== null && pattern !== undefined
        ? candidates.find((p) => globMatch(pattern, path.relative(root, p)))
        : candidates.find((p) => path.basename(p) === (spec.name ?? ""));
    if (matched === undefined) {
      throw KetchError.msg(
        `manifest expects \`${spec.path ?? spec.name ?? "<unnamed>"}\` but the release payload does not contain it`,
      );
    }
    out.push([matched, spec.name ?? path.basename(matched)]);
  }
  return out;
}

/**
 * Whether an occupied destination is this package's own to replace.
 *
 * Two kinds of evidence. A symlink pointing into `owned` — the package's
 * directory in the store, covering every version of it — was made by ketch for
 * this package. A copied `.app` leaves no mark on disk at all, so the only
 * evidence there is the record written when it was placed.
 *
 * Anything else is somebody else's: another package that claims the same
 * binary name, or an application the user installed themselves. Taking one
 * over silently means uninstalling this package later deletes it.
 */
function isOurs(link: string, owned: string, recorded: readonly LinkRecord[]): boolean {
  if (recorded.some((r) => r.link === link)) {
    return true;
  }
  let target: string;
  try {
    target = fs.readlinkSync(link);
  } catch {
    return false;
  }
  // Whole path components, as Rust `Path::starts_with` compares: the store
  // directory `mine` must not claim a sibling named `mine2`.
  return target === owned || target.startsWith(owned + path.sep);
}

/** Clear a destination, or explain who already has it. */
function clearDestination(link: string, owned: string, recorded: readonly LinkRecord[]): void {
  destinationAvailable(link, owned, recorded);
  try {
    removeAny(link);
  } catch (cause) {
    throw KetchError.io(link, toError(cause));
  }
}

function destinationAvailable(link: string, owned: string, recorded: readonly LinkRecord[]): void {
  let occupied = true;
  try {
    fs.lstatSync(link);
  } catch {
    occupied = false;
  }
  if (occupied && !isOurs(link, owned, recorded)) {
    throw KetchError.msg(
      `${link} already exists and was not installed by ketch for this package; move it aside first`,
    );
  }
}

/**
 * The discovered executables paired with the names they will take on PATH,
 * applying the lone-build-artifact rename.
 */
function discoveredTargets(root: string, packageName: string): [string, string][] {
  const found = discoverExecutables(root);
  const sole = found.length === 1;
  return found.map((entryPath) => {
    const fileName = path.basename(entryPath);
    const linkName = sole && looksLikeBuildArtifact(fileName) ? packageName : fileName;
    return [entryPath, linkName];
  });
}

/**
 * Check all destinations before replacing any old links. A multi-binary
 * upgrade must not install its first link and only then discover that its
 * second name belongs to another package.
 */
export function preflightDestinations(plan: Placement, owned: string): void {
  const bundles = plan.kind !== "binary" ? findAppBundles(plan.payloadDir) : [];
  const wantBinaries =
    plan.kind === "app" ? false : plan.kind === "binary" ? true : bundles.length === 0;
  const binaries = wantBinaries
    ? plan.binSpecs.length === 0
      ? discoveredTargets(plan.payloadDir, plan.name).map(([, name]) => name)
      : resolveBinSpecs(plan.payloadDir, plan.binSpecs).map(([, name]) => name)
    : [];

  const destinations = new Set<string>();
  for (const bundle of bundles) {
    const link = path.join(plan.appsDir, path.basename(bundle));
    if (destinations.has(link)) {
      throw KetchError.msg(`multiple payload entries want to create ${link}`);
    }
    destinations.add(link);
    destinationAvailable(link, owned, plan.replacing);
  }
  for (const name of binaries) {
    const link = path.join(plan.binDir, name);
    if (destinations.has(link)) {
      throw KetchError.msg(`multiple payload entries want to create ${link}`);
    }
    destinations.add(link);
    destinationAvailable(link, owned, plan.replacing);
  }
  if (destinations.size === 0) {
    throw new KetchError({ kind: "empty_payload", path: plan.payloadDir });
  }
}

/** Symlink one executable into the bin directory. */
export function linkBinary(
  target: string,
  binDir: string,
  name: string,
  owned: string,
  recorded: readonly LinkRecord[],
): LinkRecord {
  try {
    fs.mkdirSync(binDir, { recursive: true });
  } catch (cause) {
    throw KetchError.io(binDir, toError(cause));
  }
  const link = path.join(binDir, name);
  clearDestination(link, owned, recorded);

  // zip archives and `ditto` both lose the execute bit often enough.
  ensureExecutable(target);
  try {
    fs.symlinkSync(target, link);
  } catch (cause) {
    throw KetchError.io(link, toError(cause));
  }
  return { link, target, kind: "symlink" };
}

/** Put one `.app` bundle into the applications directory. */
export async function placeApp(
  bundle: string,
  appsDir: string,
  linkApps: boolean,
  owned: string,
  recorded: readonly LinkRecord[],
): Promise<LinkRecord> {
  try {
    fs.mkdirSync(appsDir, { recursive: true });
  } catch (cause) {
    throw KetchError.io(appsDir, toError(cause));
  }
  const link = path.join(appsDir, path.basename(bundle));
  clearDestination(link, owned, recorded);

  if (linkApps) {
    try {
      fs.symlinkSync(bundle, link);
    } catch (cause) {
      throw KetchError.io(link, toError(cause));
    }
    return { link, target: bundle, kind: "linked_app" };
  }
  // Copied by default: Launchpad and Spotlight both ignore symlinked apps.
  await copyTree(bundle, link);
  return { link, target: bundle, kind: "copied_app" };
}

function writable(dir: string): string | null {
  if (!fs.existsSync(dir)) {
    return "does not exist";
  }
  try {
    const probe = fs.mkdtempSync(path.join(dir, ".ketch-probe"));
    fs.rmdirSync(probe);
    return null;
  } catch (cause) {
    return toError(cause).message;
  }
}

// ---------------------------------------------------------------------------

/** Test seams; production callers pass nothing. */
export interface DarwinPlatformOptions {
  /** Runs `codesign`/`spctl`/`xattr`. Defaults to the real system tools. */
  runner?: ToolRunner | undefined;
  /** Whether a system tool is installed. Defaults to a filesystem check. */
  toolExists?: ((program: string) => boolean) | undefined;
  /**
   * Where `unplace` explains a link it left alone. Core cannot reach the
   * terminal — the UI sits above it — so the caller supplies the sink.
   */
  debug?: ((message: string) => void) | undefined;
}

/** The macOS implementation of `Platform`. */
export function createDarwinPlatform(options: DarwinPlatformOptions = {}): Platform {
  const run = options.runner ?? systemRunner;
  const toolExists = options.toolExists ?? ((program: string) => fs.existsSync(program));
  const debug = options.debug ?? (() => {});
  const target: TargetSpec = hostTarget();

  return {
    id: "macos",

    target(): TargetSpec {
      return target;
    },

    scoreAsset(assetName: string, allowEmulation: boolean): AssetScore | null {
      const lower = asciiLowercase(assetName.trim());
      if (lower === "" || isRejected(lower)) {
        return null;
      }

      // Anything that names a foreign OS is not ours, whatever else it says.
      if (
        findToken(lower, OS_TOKENS.linux) !== null ||
        findToken(lower, OS_TOKENS.windows) !== null
      ) {
        return null;
      }

      let score = 0;
      const reason: string[] = [];
      const osToken = findToken(lower, OS_TOKENS.macos);
      if (osToken !== null) {
        score += 50;
        reason.push(osToken);
      } else {
        // No OS in the name at all: single-platform projects do this, so it
        // stays a candidate but loses to anything explicit.
        score += 15;
      }

      const host = target.arch;
      let arch: Arch;
      let emulated = false;
      if (findToken(lower, ARCH_TOKENS[host]) !== null) {
        score += 40;
        arch = host;
      } else if (findToken(lower, ARCH_TOKENS.universal) !== null) {
        score += 35;
        arch = "universal";
      } else if (host === "aarch64" && findToken(lower, ARCH_TOKENS.x86_64) !== null) {
        score += 10;
        arch = "x86_64";
        emulated = true;
      } else if (
        findToken(lower, ARCH_TOKENS.aarch64) !== null ||
        findToken(lower, ARCH_TOKENS.x86_64) !== null
      ) {
        // Names a real architecture, just not one this machine can run.
        return null;
      } else {
        score += 18;
        arch = "universal";
      }

      if (emulated) {
        if (!allowEmulation) {
          return null;
        }
        reason.push("x86_64 under Rosetta");
      } else {
        reason.push(arch);
      }

      const [bonus, container] = containerBonus(lower);
      score += bonus;
      reason.push(container);

      return { score, arch, emulated, reason: reason.join(" / ") };
    },

    extractors(): Extractor[] {
      return [
        new DmgExtractor(),
        new PkgExtractor(),
        new TarGzExtractor(),
        new TarXzExtractor(),
        new TarBz2Extractor(),
        new TarExtractor(),
        new ZipExtractor(),
        new GzFileExtractor(),
        // Accepts anything, so it must stay last.
        new RawBinaryExtractor(),
      ];
    },

    async place(plan: Placement): Promise<LinkRecord[]> {
      const packageDir = path.dirname(plan.storeDir);
      if (plan.link) {
        preflightDestinations(plan, packageDir);
      }
      await moveIntoStore(plan.payloadDir, plan.storeDir);
      if (!plan.link) {
        return [];
      }
      const links: LinkRecord[] = [];
      // Every version of this package lives under `packageDir`. The version
      // being replaced still owns its links at this point: install retires
      // them only once placement has succeeded.

      if (plan.kind !== "binary") {
        for (const bundle of findAppBundles(plan.storeDir)) {
          links.push(
            await placeApp(bundle, plan.appsDir, plan.linkApps, packageDir, plan.replacing),
          );
        }
      }

      // An app bundle carries its own executables; do not also scatter them
      // across PATH.
      const wantBinaries =
        plan.kind === "app" ? false : plan.kind === "binary" ? true : links.length === 0;
      if (wantBinaries) {
        const targets =
          plan.binSpecs.length === 0
            ? discoveredTargets(plan.storeDir, plan.name)
            : resolveBinSpecs(plan.storeDir, plan.binSpecs);
        for (const [file, name] of targets) {
          links.push(linkBinary(file, plan.binDir, name, packageDir, plan.replacing));
        }
      }

      if (links.length === 0) {
        throw new KetchError({ kind: "empty_payload", path: plan.storeDir });
      }
      return links;
    },

    unplace(links: readonly LinkRecord[]): Promise<void> {
      for (const record of links) {
        // A symlink that no longer points where we put it belongs to
        // something else now — another package that took the name over, or
        // the user. Removing it would break whatever owns it.
        let pointsAt: string | null;
        try {
          pointsAt = fs.readlinkSync(record.link);
        } catch {
          pointsAt = null;
        }
        if (pointsAt !== null && pointsAt !== record.target) {
          debug(`leaving ${record.link}: it no longer points at ${record.target}`);
          continue;
        }
        // Anything already gone is fine: uninstall stays idempotent.
        try {
          removeAny(record.link);
        } catch (cause) {
          throw KetchError.io(record.link, toError(cause));
        }
      }
      return Promise.resolve();
    },

    async verifyTrust(file: string): Promise<TrustVerdict> {
      if (!toolExists("/usr/bin/codesign")) {
        return { kind: "not_applicable" };
      }
      const verify = await run("/usr/bin/codesign", ["--verify", "--strict", "--", file]);
      if (!verify.ok) {
        return { kind: "untrusted", detail: firstLine(verify.output) };
      }

      const info = await run("/usr/bin/codesign", ["-dv", "--verbose=4", "--", file]);
      const authority = info.output
        .split(/\r?\n/)
        .map((l) => l.trim())
        .find((l) => l.startsWith("Authority="))
        ?.slice("Authority=".length);

      if (authority === undefined) {
        // Valid but ad-hoc: the signature proves nothing about origin.
        return { kind: "weak", detail: "ad-hoc signature, no signing authority" };
      }
      if (!authority.startsWith("Developer ID")) {
        return {
          kind: "weak",
          detail: `signed by ${authority}, which is not a distribution identity`,
        };
      }

      // Only a notarized binary passes system policy; a Developer ID
      // signature on its own does not.
      const assess = await run("/usr/sbin/spctl", ["--assess", "--type", "exec", "--", file]);
      if (assess.ok) {
        return { kind: "trusted", authority };
      }
      return {
        kind: "weak",
        detail: `${authority}; system policy: ${firstLine(assess.output)}`,
      };
    },

    async clearQuarantine(file: string): Promise<void> {
      // Non-zero simply means the attribute was not there.
      await run("/usr/bin/xattr", ["-r", "-d", "com.apple.quarantine", file]);
    },

    isExecutable(file: string): Promise<boolean> {
      return Promise.resolve(isExecutableSync(file));
    },

    appBundleExtension(): string | null {
      return ".app";
    },

    doctor(cfg: Config): Promise<DoctorCheck[]> {
      const checks: DoctorCheck[] = [];

      for (const [label, dir] of [
        ["root", cfg.root],
        ["store", cfg.storeDir],
      ] as const) {
        const problem = writable(dir);
        checks.push(
          problem === null
            ? doctorOk(label, `${dir} is writable`)
            : doctorFail(label, `${dir}: ${problem}`, `mkdir -p ${dir} && chmod u+w ${dir}`),
        );
      }

      for (const tool of [
        "/usr/bin/codesign",
        "/usr/bin/xattr",
        "/usr/bin/hdiutil",
        "/usr/bin/ditto",
      ]) {
        checks.push(
          toolExists(tool)
            ? doctorOk(tool, "present")
            : doctorWarn(tool, "missing", "install the Command Line Tools: xcode-select --install"),
        );
      }

      if (target.arch === "aarch64") {
        const rosetta =
          fs.existsSync("/usr/libexec/rosetta/oahd") ||
          fs.existsSync("/Library/Apple/usr/share/rosetta");
        checks.push(
          rosetta
            ? doctorOk("rosetta", "installed — x86_64-only releases will run")
            : doctorWarn(
                "rosetta",
                "not installed — x86_64-only releases will not run",
                "softwareupdate --install-rosetta --agree-to-license",
              ),
        );
      }
      return Promise.resolve(checks);
    },
  };
}
