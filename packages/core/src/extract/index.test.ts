/**
 * Ports of the extract/mod.rs payload-unwrapping tests. The member-path guard
 * tests from the same mod live in extractor.test.ts, beside the re-export.
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { unwrapSingleDir } from "./index.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-unwrap-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

describe("unwrapSingleDir", () => {
  it("a lone bundle is the payload and is not unwrapped", () => {
    fs.mkdirSync(path.join(dir, "TestApp.app/Contents/MacOS"), { recursive: true });

    expect(unwrapSingleDir(dir)).toBe(dir);
  });

  it("a lone plain directory is still unwrapped", () => {
    const inner = path.join(dir, "tool-1.0.0");
    fs.mkdirSync(path.join(inner, "bin"), { recursive: true });

    expect(unwrapSingleDir(dir)).toBe(inner);
  });

  it("unwraps a single wrapper directory", () => {
    const inner = path.join(dir, "tool-1.2.3");
    fs.mkdirSync(path.join(inner, "bin"), { recursive: true });
    fs.writeFileSync(path.join(dir, ".DS_Store"), "x");

    expect(unwrapSingleDir(dir)).toBe(inner);
  });

  it("keeps root when payload is flat", () => {
    fs.writeFileSync(path.join(dir, "a"), "x");
    fs.writeFileSync(path.join(dir, "b"), "x");

    expect(unwrapSingleDir(dir)).toBe(dir);
  });
});
