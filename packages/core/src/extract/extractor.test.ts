/** Ports of the extract/mod.rs member-path guard tests — a trust boundary. */

import { describe, expect, it } from "vitest";
import { safeMemberPath } from "./extractor.ts";

describe("safeMemberPath", () => {
  it("rejects traversal entries", () => {
    expect(() => safeMemberPath("../../etc/passwd")).toThrow();
    expect(() => safeMemberPath("/etc/passwd")).toThrow();
    expect(() => safeMemberPath("a/../../b")).toThrow();
    expect(() => safeMemberPath("")).toThrow();
  });

  it("accepts and normalises ordinary entries", () => {
    expect(safeMemberPath("./bin/rg")).toBe("bin/rg");
    expect(safeMemberPath("rg-14/complete/rg.1")).toBe("rg-14/complete/rg.1");
  });

  it("rejects windows style names", () => {
    expect(() => safeMemberPath("C:windows")).toThrow();
    expect(() => safeMemberPath("a\\b")).toThrow();
  });
});
