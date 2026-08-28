/** Ports of the registry.rs unit tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { loadConfig } from "./config.ts";
import { collisions, load, loadDir, PACKAGE_FILE, swapIn } from "./registry.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-registry-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

function write(root: string, folder: string, body: Record<string, unknown>): void {
  const pkgDir = path.join(root, folder);
  fs.mkdirSync(pkgDir, { recursive: true });
  fs.writeFileSync(path.join(pkgDir, PACKAGE_FILE), JSON.stringify(body));
}

describe("loadDir", () => {
  it("the folder names the package", () => {
    write(dir, "ripgrep", { source: "github:BurntSushi/ripgrep" });
    const found = loadDir(dir);
    expect(found).toHaveLength(1);
    expect(found[0]?.[0].name).toBe("ripgrep");
    expect(found[0]?.[0].source).toBe("github:BurntSushi/ripgrep");
  });

  it("a declared name must match its folder", () => {
    write(dir, "fzf", { name: "fzy", source: "github:junegunn/fzf" });
    expect(loadDir(dir)).toEqual([]);
  });

  it("non-package folders and broken entries are skipped", () => {
    fs.mkdirSync(path.join(dir, ".github", "workflows"), { recursive: true });
    fs.writeFileSync(path.join(dir, "README.md"), "hi");
    write(dir, "broken", { source: 12 });
    write(dir, "jq", { source: "github:jqlang/jq" });
    const found = loadDir(dir);
    expect(found).toHaveLength(1);
    expect(found[0]?.[0].name).toBe("jq");
  });

  it("a package that would install outside the store is refused", () => {
    write(dir, "evil", {
      name: "evil",
      source: "github:a/b",
      bin: [{ name: "../../../.zshrc" }],
    });
    write(dir, "typo", { source: "github:a/b", binary: "x" });
    write(dir, "ok", { source: "github:a/b" });
    const found = loadDir(dir);
    expect(found).toHaveLength(1);
    expect(found[0]?.[0].name).toBe("ok");
  });
});

describe("collisions", () => {
  it("a name two packages claim is reported once against its first owner", () => {
    // `fd` provides its own name, which is not a collision with itself.
    write(dir, "fd", { source: "github:sharkdp/fd", provides: ["fd"] });
    write(dir, "rg", { source: "github:BurntSushi/ripgrep" });
    expect(collisions(loadDir(dir))).toEqual([]);

    write(dir, "zfd", { source: "github:someone/zfd", provides: ["fd"] });
    const found = collisions(loadDir(dir));
    expect(found).toHaveLength(1);
    expect(found[0]).toContain("both `fd` and `zfd`");
  });
});

describe("swapIn", () => {
  it("an empty tree never replaces a working registry", () => {
    const cfg = loadConfig({ root: dir });
    fs.mkdirSync(cfg.registryDir, { recursive: true });
    write(cfg.registryDir, "jq", { source: "github:jqlang/jq" });

    const empty = path.join(dir, "empty");
    fs.mkdirSync(empty, { recursive: true });
    expect(() => swapIn(cfg, empty, "someone/registry")).toThrow();
    // The old registry must still be there.
    expect(load(cfg)).toHaveLength(1);

    const fresh = path.join(dir, "fresh");
    write(fresh, "fd", { source: "github:sharkdp/fd" });
    write(fresh, "rg", { source: "github:BurntSushi/ripgrep" });
    expect(swapIn(cfg, fresh, "someone/registry")).toBe(2);
    expect(load(cfg).map(([m]) => m.name)).toEqual(["fd", "rg"]);
  });
});
