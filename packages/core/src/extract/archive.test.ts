/**
 * Ports of the extract/archive.rs unit tests — the traversal guards, format
 * detection, and mode handling. Fixture archives are built in code with
 * node-tar's `Header`, so each test states exactly the bytes it feeds in.
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as zlib from "node:zlib";
import { Header } from "tar";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  checkLinkTarget,
  GzFileExtractor,
  isProgramHead,
  TarBz2Extractor,
  TarGzExtractor,
  unpackTar,
} from "./archive.ts";
import { readHead } from "./index.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-archive-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

/** One 512-byte tar header block. */
function tarBlock(data: {
  path: string;
  type: "File" | "Directory" | "SymbolicLink" | "Link";
  mode: number;
  size?: number;
  linkpath?: string;
}): Buffer {
  const buf = Buffer.alloc(512);
  new Header({ mtime: new Date(0), size: 0, ...data }).encode(buf, 0);
  return buf;
}

function symlinkEntry(name: string, target: string): Buffer {
  return tarBlock({ path: name, type: "SymbolicLink", linkpath: target, mode: 0o777 });
}

function fileEntry(name: string, body: string, mode: number): Buffer {
  const raw = Buffer.from(body);
  const padded = Buffer.alloc(Math.ceil(raw.length / 512) * 512);
  raw.copy(padded);
  return Buffer.concat([tarBlock({ path: name, type: "File", mode, size: raw.length }), padded]);
}

/** Entries plus the two zero blocks that close every tar stream. */
function tarWith(entries: readonly Buffer[]): Buffer {
  return Buffer.concat([...entries, Buffer.alloc(1024)]);
}

/** Latin-1 keeps every char a single byte, so `\x7f` escapes stay literal. */
function bytes(text: string): Uint8Array {
  return Buffer.from(text, "latin1");
}

function tarGzWith(entries: ReadonlyArray<readonly [string, string, number]>): Buffer {
  return zlib.gzipSync(tarWith(entries.map(([name, body, mode]) => fileEntry(name, body, mode))));
}

/**
 * The planted-symlink archive used by the traversal tests:
 *
 * 1. A link back to the payload root. Lexical depth 4, four `..` land on 0,
 *    so the target guard accepts it.
 * 2. A link named *through* member 1, so its real depth is four rather than
 *    the nine its name claims — and the surplus `..` escapes.
 * 3. The payload, written through member 2.
 */
function plantedSymlinkTar(): Buffer {
  return tarWith([
    symlinkEntry("a/b/c/d/link", "../../../.."),
    symlinkEntry("a/b/c/d/link/e/f/g/h/esc", "../../../../../../../../.."),
    fileEntry("a/b/c/d/link/e/f/g/h/esc/victim", "pwned", 0o644),
  ]);
}

/**
 * `plantedSymlinkTar()` compressed with `bzip2 --best`, embedded because
 * seek-bzip only decodes and nothing else in the tree can write bzip2. To
 * regenerate: build the tar above (mtime 0, uid/gid 0) and pipe it through
 * `bzip2 --best --stdout | base64`.
 */
const PLANTED_TAR_BZ2 = Buffer.from(
  [
    "QlpoOTFBWSZTWWqPi0UAAKNfkNMAQAH3hCFABgB/71+ABAAACDAAuEVNGjQDJkNGJk0DTBgBk00G",
    "QwQ0xGjAiigU9QGm9RNoTaajDSepc5XYc5rveGieQIVdWASUKoQsEPN94dVCR+NcBVT2Na0gIkU4",
    "v20aGRiczI3NtHZAhDAIQ8GAeOONmqCLGQNsmGBG3lO66CJkktE0M3e5wx1+qk7wB/W5tbW26Q34",
    "t5o+jv8eIXRymACQu5IpwoSDVHxaKA==",
  ].join(""),
  "base64",
);

describe("archive extraction", () => {
  /**
   * The escape this guards against is not hypothetical: every member name in
   * the planted archive passes `safeMemberPath` and `checkLinkTarget`, because
   * both reason about the name while the kernel resolves the link.
   */
  it("a planted symlink is never written through", () => {
    const dest = path.join(dir, "dest");
    fs.mkdirSync(dest, { recursive: true });

    // an entry named through a planted symlink must be refused
    expect(() => unpackTar(plantedSymlinkTar(), dest, "planted.tar")).toThrow();
    // Member 2 would have been created here had the link been followed.
    expect(fs.existsSync(path.join(dest, "e"))).toBe(false);
  });

  it("ordinary symlinks inside the payload still work", () => {
    const archive = tarWith([
      fileEntry("bin/tool", "hi", 0o755),
      symlinkEntry("bin/tool-alias", "tool"),
    ]);

    unpackTar(archive, dir, "links.tar");
    const alias = path.join(dir, "bin/tool-alias");
    expect(fs.lstatSync(alias).isSymbolicLink()).toBe(true);
    expect(fs.readFileSync(alias, "utf8")).toBe("hi");
  });

  it("extracts a tar gz and keeps the executable bit", async () => {
    const src = path.join(dir, "t.tar.gz");
    fs.writeFileSync(src, tarGzWith([["rg-1/rg", "#!/bin/sh\n", 0o755]]));
    const dest = path.join(dir, "out");
    fs.mkdirSync(dest, { recursive: true });

    await new TarGzExtractor().extract(src, dest);
    const binary = path.join(dest, "rg-1/rg");
    expect(fs.statSync(binary).isFile()).toBe(true);
    expect(fs.statSync(binary).mode & 0o111).not.toBe(0);
  });

  it("bzip2 archives use the same traversal guard as tar", async () => {
    const src = path.join(dir, "payload.tar.bz2");
    fs.writeFileSync(src, PLANTED_TAR_BZ2);
    const dest = path.join(dir, "dest");
    fs.mkdirSync(dest, { recursive: true });

    await expect(new TarBz2Extractor().extract(src, dest)).rejects.toThrow();
    // Member 1 landing proves the refusal came from the guard, not from a
    // fixture that failed to decode.
    expect(fs.lstatSync(path.join(dest, "a/b/c/d/link")).isSymbolicLink()).toBe(true);
    expect(fs.existsSync(path.join(dir, "e"))).toBe(false);
  });

  it("detection separates tar gz from a lone gz", () => {
    const tarball = path.join(dir, "a.gz");
    fs.writeFileSync(tarball, tarGzWith([["x", "y", 0o644]]));
    const tarballHead = readHead(tarball);
    expect(new TarGzExtractor().detect(tarball, tarballHead)).toBe(true);
    expect(new GzFileExtractor().detect(tarball, tarballHead)).toBe(false);

    const plain = path.join(dir, "jq.gz");
    fs.writeFileSync(plain, zlib.gzipSync(Buffer.from("\x7fELF and then some payload", "latin1")));
    const plainHead = readHead(plain);
    expect(new TarGzExtractor().detect(plain, plainHead)).toBe(false);
    expect(new GzFileExtractor().detect(plain, plainHead)).toBe(true);
  });

  it("refuses links that escape the payload", () => {
    expect(() => checkLinkTarget("bin/tool", "../lib/x.dylib")).not.toThrow();
    expect(() => checkLinkTarget("bin/tool", "../../../.ssh/id")).toThrow();
    expect(() => checkLinkTarget("tool", "/etc/passwd")).toThrow();
    expect(() => checkLinkTarget("tool", "../outside")).toThrow();
  });

  it("recognises program headers", () => {
    expect(isProgramHead(bytes("#!/bin/sh"))).toBe(true);
    expect(isProgramHead(Uint8Array.of(0xcf, 0xfa, 0xed, 0xfe, 0, 0))).toBe(true);
    expect(isProgramHead(bytes("\x7fELF\x02"))).toBe(true);
    expect(isProgramHead(bytes("# Readme\n"))).toBe(false);
    expect(isProgramHead(bytes(""))).toBe(false);
  });
});
