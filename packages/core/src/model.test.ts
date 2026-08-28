/** Ports of the model.rs unit tests, one claim per test. */

import type { Manifest } from "@ketch/schemas";
import { describe, expect, it } from "vitest";
import {
  globMatch,
  inferredManifest,
  PackageRef,
  PackageSpec,
  Version,
  validateManifest,
} from "./model.ts";

describe("model", () => {
  it("validate refuses names that would escape their directory", () => {
    const base = inferredManifest(PackageRef.github("a/b"));
    expect(() => validateManifest(base)).not.toThrow();

    const badPackage: Manifest = { ...base, name: "../evil" };
    expect(() => validateManifest(badPackage)).toThrow();

    const badLink: Manifest = { ...base, bin: [{ name: "../../.zshrc", path: null }] };
    expect(() => validateManifest(badLink)).toThrow();

    const badPath: Manifest = { ...base, bin: [{ name: "x", path: "../../../x" }] };
    expect(() => validateManifest(badPath)).toThrow();

    const emptyBin: Manifest = { ...base, bin: [{ name: null, path: null }] };
    expect(() => validateManifest(emptyBin)).toThrow();

    const untypeableAlias: Manifest = { ...base, provides: ["two words"] };
    expect(() => validateManifest(untypeableAlias)).toThrow();
  });

  it("parses bare repo as github", () => {
    const r = PackageRef.parse("BurntSushi/ripgrep");
    expect(r).not.toBeNull();
    expect(r?.scheme).toBe("github");
    expect(r?.id).toBe("BurntSushi/ripgrep");
    expect(r?.shortName()).toBe("ripgrep");
  });

  it("bare word is an alias not a ref", () => {
    expect(PackageRef.parse("ripgrep")).toBeNull();
    const spec = PackageSpec.parse("ripgrep");
    expect(spec.alias).toBe("ripgrep");
  });

  it("parses explicit scheme", () => {
    const r = PackageRef.parse("gitlab:group/proj");
    expect(r?.scheme).toBe("gitlab");
    expect(r?.id).toBe("group/proj");
  });

  it("splits version after last slash only", () => {
    const s = PackageSpec.parse("BurntSushi/ripgrep@14.1.0");
    expect(s.reference?.id).toBe("BurntSushi/ripgrep");
    expect(s.version).toEqual({ kind: "exact", value: "14.1.0" });

    const latest = PackageSpec.parse("cli/cli");
    expect(latest.version).toEqual({ kind: "latest" });
  });

  it("versions order by semver then naturally", () => {
    expect(Version.parse("v1.10.0").compare(Version.parse("v1.9.0"))).toBeGreaterThan(0);
    expect(Version.parse("2024.10.1").compare(Version.parse("2024.9.30"))).toBeGreaterThan(0);
    expect(Version.parse("1.0.0").compare(Version.parse("1.0.0-beta.1"))).toBeGreaterThan(0);
    // No digits at all: fall back to natural comparison.
    expect(Version.parse("nightly-b").compare(Version.parse("nightly-a"))).toBeGreaterThan(0);
  });

  it("relaxed versions parse", () => {
    expect(Version.parse("v1.2").sem).not.toBeNull();
    expect(Version.parse("3").sem).not.toBeNull();
    expect(Version.parse("1.2.3.4").sem).not.toBeNull();
  });

  it("versions that differ only past semver still order", () => {
    // Both relax to `1.2.3`, so semver alone calls them equal and a max-scan
    // is free to hand back the older release.
    expect(Version.parse("1.2.3.5").compare(Version.parse("1.2.3.4"))).toBeGreaterThan(0);
    expect(Version.parse("1.2.3.10").compare(Version.parse("1.2.3.9"))).toBeGreaterThan(0);
    const releases = [Version.parse("1.2.3.4"), Version.parse("1.2.3.5"), Version.parse("1.2.3.2")];
    const max = releases.reduce((a, b) => (b.compare(a) > 0 ? b : a));
    expect(max.raw).toBe("1.2.3.5");
  });

  it("an inferred name is always usable as a directory", () => {
    const odd = inferredManifest(PackageRef.github("owner/.."));
    expect(() => validateManifest(odd)).not.toThrow();
  });

  it("strip prefix is bounded", () => {
    const base = inferredManifest(PackageRef.github("a/b"));
    expect(() => validateManifest({ ...base, strip_prefix: 2 })).not.toThrow();
    expect(() => validateManifest({ ...base, strip_prefix: Number.MAX_SAFE_INTEGER })).toThrow();
  });

  it("tag matching ignores v prefix", () => {
    expect(Version.parse("v14.1.0").matchesRequest("14.1.0")).toBe(true);
    expect(Version.parse("14.1.0").matchesRequest("v14.1.0")).toBe(true);
    expect(Version.parse("14.1.0").matchesRequest("14.1.1")).toBe(false);
  });

  it("glob matches asset names", () => {
    expect(globMatch("*-aarch64-apple-darwin.tar.gz", "rg-14-aarch64-apple-darwin.tar.gz")).toBe(
      true,
    );
    expect(globMatch("*.zip", "Tool-Universal.ZIP")).toBe(true);
    expect(globMatch("*.zip", "tool.tar.gz")).toBe(false);
    expect(globMatch("rg?.tar.gz", "rg1.tar.gz")).toBe(true);
    // Pathological pattern must still terminate promptly.
    expect(globMatch("*a*a*a*a*b", "a".repeat(64))).toBe(false);
  });
});
