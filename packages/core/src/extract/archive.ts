/**
 * Portable archive formats.
 *
 * Detection is by magic bytes, never by extension. Every entry path goes
 * through `safeMemberPath` before it is joined onto the destination, and every
 * write goes through `walkInside`, so a member cannot reach outside `dest`
 * either by name or by following a link an earlier member planted.
 *
 * Decompression is to memory and filesystem work is synchronous, mirroring the
 * Rust original: extraction is a short, CPU-bound step between two awaits, and
 * an async guard would have to be re-proved at every call site for no gain.
 * Only `extract` itself is async, because the interface is.
 */

import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import * as zlib from "node:zlib";
import { Parser, type ReadEntry } from "tar";
import { KetchError } from "../errors.ts";
import { type Extractor, safeMemberPath } from "./extractor.ts";

const GZIP_MAGIC = Uint8Array.of(0x1f, 0x8b);
const XZ_MAGIC = Uint8Array.of(0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00);
const BZ2_MAGIC = Uint8Array.of(0x42, 0x5a, 0x68); // "BZh"
const ZIP_MAGIC = Uint8Array.of(0x50, 0x4b, 0x03, 0x04); // "PK\x03\x04"

/** How much of a gzip stream to inflate when peeking for a tar header. */
const PEEK_INPUT = 64 * 1024;

function startsWith(head: Uint8Array, magic: Uint8Array): boolean {
  if (head.length < magic.length) {
    return false;
  }
  return magic.every((byte, index) => head[index] === byte);
}

/** The `ustar` marker tar writes at offset 257 of every header block. */
function looksLikeTar(head: Uint8Array): boolean {
  return head.length >= 262 && Buffer.from(head.subarray(257, 262)).toString("latin1") === "ustar";
}

/** First `want` bytes of a file, without reading the rest of it. */
function readPrefix(file: string, want: number): Buffer {
  let fd: number | null = null;
  try {
    fd = fs.openSync(file, "r");
    const buffer = Buffer.alloc(want);
    const filled = fs.readSync(fd, buffer, 0, want, 0);
    return buffer.subarray(0, filled);
  } catch {
    return Buffer.alloc(0);
  } finally {
    if (fd !== null) {
      fs.closeSync(fd);
    }
  }
}

/**
 * Inflate just enough of a gzip stream to see whether a tar header follows.
 *
 * This is what separates `tool.tar.gz` from `tool.gz` when the publisher named
 * the file wrongly, which happens often enough to matter. `Z_SYNC_FLUSH` lets
 * the truncated input end mid-stream without being an error; 64 KiB in yields
 * far more than the 262 bytes the answer needs, unless the whole file is
 * smaller — in which case it is inflated whole and the answer is exact.
 */
function gzipInnerHead(file: string): Uint8Array {
  try {
    return zlib.gunzipSync(readPrefix(file, PEEK_INPUT), {
      finishFlush: zlib.constants.Z_SYNC_FLUSH,
    });
  } catch {
    return Buffer.alloc(0);
  }
}

/**
 * A link inside the payload may only point at something else inside it.
 *
 * Checked lexically, before the link exists: a symlink to `../../../.ssh`
 * would otherwise turn the next `place` into an arbitrary-file copy.
 */
export function checkLinkTarget(member: string, target: string): void {
  const reject = (): never => {
    throw KetchError.msg(
      `refusing archive link ${member} -> ${target} that escapes the target directory`,
    );
  };
  if (target.startsWith("/")) {
    reject();
  }
  // Depth of the directory holding the link, relative to the payload root.
  let depth = member.split("/").filter((part) => part !== "" && part !== ".").length - 1;
  for (const part of target.split("/")) {
    if (part === "" || part === ".") {
      continue;
    }
    if (part === "..") {
      depth -= 1;
      if (depth < 0) {
        reject();
      }
      continue;
    }
    depth += 1;
  }
}

/**
 * Walk every component of `target` below `dest`, refusing any that is a symlink.
 *
 * This is what makes `checkLinkTarget` sound. That guard counts depth from the
 * member *name*, but `mkdir -p` and every write follow links, so a member
 * called `a/link/b` lands wherever `link` points once an earlier member of the
 * same archive planted it — and the surplus `..` in a later target then escapes
 * `dest`. Refusing to traverse any link below `dest` keeps the path we counted
 * and the path the kernel resolves the same one, for files, directories,
 * symlinks and hard links alike.
 *
 * `dest` itself is ketch's own directory and is trusted: on macOS it usually
 * *is* reached through a symlink (`/tmp` -> `private/tmp`).
 */
export function walkInside(dest: string, target: string, create: boolean): void {
  const rel = path.relative(dest, target);
  if (path.isAbsolute(rel) || rel === ".." || rel.startsWith(`..${path.sep}`)) {
    throw KetchError.msg(`refusing archive entry ${target} outside ${dest}`);
  }
  let built = dest;
  for (const part of rel.split(path.sep)) {
    if (part === "") {
      continue;
    }
    built = path.join(built, part);
    let meta: fs.Stats | null;
    try {
      meta = fs.lstatSync(built);
    } catch {
      meta = null;
    }
    if (meta === null) {
      if (!create) {
        // Nothing exists from here down, so nothing below can be a link.
        break;
      }
      try {
        fs.mkdirSync(built);
      } catch (cause) {
        throw KetchError.io(built, cause as Error);
      }
      continue;
    }
    if (meta.isSymbolicLink()) {
      throw KetchError.msg(`refusing archive entry that resolves through ${built}`);
    }
    // An ordinary directory, or a file at the leaf: both are fine.
  }
}

/**
 * Make `out` safe to write: its directories exist, none of them is a link,
 * and `out` itself is not a link an earlier member planted for us to write
 * through (an unguarded create would happily truncate the far end).
 */
export function ensureParent(dest: string, out: string): void {
  walkInside(dest, path.dirname(out), true);
  try {
    if (fs.lstatSync(out).isSymbolicLink()) {
      fs.rmSync(out);
    }
  } catch {
    // Nothing there, which is the common case and needs no clearing.
  }
}

/** One member of a tar stream, already read into memory. */
interface TarMember {
  readonly name: string;
  readonly type: string;
  readonly mode: number;
  readonly linkpath: string | null;
  readonly body: Buffer;
}

/**
 * Read every member of a tar stream.
 *
 * Parsing is the library's job; placing the results is ours, which is why the
 * members come back as data rather than being written where they fall.
 */
function readTar(archive: Buffer, what: string): TarMember[] {
  const members: TarMember[] = [];
  let failure: Error | null = null;

  const parser = new Parser({});
  parser.on("error", (cause: Error) => {
    failure ??= cause;
  });
  parser.on("entry", (entry: ReadEntry) => {
    const chunks: Buffer[] = [];
    entry.on("data", (chunk: Buffer) => chunks.push(chunk));
    entry.on("end", () => {
      members.push({
        name: String(entry.path),
        type: String(entry.type),
        mode: entry.mode ?? 0o644,
        linkpath: entry.linkpath ?? null,
        body: Buffer.concat(chunks),
      });
    });
    entry.resume();
  });

  try {
    parser.write(archive);
    parser.end();
  } catch (cause) {
    failure ??= cause as Error;
  }
  if (failure !== null) {
    throw KetchError.parse(what, failure.message);
  }
  return members;
}

/** Extended headers carry metadata, not payload, and have no real path. */
const METADATA_TYPES = new Set([
  "ExtendedHeader",
  "GlobalExtendedHeader",
  "NextFileHasLongLinkpath",
  "NextFileHasLongPath",
  "OldGnuLongPath",
]);

/**
 * Unpack a tar stream, validating every member path and link target.
 *
 * Written out rather than handing the destination to the tar library, so the
 * traversal guard is ours and applies identically to files, directories and
 * links.
 */
export function unpackTar(archive: Buffer, dest: string, what: string): void {
  for (const member of readTar(archive, what)) {
    if (METADATA_TYPES.has(member.type)) {
      continue;
    }
    const safe = safeMemberPath(member.name);
    const out = path.join(dest, safe);

    switch (member.type) {
      case "Directory":
      case "GNUDumpDir": {
        walkInside(dest, out, true);
        break;
      }
      case "SymbolicLink": {
        if (member.linkpath === null) {
          throw KetchError.msg(`archive symlink ${member.name} has no target`);
        }
        checkLinkTarget(safe, member.linkpath);
        ensureParent(dest, out);
        try {
          fs.symlinkSync(member.linkpath, out);
        } catch (cause) {
          throw KetchError.io(out, cause as Error);
        }
        break;
      }
      case "Link": {
        // A tar hard link names its target from the archive root.
        if (member.linkpath === null) {
          throw KetchError.msg(`archive hard link ${member.name} has no target`);
        }
        // The kernel resolves the source too, so a hard link to
        // `planted-link/.ssh/id_rsa` would pull a file from outside the
        // payload into it.
        const source = path.join(dest, safeMemberPath(member.linkpath));
        walkInside(dest, source, false);
        ensureParent(dest, out);
        try {
          fs.linkSync(source, out);
        } catch (cause) {
          throw KetchError.io(out, cause as Error);
        }
        break;
      }
      case "File":
      case "OldFile":
      case "ContiguousFile": {
        ensureParent(dest, out);
        try {
          fs.writeFileSync(out, member.body);
          fs.chmodSync(out, member.mode & 0o7777);
        } catch (cause) {
          throw KetchError.io(out, cause as Error);
        }
        break;
      }
      // Character/block devices and fifos have no place in a release.
      default:
        break;
    }
  }
}

function readWhole(file: string): Buffer {
  try {
    return fs.readFileSync(file);
  } catch (cause) {
    throw KetchError.io(file, cause as Error);
  }
}

function writeProgram(out: string, body: Buffer): void {
  try {
    fs.writeFileSync(out, body);
    // A lone compressed file in a release is a program; nothing else is
    // published this way.
    fs.chmodSync(out, 0o755);
  } catch (cause) {
    throw KetchError.io(out, cause as Error);
  }
}

/** `.tar.gz` / `.tgz` */
export class TarGzExtractor implements Extractor {
  readonly id = "tar.gz";

  detect(file: string, head: Uint8Array): boolean {
    return startsWith(head, GZIP_MAGIC) && looksLikeTar(gzipInnerHead(file));
  }

  extract(src: string, dest: string): Promise<void> {
    unpackTar(zlib.gunzipSync(readWhole(src)), dest, src);
    return Promise.resolve();
  }
}

/**
 * A ceiling on the decompressed size of an xz archive. xz expands by orders of
 * magnitude, so an unbounded decode is a bomb the downloader cannot see coming;
 * a real release, `.app` bundles included, is far below this.
 */
const XZ_MAX_BYTES = 1024 * 1024 * 1024;

/**
 * The plain tar stream inside an xz archive.
 *
 * Every JavaScript xz decoder published to npm is a WebAssembly build, and the
 * compiler that produces the released binary ships no WebAssembly host — one in
 * the module graph costs the release path entirely. macOS `tar` is libarchive
 * linked against liblzma, and `-c @archive` reads an archive and rewrites its
 * entries rather than extracting them, which is a decompressor the OS already
 * has. It is spawned by absolute path so a `tar` earlier on `PATH` cannot stand
 * in for it. Names, modes and link targets survive verbatim, so `unpackTar`
 * still sees — and still rejects — a member that tries to escape.
 *
 * A non-macOS platform needs its own decoder here: this is GNU tar syntax for
 * something else entirely.
 */
function unxz(src: string): Buffer {
  const result = spawnSync("/usr/bin/tar", ["-cf", "-", "--format=pax", `@${path.resolve(src)}`], {
    maxBuffer: XZ_MAX_BYTES,
  });
  if (result.error !== undefined) {
    throw KetchError.parse(src, `xz could not be decompressed: ${result.error.message}`);
  }
  if (result.status !== 0) {
    const detail = result.stderr.toString("utf8").trim();
    throw KetchError.parse(src, detail === "" ? "not a valid xz archive" : detail);
  }
  return result.stdout;
}

/** `.tar.xz` / `.txz` */
export class TarXzExtractor implements Extractor {
  readonly id = "tar.xz";

  detect(_file: string, head: Uint8Array): boolean {
    return startsWith(head, XZ_MAGIC);
  }

  extract(src: string, dest: string): Promise<void> {
    unpackTar(unxz(src), dest, src);
    return Promise.resolve();
  }
}

/** `.tar.bz2` */
export class TarBz2Extractor implements Extractor {
  readonly id = "tar.bz2";

  detect(_file: string, head: Uint8Array): boolean {
    return startsWith(head, BZ2_MAGIC);
  }

  async extract(src: string, dest: string): Promise<void> {
    // seek-bzip is CommonJS with a default export; the dynamic import keeps
    // that interop local instead of shaping the whole module's imports.
    const { default: bzip } = await import("seek-bzip");
    let plain: Buffer;
    try {
      plain = bzip.decode(readWhole(src));
    } catch (cause) {
      throw KetchError.parse(src, (cause as Error).message);
    }
    unpackTar(plain, dest, src);
  }
}

/** Uncompressed `.tar` */
export class TarExtractor implements Extractor {
  readonly id = "tar";

  detect(_file: string, head: Uint8Array): boolean {
    return looksLikeTar(head);
  }

  extract(src: string, dest: string): Promise<void> {
    unpackTar(readWhole(src), dest, src);
    return Promise.resolve();
  }
}

/** `.zip` */
export class ZipExtractor implements Extractor {
  readonly id = "zip";

  detect(_file: string, head: Uint8Array): boolean {
    return startsWith(head, ZIP_MAGIC);
  }

  async extract(src: string, dest: string): Promise<void> {
    for (const entry of await readZip(src)) {
      // Finder metadata, not payload.
      if (entry.name.startsWith("__MACOSX/") || entry.name.endsWith("/.DS_Store")) {
        continue;
      }
      const safe = safeMemberPath(entry.name);
      const out = path.join(dest, safe);

      if (entry.name.endsWith("/")) {
        walkInside(dest, out, true);
        continue;
      }
      ensureParent(dest, out);

      // Zip stores a symlink as a regular member whose body is the target.
      if ((entry.mode & 0o170000) === 0o120000) {
        const target = entry.body.toString("utf8").trim();
        checkLinkTarget(safe, target);
        try {
          fs.symlinkSync(target, out);
        } catch (cause) {
          throw KetchError.io(out, cause as Error);
        }
        continue;
      }

      try {
        fs.writeFileSync(out, entry.body);
        // Without this, every binary in a zip lands non-executable.
        if (entry.mode !== 0) {
          fs.chmodSync(out, entry.mode & 0o7777);
        }
      } catch (cause) {
        throw KetchError.io(out, cause as Error);
      }
    }
  }
}

/** One member of a zip, with the unix mode the publisher recorded. */
interface ZipMember {
  readonly name: string;
  readonly mode: number;
  readonly body: Buffer;
}

/**
 * Read every member of a zip into memory.
 *
 * The unix mode lives in the upper half of the DOS "external file attributes"
 * field. It is what carries the executable bit and marks symlinks, so a reader
 * that hides it cannot extract a release archive correctly.
 */
async function readZip(src: string): Promise<ZipMember[]> {
  const yauzl = await import("yauzl");
  const zip = await yauzl.fromBufferPromise(readWhole(src), { lazyEntries: true });
  const members: ZipMember[] = [];
  try {
    for (;;) {
      const entry = await nextZipEntry(zip);
      if (entry === null) {
        break;
      }
      members.push({
        name: entry.fileName,
        mode: entry.externalFileAttributes >>> 16,
        body: entry.fileName.endsWith("/") ? Buffer.alloc(0) : await readZipEntry(zip, entry),
      });
    }
  } catch (cause) {
    throw KetchError.parse(src, (cause as Error).message);
  } finally {
    zip.close();
  }
  return members;
}

function nextZipEntry(zip: import("yauzl").ZipFile): Promise<import("yauzl").Entry | null> {
  return new Promise((resolve, reject) => {
    zip.once("entry", resolve);
    zip.once("end", () => resolve(null));
    zip.once("error", reject);
    zip.readEntry();
  });
}

function readZipEntry(zip: import("yauzl").ZipFile, entry: import("yauzl").Entry): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    zip.openReadStream(entry, (err, stream) => {
      if (err !== null || stream === undefined) {
        reject(err ?? new Error(`could not read ${entry.fileName}`));
        return;
      }
      const chunks: Buffer[] = [];
      stream.on("data", (chunk: Buffer) => chunks.push(chunk));
      stream.on("error", reject);
      stream.on("end", () => resolve(Buffer.concat(chunks)));
    });
  });
}

/** `.gz` wrapping a single file rather than a tar stream. */
export class GzFileExtractor implements Extractor {
  readonly id = "gz";

  detect(file: string, head: Uint8Array): boolean {
    return startsWith(head, GZIP_MAGIC) && !looksLikeTar(gzipInnerHead(file));
  }

  extract(src: string, dest: string): Promise<void> {
    const base = path.basename(src);
    const name = base === "" ? "payload" : base;
    // A file named just `.gz` strips to nothing and is refused by the member
    // guard, exactly as a nameless archive entry would be.
    const stem = name.endsWith(".gz") ? name.slice(0, -".gz".length) : name;
    const out = path.join(dest, safeMemberPath(stem));
    writeProgram(out, zlib.gunzipSync(readWhole(src)));
    return Promise.resolve();
  }
}

/**
 * A bare executable published with no container at all.
 *
 * Must be last in the extractor list: it accepts anything the others refused.
 */
export class RawBinaryExtractor implements Extractor {
  readonly id = "raw";

  detect(_file: string, _head: Uint8Array): boolean {
    return true;
  }

  extract(src: string, dest: string): Promise<void> {
    const name = path.basename(src);
    const out = path.join(dest, safeMemberPath(name === "" ? "payload" : name));
    writeProgram(out, readWhole(src));
    return Promise.resolve();
  }
}

/** Mach-O 32/64 in both byte orders, plus the fat/universal wrappers. */
const MACH_O_MAGICS = new Set([
  0xfeed_face, 0xcefa_edfe, 0xfeed_facf, 0xcffa_edfe, 0xcafe_babe, 0xbeba_feca,
]);

/**
 * True when the first bytes are a Mach-O image, an ELF image, or a `#!` line.
 *
 * Used to tell "a program" from "a README that happens to be marked +x".
 */
export function isProgramHead(head: Uint8Array): boolean {
  if (startsWith(head, Uint8Array.of(0x23, 0x21))) {
    return true;
  }
  if (startsWith(head, Uint8Array.of(0x7f, 0x45, 0x4c, 0x46))) {
    return true;
  }
  if (head.length < 4) {
    return false;
  }
  const magic = Buffer.from(head.subarray(0, 4)).readUInt32BE(0);
  return MACH_O_MAGICS.has(magic);
}
