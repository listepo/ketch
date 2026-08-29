/** Port of the platform/mod.rs sidecar test. */

import { describe, expect, it } from "vitest";
import { isSidecar } from "./platform.ts";

describe("isSidecar", () => {
  it("detects sidecars", () => {
    expect(isSidecar("rg-14.tar.gz.sha256")).toBe(true);
    expect(isSidecar("tool.dmg.asc")).toBe(true);
    expect(isSidecar("bundle.intoto.jsonl")).toBe(true);
    expect(isSidecar("rg-14.tar.gz")).toBe(false);
  });
});
