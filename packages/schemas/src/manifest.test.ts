/** Ports of the manifest tests from the Rust model.rs, plus schema-shape cases. */

import { describe, expect, it } from "vitest";
import { type Manifest, manifestSchema, parseManifest, validateManifest } from "./manifest.ts";

function base(overrides: Partial<Manifest> = {}): Manifest {
  return manifestSchema.parse({ name: "b", source: "github:a/b", ...overrides });
}

describe("validateManifest", () => {
  it("refuses names that would escape their directory", () => {
    expect(() => validateManifest(base())).not.toThrow();

    expect(() => validateManifest(base({ name: "../evil" }))).toThrow();

    expect(() => validateManifest(base({ bin: [{ name: "../../.zshrc", path: null }] }))).toThrow();

    expect(() => validateManifest(base({ bin: [{ name: "x", path: "../../../x" }] }))).toThrow();

    expect(() => validateManifest(base({ bin: [{ name: null, path: null }] }))).toThrow(
      "a `bin` entry needs `name`, `path`, or both",
    );

    expect(() => validateManifest(base({ provides: ["two words"] }))).toThrow();
  });

  it("refuses extra paths that leave the payload", () => {
    expect(() => validateManifest(base({ extra_paths: ["../outside"] }))).toThrow(
      "must stay inside the package",
    );
    expect(() => validateManifest(base({ extra_paths: ["doc/rg.1"] }))).not.toThrow();
  });

  it("strip_prefix is bounded", () => {
    expect(() => validateManifest(base({ strip_prefix: 2 }))).not.toThrow();
    // Built directly rather than parsed: the schema refuses it too, and this
    // proves the guard also holds for manifests constructed in code.
    expect(() => validateManifest({ ...base(), strip_prefix: 99 })).toThrow(
      "`strip_prefix` must be at most 8",
    );
    expect(() => manifestSchema.parse({ name: "x", source: "a/b", strip_prefix: 99 })).toThrow();
  });
});

describe("manifestSchema", () => {
  it("the smallest manifest is a name and a source, everything else defaults", () => {
    const m = parseManifest({ name: "ripgrep", source: "BurntSushi/ripgrep" });
    expect(m.kind).toBe("auto");
    expect(m.bin).toEqual([]);
    expect(m.provides).toEqual([]);
    expect(m.asset).toEqual({ include: [], exclude: [], target: {} });
    expect(m.prerelease).toBe(false);
  });

  it("an unknown key is refused rather than ignored", () => {
    expect(() => manifestSchema.parse({ name: "x", source: "a/b", binary: "x" })).toThrow();
  });

  it("a source that is not a package reference is refused", () => {
    expect(() => manifestSchema.parse({ name: "x", source: "just-a-word" })).toThrow();
  });

  it("accepts the nulls serde writes for absent options", () => {
    const m = manifestSchema.parse({
      name: "x",
      source: "github:a/b",
      description: null,
      homepage: null,
      strip_prefix: null,
      notes: null,
    });
    expect(m.description).toBeNull();
  });

  it("a $schema pointer is allowed at the top level", () => {
    expect(() => parseManifest({ $schema: "https://x", name: "x", source: "a/b" })).not.toThrow();
  });
});
