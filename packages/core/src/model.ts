/**
 * Shared domain types.
 *
 * Everything crossing a module boundary is defined here so sources, platforms,
 * extractors and commands agree on shapes without depending on each other.
 * The manifest and state shapes themselves live in @ketch/schemas (they are
 * validated file formats); this module re-exports them and owns the behavior
 * around them. Fields that reach a JSON file keep their Rust serde names
 * byte-for-byte, so this port reads the state a Rust ketch wrote.
 */

import process from "node:process";
import type { AssetSelector, LinkRecord, Manifest, ManifestOrigin } from "@ketch/schemas";
import { asciiLowercase, normalizeName, parsePackageRef, sanitizeComponent } from "@ketch/schemas";
import type { SemVer } from "semver";
import { compareBuild, parse as semverParse } from "semver";
import { KetchError } from "./errors.ts";

// The manifest and installed-state shapes are file formats first, so their
// schemas own them; the rest of core imports them from here all the same.
export type {
  AssetSelector,
  BinSpec,
  LinkKind,
  LinkRecord,
  Manifest,
  ManifestOrigin,
  PackageKind,
} from "@ketch/schemas";
export { normalizeName, validateManifest } from "@ketch/schemas";

// ---------------------------------------------------------------------------
// Target
// ---------------------------------------------------------------------------

export type Os = "macos" | "linux" | "windows";

/** Filename tokens that indicate each OS. Order is not significant. */
export const OS_TOKENS: Record<Os, readonly string[]> = {
  macos: ["darwin", "macos", "mac", "osx", "apple", "macosx"],
  linux: ["linux", "gnu", "musl"],
  windows: ["windows", "win32", "win64", "win", "msvc"],
};

export type Arch =
  | "aarch64"
  | "x86_64"
  /** A fat binary that runs on any architecture of the host OS. */
  | "universal";

/** Filename tokens that indicate each architecture. */
export const ARCH_TOKENS: Record<Arch, readonly string[]> = {
  aarch64: ["aarch64", "arm64", "armv8", "apple-silicon", "silicon", "m1"],
  x86_64: ["x86_64", "x8664", "amd64", "x64", "intel", "64bit"],
  universal: ["universal", "universal2", "fat", "all"],
};

export interface TargetSpec {
  os: Os;
  arch: Arch;
}

/** The machine we are running on right now. */
export function hostTarget(): TargetSpec {
  const os: Os =
    process.platform === "darwin" ? "macos" : process.platform === "win32" ? "windows" : "linux";
  const arch: Arch = process.arch === "arm64" ? "aarch64" : "x86_64";
  return { os, arch };
}

/** Display form, e.g. `macos-aarch64` — also the key of `AssetSelector.target`. */
export function targetString(target: TargetSpec): string {
  return `${target.os}-${target.arch}`;
}

// ---------------------------------------------------------------------------
// Package identity
// ---------------------------------------------------------------------------

/**
 * A fully-qualified package location: which source, and an id that source
 * understands. For GitHub the id is `owner/repo`.
 *
 * Written as the `scheme:id` string everywhere it is stored, so manifests and
 * the state file read the same way a user would type it.
 */
export class PackageRef {
  constructor(
    readonly scheme: string,
    readonly id: string,
  ) {}

  static github(id: string): PackageRef {
    return new PackageRef("github", id);
  }

  /**
   * Parse `scheme:id` or a bare `owner/repo` (which implies GitHub).
   *
   * A bare word with neither `:` nor `/` is *not* a reference — it is an
   * alias to be resolved against the manifest registry, so this returns
   * `null` for it rather than guessing.
   */
  static parse(text: string): PackageRef | null {
    const parsed = parsePackageRef(text);
    return parsed === null ? null : new PackageRef(parsed.scheme, parsed.id);
  }

  /** Parse or throw — the strict form the JSON loaders use. */
  static tryFrom(text: string): PackageRef {
    const parsed = PackageRef.parse(text);
    if (parsed === null) {
      throw KetchError.msg(
        `\`${text}\` is not a package reference; expected \`scheme:id\` or \`owner/repo\``,
      );
    }
    return parsed;
  }

  /** Last path segment — the natural default package name. */
  shortName(): string {
    const slash = this.id.lastIndexOf("/");
    return slash === -1 ? this.id : this.id.slice(slash + 1);
  }

  toString(): string {
    return `${this.scheme}:${this.id}`;
  }
}

/** Which version the user asked for. */
export type VersionSpec =
  | { kind: "latest" }
  /** An exact tag or version string, matched with and without a `v` prefix. */
  | { kind: "exact"; value: string };

export function versionSpecString(spec: VersionSpec): string {
  return spec.kind === "latest" ? "latest" : spec.value;
}

/**
 * Raw user input for a package: `ripgrep`, `BurntSushi/ripgrep@14.1.0`,
 * `github:cli/cli`, `myplugin:some-id@2.0`.
 */
export class PackageSpec {
  private constructor(
    readonly raw: string,
    /** Set when the input names a source explicitly or looks like `owner/repo`. */
    readonly reference: PackageRef | null,
    /** Set when the input is a bare name to look up in the registry. */
    readonly alias: string | null,
    readonly version: VersionSpec,
  ) {}

  static parse(input: string): PackageSpec {
    const raw = input.trim();
    // Split the version off at the last `@` that follows the final `/`, so
    // scoped ids keep working and `owner/repo@v1` splits correctly.
    const slash = raw.lastIndexOf("/");
    const splitFrom = slash === -1 ? 0 : slash + 1;
    const rel = raw.slice(splitFrom).indexOf("@");
    let body: string;
    let version: VersionSpec;
    if (rel > 0) {
      const at = splitFrom + rel;
      body = raw.slice(0, at);
      version = { kind: "exact", value: raw.slice(at + 1) };
    } else {
      body = raw;
      version = { kind: "latest" };
    }
    const reference = PackageRef.parse(body);
    const alias = reference === null ? asciiLowercase(body) : null;
    return new PackageSpec(raw, reference, alias, version);
  }

  /** Best available human label before a manifest is resolved. */
  label(): string {
    if (this.alias !== null) {
      return this.alias;
    }
    if (this.reference !== null) {
      return this.reference.shortName();
    }
    return this.raw;
  }
}

// ---------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------

/**
 * A version string that orders like semver when it can, and like a human
 * reading digits when it cannot. Serialized as the raw string.
 */
export class Version {
  private constructor(
    readonly raw: string,
    readonly sem: SemVer | null,
  ) {}

  static parse(raw: string): Version {
    const trimmed = raw.trim();
    const core = trimStartChars(trimmed, "vV");
    const sem = semverParse(core) ?? relaxedSemver(core);
    return new Version(trimmed, sem);
  }

  /** True when this is a prerelease according to semver metadata. */
  isPrerelease(): boolean {
    return this.sem !== null && this.sem.prerelease.length > 0;
  }

  /** Compare ignoring a leading `v`, for matching a user-supplied tag. */
  matchesRequest(requested: string): boolean {
    const a = trimStartChars(this.raw, "vV");
    const b = trimStartChars(requested.trim(), "vV");
    return asciiLowercase(a) === asciiLowercase(b);
  }

  compare(other: Version): number {
    if (this.sem !== null && other.sem !== null) {
      // Relaxing to semver is lossy: `1.2.3.4` and `1.2.3.5` both become
      // `1.2.3`, and semver precedence ignores build metadata outright.
      // Falling back to the raw strings keeps two genuinely different
      // releases from comparing equal, which would leave a max-scan picking
      // whichever it happened to see first — sometimes the older one.
      const bySem = compareBuild(this.sem, other.sem);
      return bySem !== 0 ? bySem : naturalCmp(this.raw, other.raw);
    }
    return naturalCmp(this.raw, other.raw);
  }

  toString(): string {
    return this.raw;
  }
}

/** Strip every leading character found in `chars`, like Rust `trim_start_matches`. */
function trimStartChars(text: string, chars: string): string {
  let start = 0;
  while (start < text.length && chars.includes(text.charAt(start))) {
    start += 1;
  }
  return text.slice(start);
}

/** Strip every trailing character found in `chars`, like Rust `trim_end_matches`. */
function trimEndChars(text: string, chars: string): string {
  let end = text.length;
  while (end > 0 && chars.includes(text.charAt(end - 1))) {
    end -= 1;
  }
  return text.slice(0, end);
}

function isAsciiDigit(c: string): boolean {
  return c >= "0" && c <= "9";
}

/** Accept `1`, `1.2`, and `1.2.3.4` by padding or truncating to three parts. */
function relaxedSemver(text: string): SemVer | null {
  let headLen = 0;
  while (headLen < text.length) {
    const c = text.charAt(headLen);
    if (isAsciiDigit(c) || c === ".") {
      headLen += 1;
    } else {
      break;
    }
  }
  const head = text.slice(0, headLen);
  if (head === "") {
    return null;
  }
  const tail = text.slice(headLen);
  let parts = trimEndChars(head, ".")
    .split(".")
    .filter((p) => p !== "");
  if (parts.length === 0) {
    return null;
  }
  parts = parts.slice(0, 3);
  while (parts.length < 3) {
    parts.push("0");
  }
  const base = parts.join(".");
  const suffix = trimStartChars(tail, "-_+");
  let candidate: string;
  if (suffix === "") {
    candidate = base;
  } else {
    // Normalise separators semver rejects inside a prerelease tag.
    const cleaned = Array.from(suffix)
      .map((c) => (/[0-9A-Za-z.]/.test(c) ? c : "."))
      .join("");
    candidate = `${base}-${trimEndChars(trimStartChars(cleaned, "."), ".")}`;
  }
  return semverParse(candidate);
}

/**
 * Compare strings the way a person reads them: digit runs numerically,
 * everything else lexicographically.
 */
export function naturalCmp(a: string, b: string): number {
  // Code points, not UTF-16 units, so the comparison matches Rust `char`s.
  const as = Array.from(a);
  const bs = Array.from(b);
  let i = 0;
  let j = 0;
  for (;;) {
    const x = as[i];
    const y = bs[j];
    if (x === undefined && y === undefined) {
      return 0;
    }
    if (x === undefined) {
      return -1;
    }
    if (y === undefined) {
      return 1;
    }
    if (isAsciiDigit(x) && isAsciiDigit(y)) {
      let xs = "";
      let ys = "";
      let cx = as[i];
      while (cx !== undefined && isAsciiDigit(cx)) {
        xs += cx;
        i += 1;
        cx = as[i];
      }
      let cy = bs[j];
      while (cy !== undefined && isAsciiDigit(cy)) {
        ys += cy;
        j += 1;
        cy = bs[j];
      }
      // BigInt, because a digit run can outgrow a double without meaning to.
      const xn = BigInt(trimStartChars(xs, "0") || "0");
      const yn = BigInt(trimStartChars(ys, "0") || "0");
      if (xn !== yn) {
        return xn < yn ? -1 : 1;
      }
    } else {
      i += 1;
      j += 1;
      const xc = asciiLowercase(x).codePointAt(0) ?? 0;
      const yc = asciiLowercase(y).codePointAt(0) ?? 0;
      if (xc !== yc) {
        return xc < yc ? -1 : 1;
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

export interface Checksum {
  /** Lowercase algorithm name, currently always `sha256`. */
  algo: string;
  hex: string;
}

export function sha256Checksum(hex: string): Checksum {
  return { algo: "sha256", hex: asciiLowercase(hex) };
}

export interface ReleaseAsset {
  name: string;
  url: string;
  size: number;
  content_type: string | null;
  /** Checksum published by the source itself, when it offers one. */
  digest: Checksum | null;
  /** Extra headers a source (usually a plugin) needs for the download. */
  headers: Record<string, string>;
}

export interface Release {
  version: Version;
  tag: string;
  prerelease: boolean;
  draft: boolean;
  published_at: string | null;
  notes: string | null;
  assets: ReleaseAsset[];
}

export function releaseAsset(release: Release, name: string): ReleaseAsset | null {
  return release.assets.find((a) => a.name === name) ?? null;
}

/** Repository-level metadata, used by `info` and `search`. */
export interface SourceInfo {
  id: string;
  name: string;
  description: string | null;
  homepage: string | null;
  stars: number | null;
  license: string | null;
  archived: boolean;
}

// ---------------------------------------------------------------------------
// Manifests
// ---------------------------------------------------------------------------

export function assetSelectorIsEmpty(selector: AssetSelector): boolean {
  return (
    selector.include.length === 0 &&
    selector.exclude.length === 0 &&
    Object.keys(selector.target).length === 0
  );
}

/**
 * The manifest ketch uses when nobody wrote one: everything inferred.
 *
 * The name is sanitized rather than validated. It becomes a directory in
 * the store and a key in the state file, and nobody authored it — so a
 * reference whose last segment is unusable gets a usable name instead of
 * failing an install the user had every right to expect to work.
 */
export function inferredManifest(source: PackageRef): Manifest {
  return {
    name: sanitizeComponent(normalizeName(source.shortName())),
    source: source.toString(),
    description: null,
    homepage: null,
    kind: "auto",
    asset: { include: [], exclude: [], target: {} },
    bin: [],
    strip_prefix: null,
    prerelease: false,
    provides: [],
    notes: null,
    extra_paths: [],
  };
}

// ---------------------------------------------------------------------------
// Installed state
// ---------------------------------------------------------------------------

/**
 * One installed package, as commands work with it: the schema in
 * @ketch/schemas owns the on-disk record, this shape carries the parsed
 * `Version` and `PackageRef` so nothing downstream re-parses strings.
 */
export interface InstalledPackage {
  name: string;
  version: Version;
  source: PackageRef;
  tag: string;
  target: TargetSpec;
  asset_name: string;
  /** SHA-256 of the downloaded asset, always recorded. */
  sha256: string;
  /**
   * True when the checksum was published by the source rather than trusted
   * on first use.
   */
  checksum_verified: boolean;
  installed_at: number;
  /** Store directory holding the extracted payload. */
  prefix: string;
  links: LinkRecord[];
  pinned: boolean;
  origin: ManifestOrigin;
  /** Kept so `upgrade` reuses the same selection rules as `install`. */
  manifest: Manifest | null;
}

export function installedBinaries(pkg: InstalledPackage): LinkRecord[] {
  return pkg.links.filter((l) => l.kind === "symlink");
}

/** Seconds since the Unix epoch, saturating at 0 on a broken clock. */
export function nowUnix(): number {
  return Math.max(0, Math.floor(Date.now() / 1000));
}

/**
 * Minimal glob: `*` matches any run, `?` matches one character. Case
 * insensitive, because release asset naming is not consistent about case.
 */
export function globMatch(pattern: string, text: string): boolean {
  const p = Array.from(asciiLowercase(pattern));
  const t = Array.from(asciiLowercase(text));
  // Iterative backtracking keeps this linear in the common case and avoids
  // the exponential blowup a naive recursive matcher has on `*a*a*a*`.
  let pi = 0;
  let ti = 0;
  let star = -1;
  let mark = 0;
  while (ti < t.length) {
    const pc = pi < p.length ? p[pi] : undefined;
    if (pc !== undefined && (pc === "?" || pc === t[ti])) {
      pi += 1;
      ti += 1;
    } else if (pc === "*") {
      star = pi;
      mark = ti;
      pi += 1;
    } else if (star !== -1) {
      pi = star + 1;
      mark += 1;
      ti = mark;
    } else {
      return false;
    }
  }
  while (pi < p.length && p[pi] === "*") {
    pi += 1;
  }
  return pi === p.length;
}
