/**
 * Runtime configuration: where things live and what we are allowed to do.
 *
 * Precedence, lowest to highest: built-in defaults, `config.json` in the ketch
 * root, environment variables, command-line flags.
 *
 * Synchronous throughout, unlike the rest of core. This runs once, before
 * anything else, reading one small file — there is no concurrency to win by
 * deferring it, and `main` gets a plain value instead of a promise.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import type { ConfigFile } from "@ketch/schemas";
import {
  configFileSchema,
  REGISTRY_REPO,
  SELF_REPO,
  sanitizeComponent,
  validateRepo,
} from "@ketch/schemas";
import { KetchError } from "./errors.ts";
import type { Format, Level } from "./log.ts";
import { DEFAULT_FORMAT, DEFAULT_LEVEL, parseFormat, parseLevel } from "./log.ts";
import type { TargetSpec } from "./model.ts";
import { hostTarget } from "./model.ts";

export { REGISTRY_REPO, SELF_REPO } from "@ketch/schemas";

/** The effective configuration for one run. */
export interface Config {
  readonly root: string;
  readonly binDir: string;
  readonly storeDir: string;
  readonly cacheDir: string;
  readonly manifestDir: string;
  readonly pluginDir: string;
  readonly stateFile: string;
  readonly lockFile: string;
  readonly configFile: string;
  readonly appsDir: string;
  readonly githubToken: string | null;
  readonly prerelease: boolean;
  /** Allow installing x86_64 assets on Apple Silicon (via Rosetta). */
  readonly allowEmulation: boolean;
  /** Symlink `.app` bundles instead of copying them. */
  readonly linkApps: boolean;
  /** Refuse to install when the release publishes no checksum. */
  readonly requireChecksums: boolean;
  /** Remove the quarantine flag from code that passes signature checks. */
  readonly stripQuarantine: boolean;
  readonly selfRepo: string;
  readonly registry: string;
  readonly registryDir: string;
  readonly target: TargetSpec;
  /** Packages installed at once by a batch install. Never zero. */
  readonly jobs: number;
  readonly logFile: string;
  readonly logLevel: Level;
  readonly logFormat: Format;
}

/** How `loadConfig` is told about this run. */
export interface LoadOptions {
  /** The `--root` flag. */
  root?: string | undefined;
  /**
   * Where a config problem that is not fatal gets reported.
   *
   * Core cannot reach the terminal — the UI sits above it — so the caller
   * supplies the sink, the same way long operations receive a `ProgressSink`.
   * Defaults to discarding, which is what tests want.
   */
  warn?: ((message: string) => void) | undefined;
}

/** Build the effective config. */
export function loadConfig(options: LoadOptions = {}): Config {
  const warn = options.warn ?? (() => {});

  const rootRequest = options.root ?? env("KETCH_ROOT");
  const root = rootRequest === undefined ? defaultRoot() : absolutePath(expandTilde(rootRequest));

  const configFile = path.join(root, "config.json");
  const file = readConfigFile(configFile);

  // Environment over file, as every other setting here resolves: the file is
  // the standing preference, the variable is this run's override.
  const appsRequest = env("KETCH_APPS_DIR") ?? file.apps_dir ?? undefined;
  const appsDir = appsRequest === undefined ? "/Applications" : expandTilde(appsRequest);

  // A relative apps dir would resolve against whatever directory the user
  // happened to run ketch from, and install somewhere different every time.
  if (!path.isAbsolute(appsDir)) {
    throw config(`apps_dir must be an absolute path, not \`${appsDir}\``);
  }

  // The file lives inside the root, so it cannot choose it. Saying so is
  // better than honouring the key nowhere and explaining it nowhere.
  if (file.root !== undefined && file.root !== null) {
    warn(`\`root\` in ${configFile} has no effect; set KETCH_ROOT or pass --root`);
  }

  const selfRepo = repo("self_repo", nonBlankEnv("KETCH_SELF_REPO") ?? file.self_repo ?? SELF_REPO);
  const registry = repo(
    "registry",
    nonBlankEnv("KETCH_REGISTRY") ?? file.registry ?? REGISTRY_REPO,
  );

  // The first variable that is *set* wins even when it is empty, so an
  // explicitly blanked `KETCH_GITHUB_TOKEN` suppresses an inherited
  // `GITHUB_TOKEN` rather than falling through to it. The blank check comes
  // after the choice, not during it.
  const tokenRequest =
    env("KETCH_GITHUB_TOKEN") ?? env("GITHUB_TOKEN") ?? env("GH_TOKEN") ?? file.github_token;
  const githubToken =
    tokenRequest !== undefined && tokenRequest !== null && tokenRequest.trim() !== ""
      ? tokenRequest
      : null;

  // A parse failure here is the user's own config or environment, so it is an
  // error rather than a silent fall back to the default.
  const logLevel = parsed("KETCH_LOG_LEVEL", "log_level", file.log_level, parseLevel);
  const logFormat = parsed("KETCH_LOG_FORMAT", "log_format", file.log_format, parseFormat);

  const jobsEnv = nonBlankEnv("KETCH_JOBS");
  let jobs: number | null | undefined = file.jobs;
  if (jobsEnv !== undefined) {
    const trimmed = jobsEnv.trim();
    if (!/^\d+$/.test(trimmed)) {
      throw config(`KETCH_JOBS must be a whole number, not \`${jobsEnv}\``);
    }
    jobs = Number(trimmed);
  }

  return {
    root,
    binDir: path.join(root, "bin"),
    storeDir: path.join(root, "store"),
    cacheDir: path.join(root, "cache"),
    manifestDir: path.join(root, "manifests"),
    pluginDir: path.join(root, "plugins"),
    stateFile: path.join(root, "state.json"),
    lockFile: path.join(root, ".lock"),
    configFile,
    appsDir,
    githubToken,
    prerelease: envBool("KETCH_PRERELEASE") ?? file.prerelease ?? false,
    allowEmulation: envBool("KETCH_ALLOW_EMULATION") ?? file.allow_emulation ?? true,
    linkApps: envBool("KETCH_LINK_APPS") ?? file.link_apps ?? false,
    requireChecksums: envBool("KETCH_REQUIRE_CHECKSUMS") ?? file.require_checksums ?? false,
    stripQuarantine: envBool("KETCH_STRIP_QUARANTINE") ?? file.strip_quarantine ?? true,
    selfRepo,
    registry,
    // Deliberately not in `ensureDirs`: the directory existing is how ketch
    // knows the registry has been fetched.
    registryDir: path.join(root, "registry"),
    target: hostTarget(),
    // Downloads dominate an install and spend their time waiting, so the
    // useful number is well above the core count. Capped anyway: a hundred
    // parallel requests is how a source starts refusing them.
    jobs: Math.min(jobs !== undefined && jobs !== null && jobs > 0 ? jobs : 4, 16),
    logFile: path.join(root, "logs", "ketch.log"),
    logLevel: logLevel ?? DEFAULT_LEVEL,
    logFormat: logFormat ?? DEFAULT_FORMAT,
  };
}

/** Create the directory layout. Safe to call repeatedly. */
export function ensureDirs(cfg: Config): void {
  for (const dir of [
    cfg.root,
    cfg.binDir,
    cfg.storeDir,
    cfg.cacheDir,
    cfg.manifestDir,
    cfg.pluginDir,
  ]) {
    try {
      fs.mkdirSync(dir, { recursive: true });
    } catch (cause) {
      throw KetchError.io(dir, asError(cause));
    }
  }
}

/** Where a specific version of a package is unpacked. */
export function packageDir(cfg: Config, name: string, version: string): string {
  return path.join(cfg.storeDir, name, sanitizeComponent(version));
}

/** True when the bin dir is on the caller's PATH. */
export function binDirOnPath(cfg: Config): boolean {
  const raw = env("PATH");
  if (raw === undefined) {
    return false;
  }
  return raw.split(path.delimiter).some((entry) => entry === cfg.binDir);
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

function readConfigFile(file: string): ConfigFile {
  let text: string;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch (cause) {
    if (errno(cause) === "ENOENT" || errno(cause) === "EISDIR") {
      return {};
    }
    throw KetchError.io(file, asError(cause));
  }
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch (cause) {
    throw KetchError.parse(file, asError(cause).message);
  }
  const parsedFile = configFileSchema.safeParse(data);
  if (!parsedFile.success) {
    throw KetchError.parse(file, zodMessage(parsedFile.error));
  }
  return parsedFile.data;
}

/**
 * A setting that has to be parsed, from the environment or the config file.
 *
 * A typo is reported against whichever one supplied it, because "unknown log
 * level `verbose`" is only actionable if you know which file to fix.
 */
function parsed<T>(
  envKey: string,
  fileKey: string,
  fromFile: string | null | undefined,
  parse: (text: string) => T,
): T | undefined {
  const fromEnv = nonBlankEnv(envKey);
  const value = fromEnv ?? fromFile;
  if (value === undefined || value === null) {
    return undefined;
  }
  const whereFrom = fromEnv !== undefined ? envKey : `\`${fileKey}\` in config.json`;
  try {
    return parse(value);
  } catch (cause) {
    throw config(`${whereFrom}: ${asError(cause).message}`);
  }
}

function env(key: string): string | undefined {
  return process.env[key];
}

function nonBlankEnv(key: string): string | undefined {
  const value = env(key);
  return value !== undefined && value.trim() !== "" ? value : undefined;
}

function envBool(key: string): boolean | undefined {
  switch (env(key)?.trim().toLowerCase()) {
    case "1":
    case "true":
    case "yes":
    case "on":
      return true;
    case "0":
    case "false":
    case "no":
    case "off":
      return false;
    default:
      return undefined;
  }
}

function defaultRoot(): string {
  return path.join(homeDir() ?? ".", ".ketch");
}

function homeDir(): string | undefined {
  const home = os.homedir();
  return home === "" ? undefined : home;
}

/**
 * Resolve against the current directory. Unlike the Rust original this also
 * normalizes, because the result is the prefix of every other path ketch
 * builds and a `..` left inside it shows up in every message.
 */
function absolutePath(p: string): string {
  return path.isAbsolute(p) ? path.normalize(p) : path.resolve(process.cwd(), p);
}

function expandTilde(p: string): string {
  if (p === "~" || p.startsWith("~/")) {
    const home = homeDir();
    if (home !== undefined) {
      return p === "~" ? home : path.join(home, p.slice(2));
    }
  }
  return p;
}

/** `validateRepo` rejects with a bare `Error`; a bad repo is a config fault. */
function repo(what: string, raw: string): string {
  try {
    return validateRepo(what, raw);
  } catch (cause) {
    throw config(asError(cause).message);
  }
}

function config(text: string): KetchError {
  return new KetchError({ kind: "config", text });
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
