/**
 * `SourceRegistry` scheme-routing tests.
 *
 * The Rust `source/mod.rs` test module has no cases for `SourceRegistry`
 * itself (`load`/`builtin_only`/`get`/`for_ref`/`schemes`/`all`) — its own
 * tests are all about `pick`, already ported in `source.test.ts`. `get` and
 * `forRef` still carry real branching (case-insensitive match, throw on a
 * miss), so this adds a small, non-ported check for that, built on
 * `LiveSourceRegistry` directly with fake sources rather than through
 * `loadSourceRegistry`, which needs the concurrently-ported GitHub source
 * and real plugin discovery.
 */

import { describe, expect, it } from "vitest";
import { PackageRef } from "../model.ts";
import { LiveSourceRegistry } from "./registry.ts";
import type { Source } from "./source.ts";

function fakeSource(scheme: string): Source {
  return {
    scheme,
    async listReleases() {
      return [];
    },
    async download() {
      return "";
    },
  };
}

describe("LiveSourceRegistry", () => {
  it("finds a source case-insensitively", () => {
    const registry = new LiveSourceRegistry([fakeSource("gitlab")]);
    expect(registry.get("GitLab").scheme).toBe("gitlab");
  });

  it("throws unknown_scheme for a scheme nothing answers to", () => {
    const registry = new LiveSourceRegistry([fakeSource("gitlab")]);
    expect(() => registry.get("bitbucket")).toThrow();
  });

  it("routes forRef by the reference's scheme", () => {
    const registry = new LiveSourceRegistry([fakeSource("gitlab"), fakeSource("gitea")]);
    const reference = PackageRef.tryFrom("gitea:group/project");
    expect(registry.forRef(reference).scheme).toBe("gitea");
  });

  it("lists every registered source", () => {
    const registry = new LiveSourceRegistry([fakeSource("gitlab"), fakeSource("gitea")]);
    expect(registry.schemes()).toEqual(["gitlab", "gitea"]);
    expect(registry.all()).toHaveLength(2);
  });
});
