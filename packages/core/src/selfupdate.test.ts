/**
 * Coverage for the pieces of `selfupdate.ts` a unit test can safely reach.
 *
 * `selfupdate.rs` carries no `#[cfg(test)]` module of its own — replacing the
 * running binary is inherently an integration concern that Rust leaves to
 * manual verification against a real release. What a unit test *can* reach
 * without a network or a second ketch process is the two pieces this module
 * does not delegate to `install.ts`: the walk that finds the binary inside an
 * unpacked release (`findBinary`), and the rename/copy/verify/restore
 * choreography that swaps it in (`replaceBinary`) — the exact "kept until the
 * new one has proven it can run" guarantee the module's own header describes.
 * Both are exported for this reason alone, the same way `log.ts` exports
 * `resetForTests`.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { KetchError } from "./errors.ts";
import { currentExe, currentVersion, findBinary, replaceBinary } from "./selfupdate.ts";

function tmpDir(prefix: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function scriptFile(dir: string, name: string, body: string): string {
  const file = path.join(dir, name);
  fs.writeFileSync(file, body);
  fs.chmodSync(file, 0o755);
  return file;
}

describe("currentVersion", () => {
  it("parses the raw string the caller supplies", () => {
    expect(currentVersion("1.2.3").toString()).toBe("1.2.3");
  });
});

describe("currentExe", () => {
  // The suite itself is the hostile case: vitest is a script an interpreter
  // loaded, so `process.execPath` here is node or bun. Returning it would
  // hand `update` the interpreter to overwrite and `uninstallSelf` the
  // interpreter to delete. A compiled ketch has no entry file to resolve, so
  // it takes the other branch and names itself.
  it("refuses to name the interpreter when ketch was loaded from source", () => {
    expect(() => currentExe()).toThrow(/running from source/);
  });

  it("never returns the running interpreter", () => {
    let named: string | undefined;
    try {
      named = currentExe();
    } catch {
      named = undefined;
    }
    expect(named).not.toBe(process.execPath);
  });
});

describe("findBinary", () => {
  it("finds ketch nested inside the unpacked payload", () => {
    const payload = tmpDir("ketch-payload-");
    const nested = path.join(payload, "ketch-0.2.0-macos", "bin");
    fs.mkdirSync(nested, { recursive: true });
    fs.writeFileSync(path.join(nested, "README.md"), "not it");
    const wanted = scriptFile(nested, "ketch", "#!/bin/sh\nexit 0\n");

    expect(findBinary(payload)).toBe(wanted);
  });

  it("skips a symlink even when it is named ketch", () => {
    const payload = tmpDir("ketch-payload-");
    const real = scriptFile(payload, "real-ketch", "#!/bin/sh\nexit 0\n");
    fs.symlinkSync(real, path.join(payload, "ketch"));

    expect(() => findBinary(payload)).toThrow(KetchError);
  });

  it("throws empty_payload when nothing is named ketch", () => {
    const payload = tmpDir("ketch-payload-");
    fs.writeFileSync(path.join(payload, "other"), "");

    expect(() => findBinary(payload)).toThrow(/no installable files/);
  });
});

describe("replaceBinary", () => {
  it("swaps in a binary that proves it can run, and drops the backup", () => {
    const dir = tmpDir("ketch-exe-");
    const exe = scriptFile(dir, "ketch", "#!/bin/sh\necho old\n");
    const fresh = scriptFile(dir, "ketch.new", "#!/bin/sh\nexit 0\n");

    replaceBinary(exe, fresh);

    expect(fs.readFileSync(exe, "utf8")).toBe("#!/bin/sh\nexit 0\n");
    expect(fs.existsSync(`${exe}.old`)).toBe(false);
    expect(fs.statSync(exe).mode & 0o777).toBe(0o755);
  });

  it("restores the previous binary when the new one cannot prove it runs", () => {
    const dir = tmpDir("ketch-exe-");
    const original = "#!/bin/sh\necho old\n";
    const exe = scriptFile(dir, "ketch", original);
    const fresh = scriptFile(dir, "ketch.new", "#!/bin/sh\nexit 1\n");

    expect(() => replaceBinary(exe, fresh)).toThrow(KetchError);
    expect(fs.readFileSync(exe, "utf8")).toBe(original);
    expect(fs.existsSync(`${exe}.old`)).toBe(false);
  });
});
