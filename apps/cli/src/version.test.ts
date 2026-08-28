import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { VERSION } from "./cli.ts";

describe("the reported version", () => {
  it("matches the one package.json publishes", () => {
    const manifest: unknown = JSON.parse(
      readFileSync(new URL("../package.json", import.meta.url), "utf8"),
    );
    expect((manifest as { version: string }).version).toBe(VERSION);
  });
});
