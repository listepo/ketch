/** Ports of the state.rs unit tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { STATE_VERSION } from "@ketch/schemas";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { InstalledPackage } from "./model.ts";
import { hostTarget, PackageRef, Version } from "./model.ts";
import { Lock, State } from "./state.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-state-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

function pkg(name: string): InstalledPackage {
  const version = Version.parse("1.0.0");
  const source = PackageRef.github("owner/repo");
  if (version === null || source === null) {
    throw new Error("fixture is not parseable");
  }
  return {
    name,
    version,
    source,
    tag: "v1.0.0",
    target: hostTarget(),
    asset_name: `${name}-1.0.0.tar.gz`,
    sha256: "0".repeat(64),
    checksum_verified: true,
    installed_at: 1_700_000_000,
    prefix: path.join(dir, "store", name, "1.0.0"),
    links: [
      {
        link: path.join(dir, "bin", name),
        target: path.join(dir, "store", name, name),
        kind: "symlink",
      },
    ],
    pinned: false,
    origin: "inferred",
    manifest: null,
  };
}

describe("state", () => {
  it("treats a missing file as nothing installed", async () => {
    const state = await State.loadPath(path.join(dir, "state.json"));
    expect(state.size).toBe(0);
    expect(state.version).toBe(STATE_VERSION);
  });

  it("refuses an empty state file rather than calling it empty", async () => {
    const file = path.join(dir, "state.json");
    fs.writeFileSync(file, "   \n");
    // Truncation to zero bytes is a crash, not an uninstall of everything.
    await expect(State.loadPath(file)).rejects.toThrow(/is empty/);
  });

  it("refuses a state file written by a newer ketch", async () => {
    const file = path.join(dir, "state.json");
    fs.writeFileSync(file, JSON.stringify({ version: STATE_VERSION + 1, packages: {} }));
    await expect(State.loadPath(file)).rejects.toThrow(
      /written by a newer ketch \(state version 2\)/,
    );
  });

  it("refuses a state file that is not JSON", async () => {
    const file = path.join(dir, "state.json");
    fs.writeFileSync(file, "{ this is not json");
    await expect(State.loadPath(file)).rejects.toThrow(/state\.json/);
  });

  it("round trips through disk", async () => {
    const file = path.join(dir, "state.json");
    const state = new State();
    state.insert(pkg("rg"));
    state.insert(pkg("fd"));
    await state.savePath(file);

    const back = await State.loadPath(file);
    expect(back.size).toBe(2);
    expect(back.names()).toEqual(["fd", "rg"]);
    const rg = back.get("rg");
    expect(rg?.version.toString()).toBe("1.0.0");
    expect(rg?.source.toString()).toBe("github:owner/repo");
    expect(rg?.checksum_verified).toBe(true);
    expect(rg?.links[0]?.kind).toBe("symlink");
  });

  it("writes packages in a stable order", async () => {
    const file = path.join(dir, "state.json");
    const state = new State();
    for (const name of ["rg", "fd", "bat"]) {
      state.insert(pkg(name));
    }
    await state.savePath(file);
    const first = fs.readFileSync(file, "utf8");

    const reordered = new State();
    for (const name of ["bat", "rg", "fd"]) {
      reordered.insert(pkg(name));
    }
    await reordered.savePath(file);
    // A state file that reshuffles itself on every save is unreadable in a diff.
    expect(fs.readFileSync(file, "utf8")).toBe(first);
  });

  it("leaves no staging file behind", async () => {
    const file = path.join(dir, "state.json");
    await new State().savePath(file);
    expect(fs.readdirSync(dir)).toEqual(["state.json"]);
  });

  it("finds a package by name, source or linked binary", () => {
    const state = new State();
    state.insert(pkg("rg"));

    expect(state.find("rg")?.name).toBe("rg");
    expect(state.find("owner/repo")?.name).toBe("rg");
    expect(state.find("github:owner/repo")?.name).toBe("rg");
    expect(state.find("OWNER/REPO")?.name).toBe("rg");
    expect(state.find("absent")).toBeUndefined();
  });

  it("forgets a package that was removed", () => {
    const state = new State();
    state.insert(pkg("rg"));
    expect(state.remove("rg")?.name).toBe("rg");
    expect(state.remove("rg")).toBeUndefined();
    expect(state.size).toBe(0);
  });
});

describe("lock", () => {
  it("is released again when the holder lets go", async () => {
    const file = path.join(dir, "lock");
    const lock = await Lock.acquirePath(file);
    expect(fs.existsSync(file)).toBe(true);
    lock.release();
    expect(fs.existsSync(file)).toBe(false);
  });

  it("does not deadlock against a lock this process already holds", async () => {
    const file = path.join(dir, "lock");
    const outer = await Lock.acquirePath(file);
    const inner = await Lock.acquirePath(file);
    // The inner one does not own the file, so releasing it must not free it.
    inner.release();
    expect(fs.existsSync(file)).toBe(true);
    outer.release();
    expect(fs.existsSync(file)).toBe(false);
  });

  it("reclaims a lock left behind by a dead process, without residue", async () => {
    const file = path.join(dir, "lock");
    fs.writeFileSync(file, "999999\n");
    const lock = await Lock.acquirePath(file);
    expect(fs.readFileSync(file, "utf8").trim()).toBe(String(process.pid));
    lock.release();
    expect(fs.readdirSync(dir)).toEqual([]);
  });

  it("never steals a lock a live process is holding", async () => {
    const file = path.join(dir, "lock");
    // pid 1 is launchd: alive, and not us.
    fs.writeFileSync(file, "1\n");
    await expect(Lock.acquirePath(file)).rejects.toThrow(/pid 1/);
    expect(fs.readFileSync(file, "utf8").trim()).toBe("1");
  });
});
