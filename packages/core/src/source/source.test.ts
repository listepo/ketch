/** Ports of the source/mod.rs selection tests, one claim per test. */

import { describe, expect, it } from "vitest";
import { type Release, Version } from "../model.ts";
import { defaultListOpts, pick } from "./source.ts";

function release(tag: string, prerelease: boolean): Release {
  return {
    version: Version.parse(tag),
    tag,
    prerelease,
    draft: false,
    published_at: null,
    notes: null,
    assets: [],
  };
}

describe("release selection", () => {
  it("latest prefers highest stable", () => {
    const releases = [
      release("v1.2.0", false),
      release("v2.0.0-rc.1", true),
      release("v1.10.0", false),
    ];
    const got = pick("x", releases, { kind: "latest" }, defaultListOpts());
    expect(got.tag).toBe("v1.10.0");
  });

  it("latest uses prerelease when asked", () => {
    const releases = [release("v1.2.0", false), release("v2.0.0-rc.1", true)];
    const opts = { ...defaultListOpts(), includePrerelease: true };
    const got = pick("x", releases, { kind: "latest" }, opts);
    expect(got.tag).toBe("v2.0.0-rc.1");
  });

  it("latest falls back to prerelease when no stable exists", () => {
    const releases = [release("v0.1.0-alpha", true)];
    const got = pick("x", releases, { kind: "latest" }, defaultListOpts());
    expect(got.tag).toBe("v0.1.0-alpha");
  });

  it("exact matches with or without v prefix", () => {
    const releases = [release("v1.2.0", false)];
    const got = pick("x", releases, { kind: "exact", value: "1.2.0" }, defaultListOpts());
    expect(got.tag).toBe("v1.2.0");
    expect(() =>
      pick("x", releases, { kind: "exact", value: "9.9.9" }, defaultListOpts()),
    ).toThrow();
  });

  it("drafts are never selected", () => {
    const draft = release("v3.0.0", false);
    draft.draft = true;
    const releases = [draft, release("v1.0.0", false)];
    const got = pick("x", releases, { kind: "latest" }, defaultListOpts());
    expect(got.tag).toBe("v1.0.0");
  });
});
