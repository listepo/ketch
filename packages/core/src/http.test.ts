/**
 * Ports of the http.rs tests, one claim per test.
 *
 * `download` is covered too, against a loopback server rather than a mock: the
 * bug it is guarded against lives in how `fetch` and a real response interact,
 * which nothing short of a real response reproduces. No packet leaves the
 * machine, so the suite stays as offline as the rest of the tree.
 */

import http from "node:http";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import zlib from "node:zlib";
import { describe, expect, it, onTestFinished } from "vitest";
import { Http, extractMessage, sha256File } from "./http.ts";
import { NullProgress } from "./progress.ts";

/** A one-response server on a loopback port, torn down with the test. */
function serve(respond: http.RequestListener): Promise<string> {
  const server = http.createServer(respond);
  onTestFinished(() => {
    server.closeAllConnections();
    server.close();
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = typeof address === "object" && address !== null ? address.port : 0;
      resolve(`http://127.0.0.1:${port}/asset.bin`);
    });
  });
}

function tmpFile(name: string): string {
  return path.join(fs.mkdtempSync(path.join(os.tmpdir(), "ketch-http-")), name);
}

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

describe("download", () => {
  const body = Buffer.from("x".repeat(100_000));

  it("does not mistake a body the server encoded for a short transfer", async () => {
    // ketch asks for `identity`; a server is free to ignore that. `fetch` then
    // decodes the body while `Content-Length` still counts the encoded bytes,
    // and comparing the two deleted a download that had arrived intact.
    const gz = zlib.gzipSync(body);
    const url = await serve((_req, res) => {
      res.writeHead(200, {
        "Content-Type": "application/octet-stream",
        "Content-Encoding": "gzip",
        "Content-Length": String(gz.length),
      });
      res.end(gz);
    });
    const dest = tmpFile("asset.bin");

    await new Http().download(url, dest, {}, false, new NullProgress());

    expect(fs.statSync(dest).size).toBe(body.length);
  });

  it("leaves nothing behind when a transfer is cut off", async () => {
    // The connection dies mid-body, which is the shape a real truncation takes:
    // HTTP will not let a server declare a length and then close short of it.
    const url = await serve((_req, res) => {
      res.writeHead(200, {
        "Content-Type": "application/octet-stream",
        "Content-Length": String(body.length),
      });
      res.write(body.subarray(0, 1000));
      res.socket?.destroy();
    });
    const dest = tmpFile("asset.bin");

    await expect(new Http().download(url, dest, {}, false, new NullProgress())).rejects.toThrow();
    // Not even the staging file: a partial download must not look like a
    // finished one, and must not be left to be found later.
    expect(fs.existsSync(dest)).toBe(false);
    expect(fs.readdirSync(path.dirname(dest))).toEqual([]);
  });
});
