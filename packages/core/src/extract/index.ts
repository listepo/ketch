/**
 * Turning a downloaded file into a directory of files.
 *
 * Extractors are selected by sniffing content, not by trusting the file name:
 * release assets are routinely named `.tar.gz` while being a plain binary, or
 * `.zip` while being a tarball.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import { KetchError } from "../errors.ts";
import type { Extractor } from "./extractor.ts";

export * from "./archive.ts";
export * from "./extractor.ts";
export * from "./macos.ts";

/**
 * Pick an extractor and run it. Returns the id of the one that ran, which the
 * caller logs — this module has no terminal of its own.
 */
export async function extractAuto(
  src: string,
  dest: string,
  extractors: readonly Extractor[],
): Promise<string> {
  const head = readHead(src);
  try {
    fs.mkdirSync(dest, { recursive: true });
  } catch (cause) {
    throw KetchError.io(dest, cause as Error);
  }
  for (const extractor of extractors) {
    if (extractor.detect(src, head)) {
      await extractor.extract(src, dest);
      return extractor.id;
    }
  }
  throw new KetchError({ kind: "unsupported_archive", path: src });
}

/** First 512 bytes of a file, or fewer if it is shorter. */
export function readHead(file: string): Uint8Array {
  let fd: number;
  try {
    fd = fs.openSync(file, "r");
  } catch (cause) {
    throw KetchError.io(file, cause as Error);
  }
  try {
    const buffer = Buffer.alloc(512);
    let filled = 0;
    while (filled < buffer.length) {
      let read: number;
      try {
        read = fs.readSync(fd, buffer, filled, buffer.length - filled, null);
      } catch (cause) {
        // Node retries EINTR itself for most calls but not all; a short read is
        // never an error, so only a real failure reaches here.
        throw KetchError.io(file, cause as Error);
      }
      if (read === 0) {
        break;
      }
      filled += read;
    }
    return buffer.subarray(0, filled);
  } finally {
    fs.closeSync(fd);
  }
}

/**
 * True when a directory name is itself a macOS bundle rather than a container
 * of files.
 *
 * A bundle is a directory to everything below the filesystem and a single
 * opaque item to everything above it, which is exactly the distinction
 * `unwrapSingleDir` and app discovery both need.
 */
export function isBundleName(name: string): boolean {
  return name.endsWith(".app") || name.endsWith(".framework");
}

/**
 * If the payload is a single wrapper directory, return it.
 *
 * Almost every release tarball unpacks to `tool-1.2.3-target/`; treating that
 * wrapper as the payload root is what makes `bin` paths in manifests short and
 * stable across versions.
 */
export function unwrapSingleDir(dir: string): string {
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch (cause) {
    throw KetchError.io(dir, cause as Error);
  }

  // Metadata directories macOS and archivers leave behind are not payload.
  const payload = entries.filter(
    (entry) => entry.name !== "__MACOSX" && entry.name !== ".DS_Store",
  );

  const only = payload[0];
  if (payload.length === 1 && only !== undefined) {
    const child = path.join(dir, only.name);
    let isDir: boolean;
    try {
      isDir = fs.statSync(child).isDirectory();
    } catch {
      isDir = false;
    }
    // A lone `.app` is the commonest shape a macOS zip or dmg has, and it is
    // the payload — not a wrapper around it. Unwrapping here would hand back
    // the bundle's `Contents`, and nothing downstream would ever see an app to
    // install.
    if (isDir && !isBundleName(only.name)) {
      return child;
    }
  }
  return dir;
}
