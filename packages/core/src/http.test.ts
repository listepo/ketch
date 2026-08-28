/** Ports of the http.rs tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { extractMessage, sha256File } from "./http.ts";

describe("extractMessage", () => {
  it("reads github error messages", () => {
    const body = '{"message":"API rate limit exceeded","documentation_url":"https://x"}';
    expect(extractMessage(body)).toBe("API rate limit exceeded");
  });

  it("falls back to a body snippet", () => {
    expect(extractMessage("bad gateway")).toBe("bad gateway");
    expect(extractMessage("   ")).toBeNull();
  });
});

describe("sha256File", () => {
  it("hashes files", async () => {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-http-"));
    const filePath = path.join(dir, "f");
    fs.writeFileSync(filePath, "abc");
    await expect(sha256File(filePath)).resolves.toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });
});
