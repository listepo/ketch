/** The built-in registry must always pass the checks other manifests face. */

import { describe, expect, it } from "vitest";
import { builtinPackages } from "./builtin.ts";
import { manifestSchema, validateManifest } from "./manifest.ts";

describe("builtinPackages", () => {
  it("every builtin passes the manifest schema and its validation", () => {
    expect(builtinPackages.length).toBeGreaterThan(0);
    for (const pkg of builtinPackages) {
      expect(() => validateManifest(manifestSchema.parse(pkg))).not.toThrow();
    }
  });

  it("ripgrep answers to rg, as the rust builtin.toml says", () => {
    const ripgrep = builtinPackages.find((p) => p.name === "ripgrep");
    expect(ripgrep?.source).toBe("github:BurntSushi/ripgrep");
    expect(ripgrep?.provides).toEqual(["rg"]);
    expect(ripgrep?.bin).toEqual([{ name: "rg" }]);
  });
});
