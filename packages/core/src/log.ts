/**
 * The log file: how much gets written, in what shape, and the writer itself.
 *
 * Every status line ketch writes also goes here, minus the colour and plus a
 * timestamp — including the lines `--quiet` swallowed and the debug lines
 * `--verbose` would have shown. A terminal scrolls away and a failed install
 * is usually reported hours later; the log is what is left to read.
 *
 * Built on pino rather than a hand-rolled writer, per the TS rewrite's own
 * rules — but wired so the *shape* on disk still matches what `src/log.rs`
 * writes: JSON Lines with exactly `time`/`level`/`pid`/`msg`, keyed the same
 * way, so a Rust-written and a TS-written log line grep the same. `pino()` is
 * given a sync `pino.destination` and, for text, an in-process pino-pretty
 * stream built directly rather than through `pino.transport()` — that spawns
 * a worker thread, which Bun, Deno and a Perry-compiled binary must not need
 * just to open a log file.
 *
 * Unlike `src/log.rs`, `init` cannot fail loudly by printing to stderr
 * itself — core has no terminal, callers do — so it returns the warning
 * instead of emitting it. Nothing here is allowed to throw: a package
 * manager that refuses to install because it could not open its log is
 * broken, so an unwritable log is reported once, softly, and then ignored
 * for the rest of the run.
 */

import nodeFs from "node:fs";
import nodePath from "node:path";
import process from "node:process";
import { asciiLowercase } from "@ketch/schemas";
import pino from "pino";
import pinoPretty from "pino-pretty";
import type { Config } from "./config.ts";

/**
 * How much gets written, in order.
 *
 * A record is written when its level is at or below the configured one, which
 * puts `off` first and makes it filter everything.
 */
export const LEVEL_ORDER = ["off", "error", "warn", "info", "debug"] as const;

/** How much gets written. */
export type Level = (typeof LEVEL_ORDER)[number];

/** What ketch logs at when nothing says otherwise. */
export const DEFAULT_LEVEL: Level = "info";

/**
 * Whether a record at `level` reaches a log configured at `configured`.
 * `off` is below every real level, so it filters all of them.
 */
export function levelPasses(level: Level, configured: Level): boolean {
  return LEVEL_ORDER.indexOf(level) <= LEVEL_ORDER.indexOf(configured) && configured !== "off";
}

/** The name a record carries in the text log. */
export function levelLabel(level: Level): string {
  return level.toUpperCase();
}

/**
 * Parse a configured log level. Throws with the list of accepted values,
 * because the caller knows which file or variable supplied it and we do not.
 */
export function parseLevel(text: string): Level {
  switch (asciiLowercase(text.trim())) {
    case "off":
    case "none":
    case "false":
      return "off";
    case "error":
      return "error";
    case "warn":
    case "warning":
      return "warn";
    case "info":
      return "info";
    case "debug":
    case "trace":
      return "debug";
    default:
      throw new Error(`unknown log level \`${text.trim()}\`; use off, error, warn, info or debug`);
  }
}

/** How a record is laid out on the line. */
export type Format = "text" | "json";

/** What ketch logs as when nothing says otherwise. */
export const DEFAULT_FORMAT: Format = "text";

/** Parse a configured log format. Throws with the accepted values. */
export function parseFormat(text: string): Format {
  switch (asciiLowercase(text.trim())) {
    case "text":
    case "plain":
    case "logfmt":
      return "text";
    case "json":
    case "jsonl":
    case "ndjson":
      return "json";
    default:
      throw new Error(`unknown log format \`${text.trim()}\`; use text or json`);
  }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/** Rotate at this size. One old file is kept, so the logs cost at most twice
 * this much and never need pruning by hand. */
const MAX_BYTES = 5 * 1024 * 1024;

interface Sink {
  readonly logger: pino.Logger;
  /** Kept so tests (and only tests) can close the fd between runs. */
  readonly dest: ReturnType<typeof pino.destination>;
  readonly path: string;
  readonly level: Level;
  readonly format: Format;
}

let sink: Sink | null = null;

/**
 * Open the log for this run. Called once, as soon as the config exists.
 *
 * The failure is deliberately soft: an unwritable log is worth a warning,
 * not a reason for `ketch install` to stop working. Unlike `src/log.rs`,
 * which can `eprintln!` straight to stderr, core has no terminal of its own —
 * so the warning is returned for the caller to show, instead of printed here.
 *
 * `version` is `env!("CARGO_PKG_VERSION")` in Rust, baked in at compile time;
 * there is no TS equivalent, so the caller (the one place that knows which
 * package.json is the running application's own) passes it in.
 */
export function init(cfg: Config, version: string): string | null {
  if (cfg.logLevel === "off") {
    return null;
  }
  try {
    sink = open(cfg.logFile, cfg.logLevel, cfg.logFormat);
  } catch (cause) {
    return `could not open ${cfg.logFile}: ${asError(cause).message}`;
  }
  const args = process.argv.slice(2).join(" ");
  record("info", `ketch ${version} · ${args}`);
  return null;
}

/** Where this run is logging, once `init` has succeeded. */
export function path(): string | null {
  return sink === null ? null : sink.path;
}

/** Write one record. Never throws, never blocks on anything but the file. */
export function record(level: Level, message: string): void {
  const current = sink;
  if (current === null || level === "off" || !levelPasses(level, current.level)) {
    return;
  }
  // Text keeps a log file greppable and single-line by construction (this is
  // the one place ketch shows arbitrary bytes it did not write itself — a
  // message could carry a newline or a terminal escape). JSON does not need
  // it: pino's own string encoding already keeps a control character on the
  // line it came from, the same way `serde_json` does on the Rust side.
  const safe = current.format === "text" ? escape(message) : message;
  try {
    switch (level) {
      case "error":
        current.logger.error(safe);
        break;
      case "warn":
        current.logger.warn(safe);
        break;
      case "info":
        current.logger.info(safe);
        break;
      case "debug":
        current.logger.debug(safe);
        break;
    }
  } catch {
    // A log that cannot be written is not a reason to fail the command it
    // was recording, and reporting it here would recurse straight back in.
  }
}

/** Test-only: close the sink and forget it, so the next `init` starts clean. */
export function resetForTests(): void {
  if (sink !== null) {
    try {
      sink.dest.end();
    } catch {
      // best-effort
    }
    sink = null;
  }
}

function open(file: string, level: Level, format: Format): Sink {
  // Left to propagate as-is: `init` already wraps this whole call and
  // formats `${cfg.logFile}: ${cause.message}` once, the same shape Rust
  // gets from `{e}` on the propagated `io::Error`.
  nodeFs.mkdirSync(nodePath.dirname(file), { recursive: true });
  rotate(file);

  const dest = pino.destination({ dest: file, sync: true, append: true });
  const pid = process.pid;
  // `timestamp` and `formatters.level` are what make the JSON line match
  // `src/log.rs`'s exactly: our own RFC 3339-UTC-seconds string under `time`,
  // and the level as the lowercase word pino already computes internally
  // rather than its default numeric code. `base` replaces pino's default
  // `{pid, hostname}` outright — the Rust line never carries a hostname.
  const options: pino.LoggerOptions = {
    level: "debug",
    base: { pid },
    timestamp: () => `,"time":"${timestamp(now())}"`,
    formatters: { level: (label) => ({ level: label }) },
  };
  const logger =
    format === "text"
      ? pino(
          options,
          pinoPretty({
            destination: dest,
            sync: true,
            colorize: false,
            translateTime: false,
            singleLine: true,
          }),
        )
      : pino(options, dest);

  return { logger, dest, path: file, level, format };
}

/**
 * Move a full log aside, keeping one generation.
 *
 * Best-effort on purpose: if the rename fails the log simply keeps growing,
 * which is better than refusing to log at all.
 */
function rotate(file: string): void {
  let full: boolean;
  try {
    full = nodeFs.statSync(file).size >= MAX_BYTES;
  } catch {
    full = false; // matches metadata(path).is_ok_and(...) being false when missing
  }
  if (full) {
    try {
      nodeFs.renameSync(file, rotatedPath(file));
    } catch {
      // best-effort, matches the Rust `let _ = std::fs::rename(...)`
    }
  }
}

/** `ketch.log` → `ketch.log.1`, mirroring Rust's `path.with_extension("log.1")`. */
function rotatedPath(file: string): string {
  const dir = nodePath.dirname(file);
  const base = nodePath.basename(file);
  const dot = base.lastIndexOf(".");
  const stem = dot > 0 ? base.slice(0, dot) : base;
  return nodePath.join(dir, `${stem}.log.1`);
}

/**
 * Flatten a record onto one line.
 *
 * Every record is one line, so a log stays greppable and every reader from
 * `tail` to a shipper agrees where a record ends. Control characters go for
 * the same reason they go from a changelog: this file gets `cat`ed.
 */
function escape(message: string): string {
  let out = "";
  for (const c of message) {
    if (c === "\n") {
      out += "\\n";
    } else if (c === "\t") {
      out += "\\t";
    } else if (!isControl(c.codePointAt(0) ?? 0)) {
      out += c;
    }
  }
  return out;
}

/** C0 controls, DEL, and the C1 controls — the Unicode `Cc` category. */
function isControl(codePoint: number): boolean {
  return codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f);
}

function now(): number {
  return Math.floor(Date.now() / 1000);
}

/**
 * Seconds since the epoch as RFC 3339 UTC, which is what every log format in
 * common use writes and every parser in common use reads.
 */
function timestamp(secs: number): string {
  const days = divEuclid(secs, 86_400);
  const rest = modEuclid(secs, 86_400);
  const [y, m, d] = civilFromDays(days);
  const hh = Math.floor(rest / 3600);
  const mm = Math.floor((rest % 3600) / 60);
  const ss = rest % 60;
  return `${pad(y, 4)}-${pad(m, 2)}-${pad(d, 2)}T${pad(hh, 2)}:${pad(mm, 2)}:${pad(ss, 2)}Z`;
}

function pad(n: number, width: number): string {
  return String(n).padStart(width, "0");
}

/** Euclidean division: like Rust's `div_euclid`, the quotient rounds toward
 * negative infinity so the remainder `modEuclid` gives back is never negative. */
function divEuclid(a: number, b: number): number {
  const q = Math.trunc(a / b);
  return a % b < 0 ? q - 1 : q;
}

/** Euclidean remainder: like Rust's `rem_euclid`, always in `[0, b)`. */
function modEuclid(a: number, b: number): number {
  const r = a % b;
  return r < 0 ? r + b : r;
}

/**
 * Days since the epoch to a calendar date, by Howard Hinnant's
 * `civil_from_days`.
 *
 * Written out rather than pulled in: a date library is a dependency, a
 * build, and a supply chain, and this is the only date ketch formats.
 */
function civilFromDays(days: number): readonly [number, number, number] {
  const z = days + 719_468;
  const era = divEuclid(z, 146_097);
  const doe = z - era * 146_097; // [0, 146096]
  const yoe = Math.floor(
    (doe - Math.floor(doe / 1460) + Math.floor(doe / 36_524) - Math.floor(doe / 146_096)) / 365,
  ); // [0, 399]
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100)); // [0, 365]
  const mp = Math.floor((5 * doy + 2) / 153); // [0, 11], March-based
  const d = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  const y = yoe + era * 400 + (m <= 2 ? 1 : 0);
  return [y, m, d];
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}
