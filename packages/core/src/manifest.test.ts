/** Ports of the manifest.rs unit tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { builtinPackages } from "@ketch/schemas";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { loadConfig } from "./config.ts";
import { Resolver, parseManifestFile } from "./manifest.ts";
import type { Manifest } from "./model.ts";
import { inferredManifest, PackageRef, PackageSpec } from "./model.ts";
import { PACKAGE_FILE } from "./registry.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-manifest-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

function resolver(): Resolver {
  return new Resolver(builtinPackages, [], []);
}

describe("Resolver.resolve", () => {
  it("builtin registry parses and is reachable by alias", () => {
    expect(builtinPackages.length).toBeGreaterThan(0);
    const [manifest, origin] = resolver().resolve(PackageSpec.parse("rg"));
    expect(manifest.name).toBe("ripgrep");
    expect(origin).toBe("builtin");
  });

  it("a reference still picks up the curated manifest", () => {
    const [manifest, origin] = resolver().resolve(PackageSpec.parse("BurntSushi/ripgrep@14.1.0"));
    expect(origin).toBe("builtin");
    expect(manifest.bin[0]?.name).toBe("rg");
  });

  it("an uncurated reference falls through to inference", () => {
    const [manifest, origin] = resolver().resolve(PackageSpec.parse("someone/whatever-tool"));
    expect(origin).toBe("inferred");
    expect(manifest.name).toBe("whatever-tool");
    expect(manifest.source).toBe("github:someone/whatever-tool");
  });

  it("an unknown bare name is an error not a guess", () => {
    expect(() => resolver().resolve(PackageSpec.parse("definitely-not-a-package"))).toThrow();
  });

  it("the registry shadows builtins and lists each name once", () => {
    const entry: Manifest = {
      ...inferredManifest(PackageRef.github("registry/ripgrep")),
      provides: ["rg"],
    };
    const r = new Resolver(builtinPackages, [[entry, "/tmp/registry/ripgrep/ketch.json"]], []);

    const [manifest, origin] = r.resolve(PackageSpec.parse("rg"));
    expect(manifest.source).toBe("github:registry/ripgrep");
    expect(origin).toEqual({ registry: "/tmp/registry/ripgrep/ketch.json" });

    // The built-in `ripgrep` is the same package by another route, so it
    // must not show up as a second search result.
    const hits = r.search("ripgrep");
    expect(hits).toHaveLength(1);
    expect(hits[0]?.source).toBe("github:registry/ripgrep");
  });

  it("user manifests shadow builtins", () => {
    const mine: Manifest = {
      ...inferredManifest(PackageRef.github("me/my-ripgrep")),
      provides: ["rg"],
    };
    const r = new Resolver(builtinPackages, [], [[mine, "/tmp/my-ripgrep.json"]]);
    const [manifest, origin] = r.resolve(PackageSpec.parse("rg"));
    expect(manifest.source).toBe("github:me/my-ripgrep");
    expect(origin).toEqual({ user: "/tmp/my-ripgrep.json" });
  });
});

describe("parseManifestFile", () => {
  it("a single manifest that mentions the array marker is still a manifest", () => {
    // Sniffing the source text for the word "package" would parse this as a
    // registry of zero packages and drop the file without a word; the
    // decision has to come from the parsed shape instead.
    const text = JSON.stringify({
      name: "thing",
      source: "github:o/thing",
      notes: "declare it under a `package` array to ship several at once",
    });
    const found = parseManifestFile(text, "test");
    expect(found).toHaveLength(1);
    expect(found[0]?.name).toBe("thing");
  });

  it("a package array holds more than one manifest", () => {
    const text = JSON.stringify({
      package: [
        { name: "one", source: "github:o/one" },
        { name: "two", source: "github:o/two" },
      ],
    });
    const found = parseManifestFile(text, "test");
    expect(found.map((m) => m.name)).toEqual(["one", "two"]);
  });

  it("a validation failure in a package array names the broken entry", () => {
    const text = JSON.stringify({
      package: [
        { name: "one", source: "github:o/one" },
        { name: "two", source: "github:o/two", bin: [{}] },
      ],
    });
    expect(() => parseManifestFile(text, "test")).toThrow("package `two`");
  });
});

describe("Resolver.create", () => {
  it("reads user manifests and the registry from disk, and skips a broken user manifest", () => {
    const cfg = loadConfig({ root: dir });
    fs.mkdirSync(cfg.manifestDir, { recursive: true });
    fs.writeFileSync(
      path.join(cfg.manifestDir, "my-rg.json"),
      JSON.stringify({ name: "my-rg", source: "github:me/my-ripgrep", provides: ["rg"] }),
    );
    fs.writeFileSync(path.join(cfg.manifestDir, "broken.json"), "{ not json");
    fs.mkdirSync(path.join(cfg.registryDir, "fd"), { recursive: true });
    fs.writeFileSync(
      path.join(cfg.registryDir, "fd", PACKAGE_FILE),
      JSON.stringify({ source: "github:sharkdp/fd" }),
    );

    const warnings: string[] = [];
    const r = Resolver.create(cfg, (message) => warnings.push(message));

    const [rg, rgOrigin] = r.resolve(PackageSpec.parse("rg"));
    expect(rg.name).toBe("my-rg");
    expect(rgOrigin).toEqual({ user: path.join(cfg.manifestDir, "my-rg.json") });

    const [fd, fdOrigin] = r.resolve(PackageSpec.parse("fd"));
    expect(fd.name).toBe("fd");
    expect(fdOrigin).toEqual({ registry: path.join(cfg.registryDir, "fd", PACKAGE_FILE) });

    expect(warnings.some((w) => w.includes("broken.json"))).toBe(true);
  });
});
