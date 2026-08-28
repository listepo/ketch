/**
 * Ports of the log.rs unit tests, plus coverage for the pino-backed writer
 * that log.rs never needed a test for (it has no equivalent: Rust prints
 * straight to a `File`).
 *
 * `escape` and `timestamp` are private here exactly as they are in Rust (not
 * `pub`), so — unlike `#[cfg(test)] mod tests { use super::*; }`, which can
 * reach them directly — these are proven indirectly, through `record` and a
 * real file on disk. Same claims as log.rs's tests; different vantage point,
 * because a TS module genuinely cannot see a sibling file's private symbols
 * the way a Rust child module can.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Config } from "./config.ts";
import { loadConfig } from "./config.ts";
import {
  init,
  levelPasses,
  parseFormat,
  parseLevel,
  path as logPath,
  record,
  resetForTests,
} from "./log.ts";

let root = "";

function freshConfig(logFormat: "text" | "json" = "json"): Config {
  root = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-log-"));
  const cfg = loadConfig({ root });
  return { ...cfg, logFormat };
}

function readLines(file: string): string[] {
  return fs
    .readFileSync(file, "utf8")
    .split("\n")
    .filter((line) => line !== "");
}

/** The first JSON line whose `msg` is exactly `msg` — records are looked up
 * by content rather than position, since `init`'s own startup line is always
 * first. */
function findRecord(file: string, msg: string): Record<string, unknown> {
  const found = readLines(file)
    .map((line) => JSON.parse(line) as Record<string, unknown>)
    .find((entry) => entry["msg"] === msg);
  if (found === undefined) {
    throw new Error(`no record with msg \`${msg}\` in ${file}`);
  }
  return found;
}

afterEach(() => {
  resetForTests();
  vi.useRealTimers();
  if (root !== "") {
    fs.rmSync(root, { recursive: true, force: true });
    root = "";
  }
});

describe("log", () => {
  it("timestamps_match_dates_everybody_knows", () => {
    const cfg = freshConfig("json");
    init(cfg, "0.0.0-test");
    vi.useFakeTimers();

    const cases: ReadonlyArray<readonly [number, string]> = [
      [0, "1970-01-01T00:00:00Z"],
      [86_399, "1970-01-01T23:59:59Z"],
      [86_400, "1970-01-02T00:00:00Z"],
      [1_000_000_000, "2001-09-09T01:46:40Z"],
      [1_700_000_000, "2023-11-14T22:13:20Z"],
    ];
    for (const [secs, expected] of cases) {
      vi.setSystemTime(secs * 1000);
      record("info", `at-${secs}`);
      expect(findRecord(cfg.logFile, `at-${secs}`)["time"]).toBe(expected);
    }
  });

  it("leap_days_land_on_the_29th", () => {
    const cfg = freshConfig("json");
    init(cfg, "0.0.0-test");
    vi.useFakeTimers();

    // 2024-02-29T00:00:00Z and 2000-02-29T00:00:00Z, the century that is a
    // leap year and the one every naive rule gets wrong.
    const cases: ReadonlyArray<readonly [number, string]> = [
      [1_709_164_800, "2024-02-29T00:00:00Z"],
      [951_782_400, "2000-02-29T00:00:00Z"],
      [4_107_542_400, "2100-03-01T00:00:00Z"],
    ];
    for (const [secs, expected] of cases) {
      vi.setSystemTime(secs * 1000);
      record("info", `leap-${secs}`);
      expect(findRecord(cfg.logFile, `leap-${secs}`)["time"]).toBe(expected);
    }
  });

  it("a_record_is_always_one_line", () => {
    const cfg = freshConfig("text");
    init(cfg, "0.0.0-test");
    record("info", "first\nsecond\ttabbed[2Kcleared");

    const lines = readLines(cfg.logFile);
    const line = lines.find((l) => l.includes("tabbed"));
    expect(line).toContain("first\\nsecond\\ttabbed[2Kcleared");
    expect(lines.every((l) => !l.includes("\n"))).toBe(true);
  });

  it("a_level_admits_itself_and_everything_more_serious", () => {
    expect(levelPasses("warn", "info")).toBe(true);
    expect(levelPasses("error", "info")).toBe(true);
    expect(levelPasses("info", "info")).toBe(true);
    expect(levelPasses("debug", "info")).toBe(false);
    expect(levelPasses("warn", "error")).toBe(false);
    expect(levelPasses("debug", "debug")).toBe(true);
  });

  it("off_writes_nothing_at_all", () => {
    for (const level of ["error", "warn", "info", "debug"] as const) {
      expect(levelPasses(level, "off")).toBe(false);
    }
  });

  it("an_off_level_never_opens_a_log_file", () => {
    const cfg = freshConfig();
    const off: Config = { ...cfg, logLevel: "off" };

    expect(init(off, "0.0.0-test")).toBeNull();
    expect(logPath()).toBeNull();
    expect(fs.existsSync(off.logFile)).toBe(false);

    record("error", "should never be written");
    expect(fs.existsSync(off.logFile)).toBe(false);
  });

  it("levels_and_formats_parse_the_names_people_type", () => {
    // Not found already covered in config.test.ts as of this port: config's
    // own tests exercise `loadConfig`'s error paths for a bad level/format,
    // not the full synonym table, so this stays here where parseLevel and
    // parseFormat actually live.
    expect(parseLevel("WARNING")).toBe("warn");
    expect(parseLevel("off")).toBe("off");
    expect(parseLevel(" Debug ")).toBe("debug");
    expect(() => parseLevel("chatty")).toThrow();
    expect(parseFormat("ndjson")).toBe("json");
    expect(parseFormat("text")).toBe("text");
    expect(() => parseFormat("xml")).toThrow();
  });

  it("a_json_record_carries_exactly_time_level_pid_and_msg", () => {
    const cfg = freshConfig("json");
    init(cfg, "0.0.0-test");
    record("warn", "disk almost full");

    const rec = findRecord(cfg.logFile, "disk almost full");
    expect(Object.keys(rec).toSorted()).toEqual(["level", "msg", "pid", "time"]);
    expect(rec["level"]).toBe("warn");
    expect(rec["pid"]).toBe(process.pid);
    expect(rec["time"]).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/);
  });

  it("never writes a message below the configured level", () => {
    const cfg = { ...freshConfig("json"), logLevel: "warn" as const };
    init(cfg, "0.0.0-test");
    record("info", "should be filtered out");
    record("error", "should survive");

    const lines = readLines(cfg.logFile).map((l) => JSON.parse(l) as Record<string, unknown>);
    expect(lines.some((l) => l["msg"] === "should be filtered out")).toBe(false);
    expect(lines.some((l) => l["msg"] === "should survive")).toBe(true);
  });

  it("path_is_null_until_init_succeeds", () => {
    expect(logPath()).toBeNull();
    const cfg = freshConfig();
    init(cfg, "0.0.0-test");
    expect(logPath()).toBe(cfg.logFile);
  });

  it("init_soft_fails_when_the_log_cannot_be_opened", () => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-log-"));
    const cfg = loadConfig({ root });
    // A plain file sits where the log's parent directory needs to be, so
    // `mkdir` fails.
    fs.writeFileSync(path.dirname(cfg.logFile), "not a directory");

    const warning = init(cfg, "0.0.0-test");
    expect(warning).not.toBeNull();
    expect(warning).toContain(cfg.logFile);
    expect(logPath()).toBeNull();
  });

  it("rotates a full log to .log.1 and starts the new one fresh", () => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-log-"));
    const cfg = loadConfig({ root });
    fs.mkdirSync(path.dirname(cfg.logFile), { recursive: true });
    fs.writeFileSync(cfg.logFile, "x".repeat(6 * 1024 * 1024));

    init(cfg, "0.0.0-test");

    const rotated = `${cfg.logFile}.1`;
    expect(fs.statSync(rotated).size).toBe(6 * 1024 * 1024);
    expect(fs.statSync(cfg.logFile).size).toBeLessThan(6 * 1024 * 1024);
  });
});
