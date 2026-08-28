/** Ports of the lockfile.rs unit tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { Lockfile, plan } from "./lockfile.ts";
import type { InstalledPackage } from "./model.ts";
import { hostTarget, PackageRef, targetString, Version } from "./model.ts";
import { State } from "./state.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-lockfile-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

function installed(name: string, repo: string, tag: string): InstalledPackage {
  return {
    name,
    version: Version.parse(tag),
    source: PackageRef.github(repo),
    tag,
    target: hostTarget(),
    asset_name: `${name}.tar.gz`,
    sha256: "a".repeat(64),
    checksum_verified: true,
    installed_at: 0,
    prefix: `/store/${name}`,
    links: [],
    pinned: false,
    origin: "inferred",
    manifest: null,
  };
}

function stateWith(packages: InstalledPackage[]): State {
  const state = new State();
  for (const pkg of packages) {
    state.insert(pkg);
  }
  return state;
}

function lockOf(state: State): Lockfile {
  return Lockfile.fromState(state);
}

describe("lockfile", () => {
  it("a lockfile round-trips through a save and load", async () => {
    const state = stateWith([
      installed("ripgrep", "BurntSushi/ripgrep", "14.1.1"),
      installed("fd", "sharkdp/fd", "v10.2.0"),
    ]);
    const file = path.join(dir, "ketch.lock");
    await lockOf(state).save(file);
    const parsed = await Lockfile.load(file);
    expect(parsed.packages).toEqual(lockOf(state).packages);
  });

  it("packages are written in a stable order", () => {
    const state = stateWith([
      installed("ripgrep", "BurntSushi/ripgrep", "14.1.1"),
      installed("fd", "sharkdp/fd", "v10.2.0"),
      installed("jq", "jqlang/jq", "jq-1.7"),
    ]);
    const names = lockOf(state).packages.map((pkg) => pkg.name);
    expect(names).toEqual(["fd", "jq", "ripgrep"]);
  });

  it("a tree that matches its lockfile is clean", () => {
    const state = stateWith([installed("fd", "sharkdp/fd", "v10.2.0")]);
    const p = plan(lockOf(state), state);
    expect(p.isClean(true)).toBe(true);
    expect(p.matched).toBe(1);
  });

  it("a different tag is drift, not a missing package", () => {
    const state = stateWith([installed("fd", "sharkdp/fd", "v10.2.0")]);
    const lock = lockOf(state);
    const moved = stateWith([installed("fd", "sharkdp/fd", "v10.3.0")]);
    const p = plan(lock, moved);
    expect(p.missing).toEqual([]);
    expect(p.changed).toHaveLength(1);
    expect(p.changed[0]?.[1]).toBe("v10.3.0");
  });

  it("a package renamed upstream is still the same package", () => {
    const state = stateWith([installed("fd", "sharkdp/fd", "v10.2.0")]);
    const lock = lockOf(state);
    // Same source, installed under a different name.
    const renamed = stateWith([installed("fd-find", "sharkdp/fd", "v10.2.0")]);
    const p = plan(lock, renamed);
    expect(p.matched).toBe(1);
    expect(p.extra).toEqual([]);
  });

  it("something installed that the lockfile omits is extra", () => {
    const lock = lockOf(stateWith([installed("fd", "sharkdp/fd", "v10.2.0")]));
    const more = stateWith([
      installed("fd", "sharkdp/fd", "v10.2.0"),
      installed("jq", "jqlang/jq", "jq-1.7"),
    ]);
    const p = plan(lock, more);
    expect(p.extra).toEqual(["jq"]);
    expect(p.isClean(false)).toBe(true); // extras alone are not drift
    expect(p.isClean(true)).toBe(false); // with --prune they are
  });

  it("the pinned flag survives a round trip", async () => {
    const pkg = installed("fd", "sharkdp/fd", "v10.2.0");
    const file = path.join(dir, "ketch.lock");
    await lockOf(stateWith([{ ...pkg, pinned: true }])).save(file);
    const parsed = await Lockfile.load(file);
    expect(parsed.packages[0]?.pinned).toBe(true);
  });

  it("the recorded hash only applies to the machine that wrote it", () => {
    const lock = lockOf(stateWith([installed("fd", "sharkdp/fd", "v10.2.0")]));
    const entry = lock.packages[0];
    expect(entry?.matchesTarget(targetString(hostTarget()))).toBe(true);
    expect(entry?.matchesTarget("bogus-target")).toBe(false);
  });

  // Not in the Rust suite (its unit tests never touch the filesystem), but
  // the module doc's central claim — "a missing one is not an empty one" —
  // is otherwise unchecked, and it is the one branch with no sibling
  // precedent: state.ts treats a missing file as empty, lockfile.ts must not.
  it("a missing lockfile is an error, not an empty one", async () => {
    await expect(Lockfile.load(path.join(dir, "ketch.lock"))).rejects.toThrow(/no lockfile at/);
  });
});
