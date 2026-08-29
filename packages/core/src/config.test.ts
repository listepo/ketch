/** Ports of the config.rs unit tests, plus the precedence rules around them. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import type { ConfigFile } from "@ketch/schemas";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { binDirOnPath, ensureDirs, loadConfig, packageDir } from "./config.ts";

const KEYS = [
  "KETCH_ROOT",
  "KETCH_APPS_DIR",
  "KETCH_SELF_REPO",
  "KETCH_REGISTRY",
  "KETCH_GITHUB_TOKEN",
  "GITHUB_TOKEN",
  "GH_TOKEN",
  "KETCH_JOBS",
  "KETCH_LOG_LEVEL",
  "KETCH_LOG_FORMAT",
  "KETCH_PRERELEASE",
  "KETCH_ALLOW_EMULATION",
  "KETCH_LINK_APPS",
  "KETCH_REQUIRE_CHECKSUMS",
  "KETCH_STRIP_QUARANTINE",
];

let saved: Record<string, string | undefined> = {};
let root = "";

beforeEach(() => {
  saved = Object.fromEntries(KEYS.map((k) => [k, process.env[k]]));
  for (const k of KEYS) {
    delete process.env[k];
  }
  root = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-config-"));
});

afterEach(() => {
  for (const [k, v] of Object.entries(saved)) {
    if (v === undefined) {
      delete process.env[k];
    } else {
      process.env[k] = v;
    }
  }
  fs.rmSync(root, { recursive: true, force: true });
});

function writeFile(file: ConfigFile & Record<string, unknown>): void {
  fs.writeFileSync(path.join(root, "config.json"), JSON.stringify(file));
}

describe("config", () => {
  it("resolves relative roots against the current directory", () => {
    const cfg = loadConfig({ root: "scratch" });
    expect(path.isAbsolute(cfg.root)).toBe(true);
    expect(cfg.root).toBe(path.join(process.cwd(), "scratch"));
    expect(cfg.binDir).toBe(path.join(cfg.root, "bin"));
    expect(cfg.stateFile).toBe(path.join(cfg.root, "state.json"));
  });

  it("takes the root from the environment when no flag is given", () => {
    process.env["KETCH_ROOT"] = root;
    expect(loadConfig().root).toBe(root);
    expect(loadConfig({ root: `${root}/flag` }).root).toBe(path.join(root, "flag"));
  });

  it("prefers the environment over the config file", () => {
    writeFile({ prerelease: true, jobs: 2, log_level: "warn" });
    process.env["KETCH_PRERELEASE"] = "off";
    process.env["KETCH_JOBS"] = "8";
    const cfg = loadConfig({ root });
    expect(cfg.prerelease).toBe(false);
    expect(cfg.jobs).toBe(8);
    // Nothing overrides this one, so the file still decides.
    expect(cfg.logLevel).toBe("warn");
  });

  it("defaults emulation and quarantine stripping on, everything else off", () => {
    const cfg = loadConfig({ root });
    expect(cfg.allowEmulation).toBe(true);
    expect(cfg.stripQuarantine).toBe(true);
    expect(cfg.prerelease).toBe(false);
    expect(cfg.linkApps).toBe(false);
    expect(cfg.requireChecksums).toBe(false);
    expect(cfg.logLevel).toBe("info");
    expect(cfg.logFormat).toBe("text");
  });

  it("never runs zero jobs and never more than sixteen", () => {
    expect(loadConfig({ root }).jobs).toBe(4);
    writeFile({ jobs: 0 });
    expect(loadConfig({ root }).jobs).toBe(4);
    writeFile({ jobs: 99 });
    expect(loadConfig({ root }).jobs).toBe(16);
    writeFile({ jobs: 3 });
    expect(loadConfig({ root }).jobs).toBe(3);
  });

  it("refuses a job count that is not a whole number", () => {
    process.env["KETCH_JOBS"] = "lots";
    expect(() => loadConfig({ root })).toThrow(/KETCH_JOBS must be a whole number/);
  });

  it("refuses a relative apps directory", () => {
    process.env["KETCH_APPS_DIR"] = "Applications";
    expect(() => loadConfig({ root })).toThrow(/apps_dir must be an absolute path/);
    process.env["KETCH_APPS_DIR"] = "/Volumes/Apps";
    expect(loadConfig({ root }).appsDir).toBe("/Volumes/Apps");
  });

  it("says that root in the config file has no effect", () => {
    writeFile({ root: "/somewhere/else" });
    const warnings: string[] = [];
    const cfg = loadConfig({ root, warn: (m) => warnings.push(m) });
    expect(cfg.root).toBe(root);
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toMatch(/has no effect; set KETCH_ROOT or pass --root/);
  });

  it("names where an unparseable setting came from", () => {
    writeFile({ log_level: "verbose" });
    expect(() => loadConfig({ root })).toThrow(
      "`log_level` in config.json: unknown log level `verbose`; use off, error, warn, info or debug",
    );
    writeFile({});
    process.env["KETCH_LOG_FORMAT"] = "yaml";
    expect(() => loadConfig({ root })).toThrow(
      "KETCH_LOG_FORMAT: unknown log format `yaml`; use text or json",
    );
  });

  it("does not fall through from a token variable that was deliberately blanked", () => {
    process.env["KETCH_GITHUB_TOKEN"] = "";
    process.env["GITHUB_TOKEN"] = "inherited-from-ci";
    expect(loadConfig({ root }).githubToken).toBeNull();

    delete process.env["KETCH_GITHUB_TOKEN"];
    expect(loadConfig({ root }).githubToken).toBe("inherited-from-ci");
  });

  it("refuses a repository that is not owner/repo", () => {
    process.env["KETCH_REGISTRY"] = "http://evil/x";
    expect(() => loadConfig({ root })).toThrow(/is not a GitHub repository/);
    process.env["KETCH_REGISTRY"] = "github:someone/their-registry";
    expect(loadConfig({ root }).registry).toBe("someone/their-registry");
  });

  it("refuses a config file with an unknown key", () => {
    fs.writeFileSync(path.join(root, "config.json"), '{"prelease": true}');
    expect(() => loadConfig({ root })).toThrow(/config\.json/);
  });

  it("treats a missing config file as an empty one", () => {
    expect(loadConfig({ root }).selfRepo).toBe("listepo/ketch");
  });

  it("creates the layout but not the registry directory", () => {
    const cfg = loadConfig({ root });
    ensureDirs(cfg);
    ensureDirs(cfg);
    for (const dir of [cfg.binDir, cfg.storeDir, cfg.cacheDir, cfg.manifestDir, cfg.pluginDir]) {
      expect(fs.existsSync(dir)).toBe(true);
    }
    // Its existence is how ketch knows the registry has been fetched.
    expect(fs.existsSync(cfg.registryDir)).toBe(false);
  });

  it("keeps a version tag inside the store directory", () => {
    const cfg = loadConfig({ root });
    expect(packageDir(cfg, "rg", "release/1.2")).toBe(path.join(cfg.storeDir, "rg", "release-1.2"));
  });

  it("sees the bin directory on PATH only when it is there", () => {
    const cfg = loadConfig({ root });
    const path0 = process.env["PATH"];
    try {
      process.env["PATH"] = "/usr/bin:/bin";
      expect(binDirOnPath(cfg)).toBe(false);
      process.env["PATH"] = `/usr/bin:${cfg.binDir}:/bin`;
      expect(binDirOnPath(cfg)).toBe(true);
    } finally {
      if (path0 === undefined) {
        delete process.env["PATH"];
      } else {
        process.env["PATH"] = path0;
      }
    }
  });
});
