/** Ports of the guard tests from the Rust config.rs, extract/mod.rs and model.rs. */

import { describe, expect, it } from "vitest";
import { parsePackageRef, safeMemberPath, sanitizeComponent, validateRepo } from "./util.ts";

describe("validateRepo", () => {
  it("only owner/repo is accepted as a repository", () => {
    const want = "listepo/ketch-registry";
    expect(validateRepo("registry", want)).toBe(want);
    expect(validateRepo("registry", "github:listepo/ketch-registry")).toBe(want);
    for (const bad of ["", "listepo", "a/b/c", "../etc", "a/../b", "o/r?x=1", "http://x/y"]) {
      expect(() => validateRepo("registry", bad), `${bad} must be rejected`).toThrow();
    }
  });
});

describe("sanitizeComponent", () => {
  it("sanitizes path components", () => {
    expect(sanitizeComponent("v1.2.3")).toBe("v1.2.3");
    expect(sanitizeComponent("release/1.2")).toBe("release-1.2");
    expect(sanitizeComponent("../../etc")).toBe("etc");
    expect(sanitizeComponent("..")).toBe("unknown");
    expect(sanitizeComponent("")).toBe("unknown");
  });
});

describe("safeMemberPath", () => {
  it("rejects member paths that would escape the destination", () => {
    expect(() => safeMemberPath("../../etc/passwd")).toThrow();
    expect(() => safeMemberPath("/etc/passwd")).toThrow();
    expect(() => safeMemberPath("a/../../b")).toThrow();
    expect(() => safeMemberPath("")).toThrow();
  });

  it("normalizes safe member paths and keeps nested ones", () => {
    expect(safeMemberPath("./bin/rg")).toBe("bin/rg");
    expect(safeMemberPath("rg-14/complete/rg.1")).toBe("rg-14/complete/rg.1");
  });

  it("rejects windows drive and stream syntax", () => {
    expect(() => safeMemberPath("C:windows")).toThrow();
    expect(() => safeMemberPath("a\\b")).toThrow();
  });
});

describe("parsePackageRef", () => {
  it("parses bare repo as github", () => {
    const ref = parsePackageRef("BurntSushi/ripgrep");
    expect(ref).toEqual({ scheme: "github", id: "BurntSushi/ripgrep" });
  });

  it("bare word is an alias, not a ref", () => {
    expect(parsePackageRef("ripgrep")).toBeNull();
  });

  it("parses explicit scheme", () => {
    expect(parsePackageRef("gitlab:group/proj")).toEqual({ scheme: "gitlab", id: "group/proj" });
  });

  it("splits the scheme at the first colon and refuses empty input", () => {
    expect(parsePackageRef("https://host/x")).toEqual({ scheme: "https", id: "//host/x" });
    expect(parsePackageRef("")).toBeNull();
  });
});
