/** Port of the platform/macos.rs tests, plus the injectable-runner trust checks. */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { hostTarget } from "../model.ts";
import {
  createDarwinPlatform,
  ensureExecutable,
  findAppBundles,
  linkBinary,
  looksLikeBuildArtifact,
  moveIntoStore,
  placeApp,
  preflightDestinations,
  tokenAt,
} from "./darwin.ts";
import type { AssetScore, Placement } from "./platform.ts";

const tmpDirs: string[] = [];

function tmpdir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-darwin-"));
  tmpDirs.push(dir);
  return dir;
}

afterEach(() => {
  for (const dir of tmpDirs.splice(0)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

function score(name: string): AssetScore | null {
  return createDarwinPlatform().scoreAsset(name, true);
}

function mustScore(name: string): AssetScore {
  const scored = score(name);
  if (scored === null) {
    throw new Error(`expected ${name} to score`);
  }
  return scored;
}

function writeExecutable(file: string): void {
  fs.writeFileSync(file, "#!/bin/sh\n");
  fs.chmodSync(file, 0o755);
}

function linkExists(file: string): boolean {
  try {
    fs.lstatSync(file);
    return true;
  } catch {
    return false;
  }
}

function binaryPlan(
  overrides: Partial<Placement> & Pick<Placement, "payloadDir" | "storeDir" | "binDir" | "appsDir">,
): Placement {
  return {
    name: "pkg",
    version: "1.0",
    kind: "binary",
    binSpecs: [],
    replacing: [],
    linkApps: false,
    link: true,
    ...overrides,
  };
}

describe("asset scoring", () => {
  it("token matching respects word boundaries", () => {
    // The whole reason `includes` is not good enough.
    expect(tokenAt("x86_64-apple-darwin", "win")).toBe(false);
    expect(tokenAt("tool-windows-amd64.zip", "windows")).toBe(true);
    expect(tokenAt("tool-install.tar.gz", "all")).toBe(false);
    expect(tokenAt("tool-universal-all.zip", "all")).toBe(true);
  });

  it("rejects foreign platforms and sidecars", () => {
    for (const name of [
      "rg-14.1.0-x86_64-unknown-linux-musl.tar.gz",
      "tool-windows-amd64.zip",
      "rg-14.1.0-aarch64-apple-darwin.tar.gz.sha256",
      "ripgrep_14.1.0_amd64.deb",
      "checksums.txt",
      "tool-macos-arm64.dSYM.zip",
    ]) {
      expect(score(name), `should have rejected ${name}`).toBeNull();
    }
  });

  it("prefers the native architecture over emulation", () => {
    const native = mustScore("rg-14.1.0-aarch64-apple-darwin.tar.gz");
    const rosetta = mustScore("rg-14.1.0-x86_64-apple-darwin.tar.gz");
    if (hostTarget().arch === "aarch64") {
      expect(native.score).toBeGreaterThan(rosetta.score);
      expect(rosetta.emulated).toBe(true);
      expect(native.emulated).toBe(false);
      // Emulation is a choice, not a default the user cannot refuse.
      expect(
        createDarwinPlatform().scoreAsset("rg-14.1.0-x86_64-apple-darwin.tar.gz", false),
      ).toBeNull();
    }
  });

  it("universal builds are accepted on any mac", () => {
    const universal = mustScore("tool-1.0-universal2-apple-darwin.tar.gz");
    expect(universal.arch).toBe("universal");
    expect(universal.emulated).toBe(false);
  });

  it("names without an os still qualify but rank lower", () => {
    const explicit = mustScore("tool_1.0_darwin_arm64.tar.gz");
    const bare = mustScore("tool_1.0_arm64.tar.gz");
    expect(explicit.score).toBeGreaterThan(bare.score);
  });

  it("recognises build metadata in a binary name", () => {
    expect(looksLikeBuildArtifact("jq-macos-arm64")).toBe(true);
    expect(looksLikeBuildArtifact("tool-v1.2.3")).toBe(true);
    expect(looksLikeBuildArtifact("rg")).toBe(false);
    expect(looksLikeBuildArtifact("fd")).toBe(false);
  });
});

describe("placement", () => {
  it("making a binary executable does not grant read access", () => {
    const file = path.join(tmpdir(), "tool");
    fs.writeFileSync(file, "#!/bin/sh\n");
    fs.chmodSync(file, 0o600);

    ensureExecutable(file);

    expect(fs.statSync(file).mode & 0o7777).toBe(0o711);
  });

  it("app bundles are found but their helpers are not", () => {
    const root = tmpdir();
    const app = path.join(root, "Thing.app");
    fs.mkdirSync(path.join(app, "Contents/Updater.app"), { recursive: true });
    fs.mkdirSync(path.join(root, "Extra.app"));

    const found = findAppBundles(root);
    found.sort();
    expect(found).toEqual([path.join(root, "Extra.app"), app]);
  });

  it("a failed upgrade leaves the installed version in place", async () => {
    const tmp = tmpdir();
    const store = path.join(tmp, "store/tool/1.0");
    fs.mkdirSync(store, { recursive: true });
    fs.writeFileSync(path.join(store, "tool"), "the working version");

    // Nothing to move: the version already installed must survive.
    await expect(moveIntoStore(path.join(tmp, "missing"), store)).rejects.toThrow();
    expect(fs.readFileSync(path.join(store, "tool"), "utf8")).toBe("the working version");

    const payload = path.join(tmp, "payload");
    fs.mkdirSync(payload);
    fs.writeFileSync(path.join(payload, "tool"), "the new version");
    await moveIntoStore(payload, store);
    expect(fs.readFileSync(path.join(store, "tool"), "utf8")).toBe("the new version");
    expect(linkExists(path.join(tmp, "store/tool/1.0.old"))).toBe(false);
    expect(linkExists(path.join(tmp, "store/tool/1.0.incoming"))).toBe(false);
  });

  it("a binary name another package owns is not taken over", () => {
    const tmp = tmpdir();
    const mine = path.join(tmp, "store/mine");
    const theirs = path.join(tmp, "store/theirs/1.0");
    fs.mkdirSync(path.join(mine, "1.0"), { recursive: true });
    fs.mkdirSync(theirs, { recursive: true });
    const target = path.join(mine, "1.0/tool");
    fs.writeFileSync(target, "#!/bin/sh\n");
    fs.writeFileSync(path.join(theirs, "tool"), "#!/bin/sh\n");

    const bin = path.join(tmp, "bin");
    fs.mkdirSync(bin);
    fs.symlinkSync(path.join(theirs, "tool"), path.join(bin, "tool"));
    expect(() => linkBinary(target, bin, "tool", mine, [])).toThrow();
    // The other package must still own its link.
    expect(fs.readlinkSync(path.join(bin, "tool"))).toBe(path.join(theirs, "tool"));

    // An older version of the same package is ours to replace.
    const old = path.join(mine, "0.9");
    fs.mkdirSync(old);
    fs.writeFileSync(path.join(old, "tool"), "#!/bin/sh\n");
    fs.unlinkSync(path.join(bin, "tool"));
    fs.symlinkSync(path.join(old, "tool"), path.join(bin, "tool"));
    const record = linkBinary(target, bin, "tool", mine, []);
    expect(fs.readlinkSync(record.link)).toBe(target);
  });

  it("placement checks all binary destinations before replacing any", () => {
    const tmp = tmpdir();
    const payload = path.join(tmp, "payload");
    fs.mkdirSync(payload);
    for (const name of ["first", "second"]) {
      writeExecutable(path.join(payload, name));
    }

    const bin = path.join(tmp, "bin");
    fs.mkdirSync(bin);
    const external = path.join(tmp, "external");
    fs.writeFileSync(external, "#!/bin/sh\n");
    fs.symlinkSync(external, path.join(bin, "second"));

    const store = path.join(tmp, "store/pkg/1.0");
    const plan = binaryPlan({
      payloadDir: payload,
      storeDir: store,
      binDir: bin,
      appsDir: path.join(tmp, "Applications"),
    });

    expect(() => preflightDestinations(plan, path.dirname(store))).toThrow();
    // Preflight must not create links.
    expect(linkExists(path.join(bin, "first"))).toBe(false);
  });

  it("placement preflight checks the package name for a single build artifact", () => {
    const tmp = tmpdir();
    const payload = path.join(tmp, "payload");
    fs.mkdirSync(payload);
    writeExecutable(path.join(payload, "tool-macos-arm64"));

    const bin = path.join(tmp, "bin");
    fs.mkdirSync(bin);
    const external = path.join(tmp, "external");
    fs.writeFileSync(external, "#!/bin/sh\n");
    fs.symlinkSync(external, path.join(bin, "tool"));

    const store = path.join(tmp, "store/tool/1.0");
    const plan = binaryPlan({
      name: "tool",
      payloadDir: payload,
      storeDir: store,
      binDir: bin,
      appsDir: path.join(tmp, "Applications"),
    });

    expect(() => preflightDestinations(plan, path.dirname(store))).toThrow();
  });

  it("an app ketch did not install is never deleted", async () => {
    const tmp = tmpdir();
    const packageDir = path.join(tmp, "store/thing");
    const bundle = path.join(packageDir, "1.0/Thing.app");
    fs.mkdirSync(path.join(bundle, "Contents"), { recursive: true });
    fs.writeFileSync(path.join(bundle, "Contents/Info.plist"), "x");

    const apps = path.join(tmp, "Applications");
    const existing = path.join(apps, "Thing.app");
    fs.mkdirSync(existing, { recursive: true });
    fs.writeFileSync(path.join(existing, "mine.txt"), "the user's own copy");

    await expect(placeApp(bundle, apps, false, packageDir, [])).rejects.toThrow();
    // Must not be deleted.
    expect(fs.statSync(path.join(existing, "mine.txt")).isFile()).toBe(true);

    // A copied bundle leaves no mark on disk; the record we wrote when we
    // placed it is the only thing that makes it ours to replace.
    const recorded = [{ link: existing, target: bundle, kind: "copied_app" as const }];
    await placeApp(bundle, apps, false, packageDir, recorded);
    expect(fs.statSync(path.join(existing, "Contents/Info.plist")).isFile()).toBe(true);
    expect(linkExists(path.join(existing, "mine.txt"))).toBe(false);
  });

  it("a link that now points elsewhere survives uninstall", async () => {
    const tmp = tmpdir();
    const ours = path.join(tmp, "ours");
    const theirs = path.join(tmp, "theirs");
    fs.writeFileSync(ours, "x");
    fs.writeFileSync(theirs, "x");
    const link = path.join(tmp, "tool");
    fs.symlinkSync(theirs, link);

    const record = { link, target: ours, kind: "symlink" as const };
    const platform = createDarwinPlatform();
    await platform.unplace([record]);
    // Not ours to remove.
    expect(linkExists(link)).toBe(true);

    fs.unlinkSync(link);
    fs.symlinkSync(ours, link);
    await platform.unplace([record]);
    expect(linkExists(link)).toBe(false);
  });
});

/** A platform whose `codesign`/`spctl` are canned transcripts. */
function fakeToolPlatform(transcript: {
  verify: { ok: boolean; output: string };
  describe: string;
  assess?: { ok: boolean; output: string };
}) {
  return createDarwinPlatform({
    toolExists: () => true,
    runner: (_program, args) => {
      if (args[0] === "--verify") {
        return Promise.resolve(transcript.verify);
      }
      if (args[0] === "-dv") {
        return Promise.resolve({ ok: true, output: transcript.describe });
      }
      return Promise.resolve(transcript.assess ?? { ok: true, output: "accepted" });
    },
  });
}

describe("trust checks", () => {
  it("a notarized developer id signature is trusted", async () => {
    const platform = fakeToolPlatform({
      verify: { ok: true, output: "" },
      describe:
        "Executable=/x\nAuthority=Developer ID Application: Example Corp\nAuthority=Apple Root CA\n",
    });
    expect(await platform.verifyTrust("/x")).toEqual({
      kind: "trusted",
      authority: "Developer ID Application: Example Corp",
    });
  });

  it("a broken signature is untrusted with the tool's first line", async () => {
    const platform = fakeToolPlatform({
      verify: { ok: false, output: "\n/x: code object is not signed at all\n" },
      describe: "",
    });
    expect(await platform.verifyTrust("/x")).toEqual({
      kind: "untrusted",
      detail: "/x: code object is not signed at all",
    });
  });

  it("a valid ad-hoc signature is weak", async () => {
    const platform = fakeToolPlatform({
      verify: { ok: true, output: "" },
      describe: "Executable=/x\nSignature=adhoc\n",
    });
    expect(await platform.verifyTrust("/x")).toEqual({
      kind: "weak",
      detail: "ad-hoc signature, no signing authority",
    });
  });

  it("an unnotarized developer id signature is weak", async () => {
    const platform = fakeToolPlatform({
      verify: { ok: true, output: "" },
      describe: "Authority=Developer ID Application: Example Corp\n",
      assess: { ok: false, output: "/x: rejected\nsource=Unnotarized Developer ID\n" },
    });
    expect(await platform.verifyTrust("/x")).toEqual({
      kind: "weak",
      detail: "Developer ID Application: Example Corp; system policy: /x: rejected",
    });
  });

  it("a machine without codesign skips the check", async () => {
    const platform = createDarwinPlatform({
      toolExists: () => false,
      runner: () => Promise.reject(new Error("must not run any tool")),
    });
    expect(await platform.verifyTrust("/x")).toEqual({ kind: "not_applicable" });
  });
});
