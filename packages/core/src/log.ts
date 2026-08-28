/**
 * The log file: how much gets written, and in what shape.
 *
 * Only the vocabulary lives here so far. It is in this module rather than in
 * the config loader because the log owns the concepts — the loader merely has
 * to parse both before any logging exists to report a bad value. The writer
 * lands beside them.
 */

import { asciiLowercase } from "@ketch/schemas";

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
