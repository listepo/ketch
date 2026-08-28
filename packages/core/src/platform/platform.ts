/**
 * Per-operating-system behaviour.
 *
 * Everything that differs between macOS, Linux and Windows lives behind this
 * interface: which release asset is even installable, how a payload becomes
 * something on PATH, and what "is this code trustworthy" means locally.
 *
 * Only macOS is implemented today. Adding Linux means adding a file here and
 * one line in `hostPlatform()` — no changes anywhere else in the codebase.
 */

import process from "node:process";
import { asciiLowercase } from "@ketch/schemas";
import type { Config } from "../config.ts";
import { KetchError } from "../errors.ts";
import type { Extractor } from "../extract/extractor.ts";
import type { Arch, BinSpec, LinkRecord, PackageKind, TargetSpec } from "../model.ts";

/** Why an asset was chosen, and at what cost. */
export interface AssetScore {
  /** Higher wins. Only compared between assets of the same release. */
  score: number;
  /** Architecture this asset actually provides. */
  arch: Arch;
  /** True when it runs only under emulation (x86_64 on Apple Silicon). */
  emulated: boolean;
  /** Short explanation, shown with `--verbose` and in `ketch info`. */
  reason: string;
}

/** Everything the platform needs to place an extracted payload. */
export interface Placement {
  name: string;
  version: string;
  /** Directory holding the extracted release payload. */
  payloadDir: string;
  /** Final home of this version inside the store. */
  storeDir: string;
  binDir: string;
  appsDir: string;
  kind: PackageKind;
  /** Explicit binaries from the manifest. Empty means "discover them". */
  binSpecs: readonly BinSpec[];
  /**
   * Links recorded for the version being replaced, which still exist:
   * placement runs before the old version is retired. A destination listed
   * here is ketch's own to overwrite. Anything else occupying a destination
   * belongs to another package or to the user.
   */
  replacing: readonly LinkRecord[];
  /** Symlink `.app` bundles rather than copying them. */
  linkApps: boolean;
  /**
   * Create user-visible links. False still moves the payload into the
   * store, so `ketch relink` can expose it later without re-downloading.
   */
  link: boolean;
}

/** Result of a local trust check on downloaded code. */
export type TrustVerdict =
  /** Validly signed and accepted by the system policy. */
  | { kind: "trusted"; authority: string }
  /** Signed, but the system would still warn (ad-hoc, or unnotarized). */
  | { kind: "weak"; detail: string }
  /** No usable signature. */
  | { kind: "untrusted"; detail: string }
  /** This platform does not do signature checks. */
  | { kind: "not_applicable" };

/**
 * Whether it is safe to remove the quarantine flag without silently disabling
 * a protection the user is relying on.
 */
export function mayStripQuarantine(verdict: TrustVerdict): boolean {
  return verdict.kind === "trusted";
}

export type CheckStatus = "ok" | "warn" | "fail";

/** One line of `ketch doctor` output. */
export interface DoctorCheck {
  name: string;
  status: CheckStatus;
  detail: string;
  fix: string | null;
}

export function doctorOk(name: string, detail: string): DoctorCheck {
  return { name, status: "ok", detail, fix: null };
}

export function doctorWarn(name: string, detail: string, fix: string): DoctorCheck {
  return { name, status: "warn", detail, fix };
}

export function doctorFail(name: string, detail: string, fix: string): DoctorCheck {
  return { name, status: "fail", detail, fix };
}

/** Present so `ketch doctor` can colour a summary without re-deriving it. */
export function worstStatus(checks: readonly DoctorCheck[]): CheckStatus {
  if (checks.some((c) => c.status === "fail")) {
    return "fail";
  }
  if (checks.some((c) => c.status === "warn")) {
    return "warn";
  }
  return "ok";
}

/** The host operating system's rules. */
export interface Platform {
  /** Stable identifier, e.g. `macos`. */
  readonly id: string;

  target(): TargetSpec;

  /**
   * Rate an asset by file name alone.
   *
   * `null` means "cannot run here" and the asset is discarded. This is the
   * single most important function for install quality: it is what stops
   * ketch grabbing a Linux tarball or a `.sha256` sidecar.
   */
  scoreAsset(assetName: string, allowEmulation: boolean): AssetScore | null;

  /** Extractors this platform can use, most specific first. */
  extractors(): Extractor[];

  /** Move the payload into the store and create user-visible links. */
  place(plan: Placement): Promise<LinkRecord[]>;

  /** Undo `place`. Must tolerate links that are already gone. */
  unplace(links: readonly LinkRecord[]): Promise<void>;

  /** Inspect downloaded code before it is exposed to the user. */
  verifyTrust(path: string): Promise<TrustVerdict>;

  /**
   * Clear the OS "downloaded from the internet" mark. Only called when the
   * trust verdict allows it.
   */
  clearQuarantine(path: string): Promise<void>;

  /** Is this file something we can execute and link onto PATH? */
  isExecutable(path: string): Promise<boolean>;

  /** Files this platform treats as app bundles rather than executables. */
  appBundleExtension(): string | null;

  /** Environment checks for `ketch doctor`, against the resolved config. */
  doctor(cfg: Config): Promise<DoctorCheck[]>;
}

/**
 * The platform for the machine we are on.
 *
 * Unsupported hosts fail here with one clear message rather than misbehaving
 * deeper in the install pipeline. The macOS backend is imported lazily so the
 * error path never loads platform code that cannot run.
 */
export async function hostPlatform(): Promise<Platform> {
  if (process.platform !== "darwin") {
    throw KetchError.msg(
      "ketch supports macOS only. Linux and Windows backends are planned; " +
        "see ROADMAP.md — implementing `Platform` in packages/core/src/platform/ is all that is required.",
    );
  }
  const darwin = await import("./darwin.ts");
  return darwin.createDarwinPlatform();
}

/**
 * Tokens in an asset name that mean "this is not a program".
 *
 * Shared by every platform, because signature and checksum sidecars look the
 * same everywhere.
 */
export const SIDECAR_SUFFIXES: readonly string[] = [
  ".sha256",
  ".sha512",
  ".sha1",
  ".md5",
  ".asc",
  ".sig",
  ".sigstore",
  ".pem",
  ".crt",
  ".sbom",
  ".sbom.json",
  ".spdx.json",
  ".intoto.jsonl",
  ".pubkey",
  ".minisig",
  ".cert",
];

/** Substrings that mark a file as source code or metadata, not a build. */
export const NON_BINARY_TOKENS: readonly string[] = [
  "checksum",
  "checksums",
  "sha256sums",
  "sha512sums",
  "source-code",
  "sources",
  "src.tar",
  "-src-",
  "vendor",
  "manifest",
  "provenance",
  "attestation",
  "changelog",
  "release-notes",
];

/** Extensions that never contain a runnable macOS/Linux payload. */
export const REJECTED_EXTENSIONS: readonly string[] = [
  ".txt",
  ".md",
  ".json",
  ".yaml",
  ".yml",
  ".xml",
  ".csv",
  ".log",
  ".deb",
  ".rpm",
  ".apk",
  ".msi",
  ".exe",
  ".appimage",
  ".snap",
  ".flatpak",
  ".nupkg",
  ".jar",
  ".war",
  ".whl",
  ".gem",
];

/** True when `name` ends with any known sidecar suffix. */
export function isSidecar(name: string): boolean {
  const lower = asciiLowercase(name);
  return SIDECAR_SUFFIXES.some((s) => lower.endsWith(s));
}
