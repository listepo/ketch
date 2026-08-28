/** Ports of the shape-relevant state tests from the Rust state.rs and model.rs. */

import { describe, expect, it } from "vitest";
import { parseState, stateSchema } from "./state.ts";

/** state.json exactly as the Rust binary serializes one installed package. */
function rustWrittenState(): Record<string, unknown> {
  return {
    version: 1,
    packages: {
      ripgrep: {
        name: "ripgrep",
        version: "14.1.1",
        source: "github:BurntSushi/ripgrep",
        tag: "14.1.1",
        target: { os: "macos", arch: "aarch64" },
        asset_name: "ripgrep-14.1.1-aarch64-apple-darwin.tar.gz",
        sha256: "a".repeat(64),
        checksum_verified: true,
        installed_at: 1735689600,
        prefix: "/Users/me/.ketch/store/ripgrep/14.1.1",
        links: [
          {
            link: "/Users/me/.ketch/bin/rg",
            target: "/Users/me/.ketch/store/ripgrep/14.1.1/rg",
            kind: "symlink",
          },
        ],
        pinned: false,
        origin: { registry: "/Users/me/.ketch/registry/ripgrep/ketch.toml" },
        manifest: {
          name: "ripgrep",
          source: "github:BurntSushi/ripgrep",
          description: null,
          homepage: null,
          kind: "auto",
          asset: { include: [], exclude: [], target: {} },
          bin: [],
          strip_prefix: null,
          prerelease: false,
          provides: ["rg"],
          notes: null,
          extra_paths: [],
        },
      },
    },
  };
}

describe("stateSchema", () => {
  it("reads a rust-written state file unchanged", () => {
    const state = parseState(rustWrittenState(), "state.json");
    const pkg = state.packages["ripgrep"];
    expect(pkg?.origin).toEqual({ registry: "/Users/me/.ketch/registry/ripgrep/ketch.toml" });
    expect(pkg?.links[0]?.kind).toBe("symlink");
    expect(pkg?.manifest?.provides).toEqual(["rg"]);
  });

  it("refuses a newer state version", () => {
    expect(() => parseState({ version: 99, packages: {} }, "state.json")).toThrow(
      "written by a newer ketch",
    );
  });

  it("missing optional fields take their serde defaults", () => {
    const data = rustWrittenState();
    const packages = data["packages"] as Record<string, Record<string, unknown>>;
    const pkg = packages["ripgrep"] as Record<string, unknown>;
    delete pkg["checksum_verified"];
    delete pkg["links"];
    delete pkg["pinned"];
    delete pkg["manifest"];
    const state = stateSchema.parse(data);
    const parsed = state.packages["ripgrep"];
    expect(parsed?.checksum_verified).toBe(false);
    expect(parsed?.links).toEqual([]);
    expect(parsed?.pinned).toBe(false);
  });

  it("origin unit variants are the strings serde writes", () => {
    const data = rustWrittenState();
    const packages = data["packages"] as Record<string, Record<string, unknown>>;
    const pkg = packages["ripgrep"] as Record<string, unknown>;
    pkg["origin"] = "inferred";
    expect(stateSchema.parse(data).packages["ripgrep"]?.origin).toBe("inferred");
    pkg["origin"] = "surprise";
    expect(() => stateSchema.parse(data)).toThrow();
  });

  it("an empty packages map defaults when absent", () => {
    expect(stateSchema.parse({ version: 1 }).packages).toEqual({});
  });
});
