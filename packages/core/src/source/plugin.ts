/**
 * External source plugins.
 *
 * A plugin is any executable named `ketch-source-<scheme>` found in the
 * plugins directory or on PATH. ketch invokes it with a subcommand and reads
 * one JSON document from stdout. That is the entire contract — plugins can be
 * written in any language and need no ketch release to ship.
 *
 * The protocol is specified in `docs/PLUGINS.md`; `PROTOCOL_VERSION` (from
 * `@ketch/schemas`) is what this build speaks.
 *
 * ```text
 * capabilities              -> {"protocol":1,"scheme":"gitlab","download":false,"search":true}
 * describe <id>             -> a SourceInfo object, or null
 * releases <id> [--prerelease] [--limit N]
 *                           -> [ {"tag":"v1.2.3","version":"1.2.3","assets":[...]}, ... ]
 * search <query> --limit N  -> [ SourceInfo, ... ]
 * download <url> <dest>     -> only when capabilities.download is true
 * ```
 *
 * Process handling goes through `node:child_process`'s `execFile` rather than
 * a hand-rolled spawn-and-drain loop: its `timeout`/`killSignal` options are
 * the deadline the Rust source enforces with a polling wait loop, and its
 * `maxBuffer` is the output cap the Rust source enforces by bounding how much
 * a reader thread will drain from each pipe. Both collapse to stdlib options
 * here, so neither `wait_with_deadline` nor `capped` has a literal port —
 * `runPlugin`'s own tests exercise the same claims against the real
 * subprocess runner instead of an in-memory stand-in.
 */

import type { ExecFileException } from "node:child_process";
import { execFile } from "node:child_process";
import fsp from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import type {
  PluginCapabilities,
  PluginRelease,
  PluginReleaseAsset,
  SourceInfo as PluginSourceInfo,
} from "@ketch/schemas";
import {
  PLUGIN_PREFIX,
  PROTOCOL_VERSION,
  pluginCapabilitiesSchema,
  pluginDescribeSchema,
  pluginReleasesSchema,
  pluginSearchSchema,
  usableScheme,
} from "@ketch/schemas";
import type { Config } from "../config.ts";
import { KetchError } from "../errors.ts";
import { Http, sha256File } from "../http.ts";
import type { Release, ReleaseAsset, SourceInfo } from "../model.ts";
import { Version } from "../model.ts";
import type { Platform } from "../platform/platform.ts";
import { hostPlatform } from "../platform/platform.ts";
import type { ProgressSink } from "../progress.ts";
import type { ListOpts, Source } from "./source.ts";

/** How long a plugin has to answer one subcommand. */
const PLUGIN_TIMEOUT_MS = 30_000;

/** How much it may write to one pipe while doing so. */
const PLUGIN_MAX_OUTPUT = 8 * 1024 * 1024;

/** `runPlugin`'s deadline and output cap, overridable so tests do not wait out the real ones. */
export interface PluginRunOptions {
  timeoutMs?: number;
  maxOutputBytes?: number;
}

/**
 * Run one plugin subcommand and return its stdout.
 *
 * A plugin is a third-party executable, so this is a trust boundary and not
 * just a convenience wrapper. Three things are enforced: no stdin, so a
 * plugin cannot sit waiting on a terminal nobody is typing at; a bound on how
 * much it may write, so it cannot exhaust memory; and a deadline, after which
 * it is killed. Without them a single misbehaving plugin hangs every ketch
 * command, because discovery probes all of them before anything else runs.
 */
export function runPlugin(
  execPath: string,
  args: readonly string[],
  opts: PluginRunOptions = {},
): Promise<string> {
  const timeoutMs = opts.timeoutMs ?? PLUGIN_TIMEOUT_MS;
  const maxOutputBytes = opts.maxOutputBytes ?? PLUGIN_MAX_OUTPUT;

  return new Promise<string>((resolve, reject) => {
    const child = execFile(
      execPath,
      Array.from(args),
      {
        env: { ...process.env, KETCH_PROTOCOL_VERSION: String(PROTOCOL_VERSION) },
        timeout: timeoutMs,
        killSignal: "SIGKILL",
        maxBuffer: maxOutputBytes,
        encoding: "buffer",
        windowsHide: true,
      },
      (error, stdout, stderr) => {
        if (error !== null) {
          reject(translateExecError(execPath, args, error, stderr, timeoutMs, maxOutputBytes));
          return;
        }
        try {
          resolve(decodeUtf8Strict(execPath, stdout));
        } catch (cause) {
          reject(cause);
        }
      },
    );
    // Closed rather than inherited: a plugin that tries to prompt gets EOF
    // rather than the user's terminal. `execFile`'s own `stdio` option is not
    // honoured for this (verified empirically — stdin stays open regardless
    // of what is passed there), so the pipe is closed by hand instead.
    child.stdin?.end();
  });
}

function translateExecError(
  execPath: string,
  args: readonly string[],
  error: ExecFileException,
  stderr: Buffer,
  timeoutMs: number,
  maxOutputBytes: number,
): KetchError {
  const name = fileName(execPath);
  if (error.code === "ERR_CHILD_PROCESS_STDIO_MAXBUFFER") {
    const stream = error.message.includes("stderr") ? "stderr" : "stdout";
    return new KetchError({
      kind: "plugin",
      name,
      detail: `wrote more than ${maxOutputBytes} bytes to ${stream}`,
    });
  }
  if (error.killed === true) {
    return new KetchError({
      kind: "plugin",
      name,
      detail: `did not answer within ${Math.round(timeoutMs / 1000)}s and was stopped`,
    });
  }
  if (typeof error.code !== "number") {
    // Spawn-level failure (ENOENT, EACCES, ...): the process never ran.
    return new KetchError({
      kind: "plugin",
      name,
      detail: `could not run ${execPath}: ${error.message}`,
    });
  }
  return new KetchError({
    kind: "command",
    cmd: `${name} ${args.join(" ")}`,
    status: `exit status: ${error.code}`,
    stderr: stderr.toString("utf8"),
  });
}

function decodeUtf8Strict(execPath: string, buf: Buffer): string {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(buf);
  } catch (cause) {
    throw new KetchError({
      kind: "plugin",
      name: fileName(execPath),
      detail: `wrote output that is not UTF-8: ${asError(cause).message}`,
    });
  }
}

function fileName(execPath: string): string {
  const base = path.basename(execPath);
  return base === "" ? "plugin" : base;
}

// ---------------------------------------------------------------------------
// JSON parsing
// ---------------------------------------------------------------------------

/**
 * The slice of a Zod schema `parseJson` needs. Structural rather than a
 * direct `zod` import: @ketch/core does not depend on `zod` itself, only on
 * `@ketch/schemas`, which already exports fully-inferred types alongside its
 * schema values.
 */
interface Parseable<T> {
  safeParse(input: unknown):
    | { success: true; data: T }
    | {
        success: false;
        error: { issues: ReadonlyArray<{ path: PropertyKey[]; message: string }> };
      };
}

function parseJson<T>(execPath: string, schema: Parseable<T>, body: string): T {
  let json: unknown;
  try {
    json = JSON.parse(body);
  } catch (cause) {
    throw new KetchError({
      kind: "plugin",
      name: fileName(execPath),
      detail: `returned JSON ketch cannot read: ${asError(cause).message}`,
    });
  }
  const result = schema.safeParse(json);
  if (!result.success) {
    throw new KetchError({
      kind: "plugin",
      name: fileName(execPath),
      detail: `returned JSON ketch cannot read: ${zodMessage(result.error)}`,
    });
  }
  return result.data;
}

function zodMessage(error: {
  issues: ReadonlyArray<{ path: PropertyKey[]; message: string }>;
}): string {
  return error.issues
    .map((issue) => (issue.path.length > 0 ? `${issue.path.join(".")}: ` : "") + issue.message)
    .join("; ");
}

// ---------------------------------------------------------------------------
// Wire format -> domain model
// ---------------------------------------------------------------------------

function toReleaseAsset(a: PluginReleaseAsset): ReleaseAsset {
  return {
    name: a.name,
    url: a.url,
    size: a.size,
    content_type: a.content_type ?? null,
    digest: a.digest ?? null,
    headers: a.headers,
  };
}

function toRelease(p: PluginRelease): Release {
  return {
    version: Version.parse(p.version),
    tag: p.tag,
    prerelease: p.prerelease,
    draft: p.draft,
    published_at: p.published_at ?? null,
    notes: p.notes ?? null,
    assets: p.assets.map(toReleaseAsset),
  };
}

function toSourceInfo(s: PluginSourceInfo): SourceInfo {
  return {
    id: s.id,
    name: s.name,
    description: s.description ?? null,
    homepage: s.homepage ?? null,
    stars: s.stars ?? null,
    license: s.license ?? null,
    archived: s.archived,
  };
}

// ---------------------------------------------------------------------------
// PluginSource
// ---------------------------------------------------------------------------

/** A discovered plugin executable. */
export class PluginSource implements Source {
  readonly scheme: string;
  /** File name of the executable, for diagnostics. */
  readonly name: string;
  readonly path: string;
  private readonly downloads: boolean;
  private readonly searches: boolean;

  private constructor(execPath: string, caps: PluginCapabilities) {
    this.path = execPath;
    this.name = fileName(execPath);
    this.scheme = caps.scheme;
    this.downloads = caps.download;
    this.searches = caps.search;
  }

  /** Interrogate an executable and adopt it if it speaks a version we know. */
  static async probe(execPath: string): Promise<PluginSource> {
    const raw = await runPlugin(execPath, ["capabilities"]);
    const caps = parseJson(execPath, pluginCapabilitiesSchema, raw);
    if (caps.protocol !== PROTOCOL_VERSION) {
      throw new KetchError({
        kind: "plugin",
        name: fileName(execPath),
        detail: `speaks protocol ${caps.protocol} but this ketch speaks ${PROTOCOL_VERSION}`,
      });
    }
    // The scheme ends up in user input and in recorded state, so it has to
    // be something that can be typed and round-tripped unambiguously.
    if (!usableScheme(caps.scheme)) {
      throw new KetchError({
        kind: "plugin",
        name: fileName(execPath),
        detail: `reports an unusable scheme \`${caps.scheme}\``,
      });
    }
    return new PluginSource(execPath, caps);
  }

  private run<T>(schema: Parseable<T>, args: readonly string[]): Promise<T> {
    return runPlugin(this.path, args).then((raw) => parseJson(this.path, schema, raw));
  }

  async listReleases(id: string, opts: ListOpts): Promise<Release[]> {
    const args = ["releases", id, "--limit", String(opts.limit)];
    if (opts.includePrerelease) {
      args.push("--prerelease");
    }
    const releases = await this.run(pluginReleasesSchema, args);
    // The trait promises drafts are gone and prereleases are filtered; a
    // plugin that ignores its flags must not change what ketch installs.
    return releases
      .filter((r) => !r.draft && (opts.includePrerelease || !r.prerelease))
      .map(toRelease);
  }

  async describe(id: string): Promise<SourceInfo | null> {
    const found = await this.run(pluginDescribeSchema, ["describe", id]);
    return found === null ? null : toSourceInfo(found);
  }

  async search(query: string, limit: number): Promise<SourceInfo[]> {
    if (!this.searches) {
      return [];
    }
    const found = await this.run(pluginSearchSchema, ["search", query, "--limit", String(limit)]);
    return found.map(toSourceInfo);
  }

  async download(asset: ReleaseAsset, dest: string, progress: ProgressSink): Promise<string> {
    if (!this.downloads) {
      // No token is ever handed to a plugin's URLs: whatever credentials an
      // asset needs must come from the plugin's own headers. `Http.anonymous`
      // carries none, and its `download` already streams to a staged path
      // and renames, so an interrupted transfer never looks complete.
      return Http.anonymous().download(asset.url, dest, asset.headers, false, progress);
    }
    await runPlugin(this.path, ["download", asset.url, dest]);
    if (!(await pathExists(dest))) {
      throw new KetchError({
        kind: "plugin",
        name: this.name,
        detail: `reported success but wrote no file to ${dest}`,
      });
    }
    // Hash what actually landed on disk. A plugin does not get to assert the
    // checksum of its own download.
    return sha256File(dest);
  }
}

async function pathExists(file: string): Promise<boolean> {
  try {
    await fsp.access(file);
    return true;
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/** One discovery candidate: either a plugin that probed clean, or why it did not. */
export type PluginProbeResult =
  | { readonly ok: true; readonly plugin: PluginSource }
  | { readonly ok: false; readonly path: string; readonly error: KetchError };

/**
 * Find every plugin available to this run.
 *
 * Returns one entry per candidate so a single broken plugin can be reported
 * without hiding the ones that work.
 */
export async function discoverPlugins(cfg: Config): Promise<PluginProbeResult[]> {
  let platform: Platform;
  try {
    platform = await hostPlatform();
  } catch {
    return [];
  }

  const pathDirs = (process.env["PATH"] ?? "").split(path.delimiter).filter((d) => d !== "");
  const dirs = [cfg.pluginDir, ...pathDirs];

  const found: PluginProbeResult[] = [];
  const seen = new Set<string>();
  for (const dir of dirs) {
    let entries: string[];
    try {
      entries = await fsp.readdir(dir);
    } catch {
      continue;
    }
    // Directory order is arbitrary; a stable list keeps `plugin list` and any
    // shadowing warning reproducible.
    const names = entries
      .filter((name) => name.startsWith(PLUGIN_PREFIX) && name.length > PLUGIN_PREFIX.length)
      .toSorted();

    for (const name of names) {
      const candidate = path.join(dir, name);
      // The plugins dir comes first, so it wins over a copy on PATH.
      if (seen.has(name) || !(await platform.isExecutable(candidate))) {
        continue;
      }
      seen.add(name);
      try {
        found.push({ ok: true, plugin: await PluginSource.probe(candidate) });
      } catch (cause) {
        found.push({ ok: false, path: candidate, error: toKetchError(cause) });
      }
    }
  }
  return found;
}

function toKetchError(cause: unknown): KetchError {
  return cause instanceof KetchError
    ? cause
    : new KetchError({ kind: "plugin", name: "plugin", detail: asError(cause).message });
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}
