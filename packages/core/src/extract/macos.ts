/**
 * macOS container formats.
 *
 * Both shell out to the system tools, because reimplementing HFS+/APFS image
 * reading or Apple's flat-package format would be a large amount of code with
 * no upside — `hdiutil` and `pkgutil` ship on every Mac.
 */

import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { KetchError } from "../errors.ts";
import type { Extractor } from "./extractor.ts";

/**
 * Run a system tool and return its stdout.
 *
 * stdin is closed so a tool that decides to prompt (a DMG with a licence
 * agreement, say) fails fast instead of hanging the install forever.
 */
export function runTool(program: string, args: readonly string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(program, [...args], { stdio: ["ignore", "pipe", "pipe"] });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", (cause) => reject(KetchError.io(program, cause)));
    child.on("close", (code) => {
      if (code === 0) {
        resolve(Buffer.concat(stdout).toString("utf8"));
        return;
      }
      reject(
        new KetchError({
          kind: "command",
          cmd: `${program} ${args.join(" ")}`,
          status: code === null ? "killed by signal" : `exit ${code}`,
          stderr: Buffer.concat(stderr).toString("utf8").trim(),
        }),
      );
    });
  });
}

/**
 * Same, but a non-zero exit is reported rather than fatal. Used for the
 * best-effort cleanup paths where failing would lose the real error.
 */
export function tryTool(program: string, args: readonly string[]): Promise<boolean> {
  return new Promise((resolve) => {
    const child = spawn(program, [...args], { stdio: "ignore" });
    child.on("error", () => resolve(false));
    child.on("close", (code) => resolve(code === 0));
  });
}

/**
 * `ditto` rather than a hand-rolled copy: it preserves extended attributes,
 * symlinks and resource forks, and a `.app` whose code signature is broken by
 * a naive copy will refuse to launch.
 */
export async function copyTree(src: string, dest: string): Promise<void> {
  await runTool("/usr/bin/ditto", [src, dest]);
}

/** The 512-byte `koly` trailer that closes every Apple disk image. */
function hasKolyTrailer(file: string): boolean {
  let fd: number | null = null;
  try {
    const size = fs.statSync(file).size;
    if (size < 512) {
      return false;
    }
    fd = fs.openSync(file, "r");
    const magic = Buffer.alloc(4);
    const filled = fs.readSync(fd, magic, 0, 4, size - 512);
    return filled === 4 && magic.toString("latin1") === "koly";
  } catch {
    return false;
  } finally {
    if (fd !== null) {
      fs.closeSync(fd);
    }
  }
}

function hasExtension(file: string, want: readonly string[]): boolean {
  return want.includes(path.extname(file).replace(/^\./, "").toLowerCase());
}

/** A throwaway directory that is removed however the body ends. */
async function withTempDir<T>(run: (dir: string) => Promise<T>): Promise<T> {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-extract-"));
  try {
    return await run(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/**
 * `.dmg` — attached read-only with no browse and no auto-open, contents
 * copied out, then detached even if the copy failed.
 */
export class DmgExtractor implements Extractor {
  readonly id = "dmg";

  detect(file: string, _head: Uint8Array): boolean {
    return hasKolyTrailer(file) || hasExtension(file, ["dmg"]);
  }

  extract(src: string, dest: string): Promise<void> {
    return withTempDir(async (mountRoot) => {
      const listing = await runTool("/usr/bin/hdiutil", [
        "attach",
        src,
        "-nobrowse",
        "-noautoopen",
        "-readonly",
        "-noverify",
        "-mountrandom",
        mountRoot,
      ]);

      const mount = findMountPoint(listing);
      if (mount === null) {
        // The image is attached even though nothing mounted, and the mount
        // point is the handle we would normally detach by. The device node is
        // the only one left; without it the image stays attached for the rest
        // of the session with nothing pointing at it.
        const device = findDevice(listing);
        if (device !== null) {
          await tryTool("/usr/bin/hdiutil", ["detach", device, "-force"]);
        }
        throw KetchError.msg(`hdiutil attached ${src} but reported no mount point`);
      }

      // The detach must run whether or not the copy worked, or the image stays
      // attached for the rest of the session.
      try {
        await copyVolume(mount, dest);
      } finally {
        await tryTool("/usr/bin/hdiutil", ["detach", mount, "-force"]);
      }
    });
  }
}

/**
 * Pull the mount point out of `hdiutil attach` output.
 *
 * Lines are tab-separated `device \t type \t mountpoint`, and only some
 * partitions are mounted at all, so the mount point is the last field of a
 * line — and the volume is the first such field that is a directory.
 */
export function findMountPoint(listing: string): string | null {
  for (const line of listing.split("\n")) {
    const fields = line.split("\t");
    const last = (fields[fields.length - 1] ?? "").trim();
    if (!last.startsWith("/")) {
      continue;
    }
    try {
      if (fs.statSync(last).isDirectory()) {
        return last;
      }
    } catch {
      // Not a directory that exists, so not the volume.
    }
  }
  return null;
}

/**
 * The device node `hdiutil attach` created, from the first line that names one.
 *
 * Detaching by device takes the whole image down, partitions included, which
 * is exactly what is wanted when none of them mounted.
 */
export function findDevice(listing: string): string | null {
  for (const line of listing.split("\n")) {
    const first = (line.split("\t")[0] ?? "").trim();
    if (first.startsWith("/dev/")) {
      return first;
    }
  }
  return null;
}

async function copyVolume(mount: string, dest: string): Promise<void> {
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(mount, { withFileTypes: true });
  } catch (cause) {
    throw KetchError.io(mount, cause as Error);
  }

  let copiedAny = false;
  for (const entry of entries) {
    // Volume bookkeeping and the customary `/Applications` drop-target.
    if (entry.name.startsWith(".") || entry.isSymbolicLink()) {
      continue;
    }
    await copyTree(path.join(mount, entry.name), path.join(dest, entry.name));
    copiedAny = true;
  }

  if (!copiedAny) {
    throw new KetchError({ kind: "empty_payload", path: mount });
  }
}

/**
 * `.pkg` / `.mpkg` — expanded and unpacked into the store rather than
 * installed system-wide, so ketch stays the only thing that owns the files and
 * `ketch uninstall` can actually undo it.
 */
export class PkgExtractor implements Extractor {
  readonly id = "pkg";

  detect(file: string, head: Uint8Array): boolean {
    const magic = Buffer.from(head.subarray(0, 4)).toString("latin1");
    return magic === "xar!" || hasExtension(file, ["pkg", "mpkg"]);
  }

  extract(src: string, dest: string): Promise<void> {
    return withTempDir(async (work) => {
      // `pkgutil` insists the destination not exist yet.
      const expanded = path.join(work, "expanded");
      await runTool("/usr/sbin/pkgutil", ["--expand-full", src, expanded]);

      const payloads = findPayloadRoots(expanded);
      if (payloads.length === 0) {
        throw KetchError.msg(`${src} expanded but contained no payload`);
      }
      for (const payload of payloads) {
        let entries: string[];
        try {
          entries = fs.readdirSync(payload);
        } catch (cause) {
          throw KetchError.io(payload, cause as Error);
        }
        for (const name of entries) {
          await copyTree(path.join(payload, name), path.join(dest, name));
        }
      }
    });
  }
}

/**
 * Find the directories holding the actual files.
 *
 * `pkgutil --expand-full` writes one `<component>.pkg/Payload/` per component;
 * older layouts call it `Root`. Everything else in the expansion is install
 * metadata that has no meaning outside the system installer.
 */
export function findPayloadRoots(expanded: string): string[] {
  const found: string[] = [];
  const walk = (dir: string, depth: number): void => {
    if (depth > 4) {
      return;
    }
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (!entry.isDirectory()) {
        continue;
      }
      const child = path.join(dir, entry.name);
      if (entry.name === "Payload" || entry.name === "Root") {
        found.push(child);
      }
      // A match is still descended into, as the Rust WalkDir scan does: a
      // Payload nested below another within the depth limit is payload too.
      walk(child, depth + 1);
    }
  };
  walk(expanded, 1);
  return found;
}
