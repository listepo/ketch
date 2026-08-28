/** Ports of the entry-reading tests from the Rust registry.rs. */

import { describe, expect, it } from "vitest";
import { normalizeName, parseRegistryPackage } from "./registry.ts";

describe("parseRegistryPackage", () => {
  it("the folder names the package", () => {
    const m = parseRegistryPackage({ source: "github:BurntSushi/ripgrep" }, "ripgrep", "x");
    expect(m.name).toBe("ripgrep");
    expect(m.source).toBe("github:BurntSushi/ripgrep");
  });

  it("a declared name must match its folder", () => {
    expect(() =>
      parseRegistryPackage({ name: "fzy", source: "github:junegunn/fzf" }, "fzf", "x"),
    ).toThrow("declares name `fzy` but sits in folder `fzf`");
  });

  it("a declared name may differ from the folder only by decoration", () => {
    const m = parseRegistryPackage({ name: "Tool", source: "github:a/tool" }, "tool.rs", "x");
    expect(m.name).toBe("Tool");
  });

  it("a package that would install outside the store is refused", () => {
    expect(() =>
      parseRegistryPackage(
        { name: "evil", source: "github:a/b", bin: [{ name: "../../../.zshrc" }] },
        "evil",
        "x",
      ),
    ).toThrow();
    // A misspelt key is a broken entry, not a silently different package.
    expect(() =>
      parseRegistryPackage({ source: "github:a/b", binary: "x" }, "typo", "x"),
    ).toThrow();
  });
});

describe("normalizeName", () => {
  it("lowercases and strips repo-name decoration", () => {
    expect(normalizeName("RipGrep")).toBe("ripgrep");
    expect(normalizeName("tool.rs")).toBe("tool");
    expect(normalizeName("tool.git")).toBe("tool");
    expect(normalizeName("  fd ")).toBe("fd");
  });
});
