/** Ports of the install.rs unit tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import type { Config } from "./config.ts";
import { loadConfig } from "./config.ts";
import { removeStoreDir, scoreAssets } from "./install.ts";
import type { AssetSelector, Release, ReleaseAsset } from "./model.ts";
import { Version } from "./model.ts";
import type { Platform } from "./platform/platform.ts";

/**
 * Stands in for a real platform: macOS assets only, longer names never
 * outrank shorter ones by accident.
 */
const fakePlatform: Platform = {
  id: "fake",
  target: () => ({ os: "macos", arch: "aarch64" }),
  scoreAsset: (name) =>
    name.includes("darwin")
      ? { score: 50, arch: "aarch64", emulated: false, reason: "fake" }
      : null,
  extractors: () => [],
  place: async () => [],
  unplace: async () => {},
  verifyTrust: async () => ({ kind: "not_applicable" }),
  clearQuarantine: async () => {},
  isExecutable: async () => true,
  appBundleExtension: () => null,
  doctor: async () => [],
};

function asset(name: string): ReleaseAsset {
  return {
    name,
    url: `https://example.invalid/${name}`,
    size: 1,
    content_type: null,
    digest: null,
    headers: {},
  };
}

function release(names: readonly string[]): Release {
  return {
    version: Version.parse("1.0.0"),
    tag: "v1.0.0",
    prerelease: false,
    draft: false,
    published_at: null,
    notes: null,
    assets: names.map((name) => asset(name)),
  };
}

function config(): Config {
  const cfg = loadConfig({ root: path.join(os.tmpdir(), "ketch-test-root") });
  return { ...cfg, target: { os: "macos", arch: "aarch64" } };
}

function emptySelector(): AssetSelector {
  return { include: [], exclude: [], target: {} };
}

describe("scoreAssets", () => {
  it("drops assets the platform cannot run", () => {
    const picked = scoreAssets(
      config(),
      fakePlatform,
      release(["tool-linux.tar.gz", "tool-darwin.tar.gz"]),
      emptySelector(),
    );
    expect(picked).toHaveLength(1);
    expect(picked[0]?.asset.name).toBe("tool-darwin.tar.gz");
  });

  it("exclude wins over include and over the target pin", () => {
    const selector: AssetSelector = {
      include: ["*darwin*"],
      exclude: ["*.dmg"],
      target: { "macos-aarch64": "*.dmg" },
    };
    const picked = scoreAssets(
      config(),
      fakePlatform,
      release(["tool-darwin.dmg", "tool-darwin.tar.gz"]),
      selector,
    );
    // The pin would have taken the dmg; the exclusion removes it first, and
    // with a pin present the tarball is not considered either.
    expect(picked).toHaveLength(0);
  });

  it("a target pin overrides platform scoring", () => {
    const selector: AssetSelector = {
      include: [],
      exclude: [],
      target: { "macos-aarch64": "*-mac-universal.zip" },
    };
    // `mac` alone would score null from the fake platform; the pin still wins.
    const picked = scoreAssets(
      config(),
      fakePlatform,
      release(["tool-darwin.tar.gz", "tool-mac-universal.zip"]),
      selector,
    );
    expect(picked).toHaveLength(1);
    expect(picked[0]?.asset.name).toBe("tool-mac-universal.zip");
  });
});

describe("removeStoreDir", () => {
  it("refuses to delete outside the store", () => {
    const cfg = config();
    const outside = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-not-the-store"));
    try {
      removeStoreDir(cfg, outside);
      expect(fs.statSync(outside).isDirectory(), "a path outside the store must survive").toBe(
        true,
      );
    } finally {
      fs.rmSync(outside, { recursive: true, force: true });
    }
  });
});
