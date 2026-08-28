/**
 * End-to-end: `ketch.lock` and `ketch sync` — writing down what a machine has,
 * and putting it back somewhere else.
 *
 * Port of the lockfile tests in `tests/install.rs`. The file is JSON here
 * rather than TOML, so the assertions look for the keys the writer emits; the
 * claims are the Rust ones unchanged, including the one that matters most —
 * a payload that disagrees with the lock is refused before anything is
 * unpacked.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it, onTestFinished } from "vitest";
import {
  appArchive,
  hostArch,
  publishTool,
  Release,
  runProgram,
  Sandbox,
  toolArchive,
} from "./support.ts";

/** Each test drives several ketch processes, and each one starts a runtime. */
const TIMEOUT = 120_000;

/**
 * Where the lockfile goes in a test: inside the sandbox, never the cwd the
 * suite happens to run from.
 */
function lockAt(sandbox: Sandbox): string {
  return path.join(sandbox.home(), "ketch.lock");
}

describe("lockfile", () => {
  it(
    "a lockfile records what is installed and sync puts it back",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      const lock = lockAt(sandbox);

      sandbox.ok(["install", "test:testtool", "--yes"]);
      sandbox.ok(["lock", "--file", lock]);

      const text = fs.readFileSync(lock, "utf8");
      expect(text).toContain('"name": "testtool"');
      expect(text).toContain('"tag": "v1.0.0"');
      expect(text).toContain('"source": "test:testtool"');

      sandbox.ok(["lock", "--check", "--file", lock]);

      // Wipe it, then let the lockfile put it back.
      sandbox.ok(["uninstall", "testtool", "--yes"]);
      expect(fs.existsSync(path.join(sandbox.bin(), "testtool"))).toBe(false);
      sandbox.fail(["lock", "--check", "--file", lock]);

      sandbox.ok(["sync", "--file", lock]);
      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 1.0.0");
      sandbox.ok(["lock", "--check", "--file", lock]);
    },
    TIMEOUT,
  );

  /**
   * The reason to write versions down: a newer release exists and sync must
   * still produce the one that was locked.
   */
  it(
    "sync installs the locked tag, not the latest one",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      const lock = lockAt(sandbox);

      sandbox.ok(["install", "test:testtool", "--yes"]);
      sandbox.ok(["lock", "--file", lock]);

      // A newer release lands, and the machine takes it. The 1.0.0 asset is
      // the file already on disk rather than a fresh pack of the same tree:
      // the lock records its bytes, and the system `tar` does not repeat them.
      const arch = hostArch();
      const newer = sandbox.asset(
        `testtool-2.0.0-${arch}-apple-darwin.tar.gz`,
        toolArchive("2.0.0"),
      );
      const older = sandbox.publishedAsset(`testtool-1.0.0-${arch}-apple-darwin.tar.gz`);
      sandbox.publish("testtool", [new Release("2.0.0", [newer]), new Release("1.0.0", [older])]);
      sandbox.ok(["upgrade", "--yes"]);
      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 2.0.0");

      sandbox.fail(["lock", "--check", "--file", lock]);
      sandbox.ok(["sync", "--file", lock]);
      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 1.0.0");
    },
    TIMEOUT,
  );

  /**
   * A release replaced under a tag it already published is the thing a lockfile
   * exists to catch, and it must be caught before anything is unpacked.
   */
  it(
    "sync refuses a payload that is not the one that was locked",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      const lock = lockAt(sandbox);

      sandbox.ok(["install", "test:testtool", "--yes"]);
      sandbox.ok(["lock", "--file", lock]);
      sandbox.ok(["uninstall", "testtool", "--yes"]);

      // Same tag, different bytes — exactly what a re-tagged release looks like.
      const text = fs.readFileSync(lock, "utf8");
      const recorded = /"sha256": "([0-9a-fA-F]{64})"/.exec(text)?.[1];
      if (recorded === undefined) {
        throw new Error(`a sha256 in the lockfile:\n${text}`);
      }
      fs.writeFileSync(lock, text.replaceAll(recorded, "b".repeat(64)));

      const said = sandbox.fail(["sync", "--file", lock]);
      expect(said).toContain("does not match the lockfile");
      expect(
        fs.existsSync(path.join(sandbox.bin(), "testtool")),
        "a payload that did not match the lock was installed anyway",
      ).toBe(false);
    },
    TIMEOUT,
  );

  it(
    "prune removes what the lockfile does not name",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      const asset = sandbox.asset("TestApp-1.0.0-macos.zip", appArchive("1.0.0"));
      sandbox.publish("testapp", [new Release("1.0.0", [asset])]);
      const lock = lockAt(sandbox);

      sandbox.ok(["install", "test:testtool", "--yes"]);
      sandbox.ok(["lock", "--file", lock]);
      sandbox.ok(["install", "test:testapp", "--yes"]);

      // An extra is not drift on its own — only `--prune` treats it as such.
      sandbox.ok(["lock", "--check", "--file", lock]);
      sandbox.ok(["sync", "--prune", "--yes", "--file", lock]);
      expect(fs.existsSync(path.join(sandbox.apps(), "TestApp.app"))).toBe(false);
      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 1.0.0");
    },
    TIMEOUT,
  );

  it(
    "a lockfile naming a path instead of a package is refused",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const lock = lockAt(sandbox);
      fs.writeFileSync(
        lock,
        JSON.stringify({
          version: 1,
          package: [
            {
              name: "../../.zshrc",
              source: "test:testtool",
              version: "1.0.0",
              tag: "1.0.0",
              target: "macos-aarch64",
              asset: "t.tar.gz",
              sha256: "a".repeat(64),
            },
          ],
        }),
      );

      const said = sandbox.fail(["sync", "--file", lock]);
      expect(said).toContain("not a usable package name");
    },
    TIMEOUT,
  );
});
