/**
 * End-to-end: the real CLI, a real install tree, real archives.
 *
 * These exist because the unit tests each prove one function and none of them
 * prove the pipeline. Every bug this suite was written against — a bundle
 * unwrapped into its own `Contents`, a link left pointing at a deleted store,
 * an upgrade that removed the old version before the new one was in place —
 * passed every unit test in the tree.
 *
 * Port of the install half of `tests/install.rs`: placing a payload, refusing
 * one, taking it away again, and the batch that does several at once.
 * macOS-only, like the platform layer they exercise.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it, onTestFinished } from "vitest";
import {
  appArchive,
  hostArch,
  lines,
  publishNamed,
  publishTool,
  Release,
  runProgram,
  Sandbox,
  toolArchive,
} from "./support.ts";

/** Each test drives several ketch processes, and each one starts a runtime. */
const TIMEOUT = 120_000;

/** Whether `p` is a symlink, without throwing when nothing is there at all. */
function isSymlink(p: string): boolean {
  return fs.lstatSync(p, { throwIfNoEntry: false })?.isSymbolicLink() ?? false;
}

function isDir(p: string): boolean {
  return fs.statSync(p, { throwIfNoEntry: false })?.isDirectory() ?? false;
}

function isFile(p: string): boolean {
  return fs.statSync(p, { throwIfNoEntry: false })?.isFile() ?? false;
}

describe("install", () => {
  it(
    "a tool is downloaded, verified, linked and runnable",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");

      sandbox.ok(["install", "test:testtool", "--yes"]);

      const link = path.join(sandbox.bin(), "testtool");
      expect(isSymlink(link), `expected a symlink at ${link}`).toBe(true);
      expect(runProgram(link)).toBe("testtool 1.0.0");

      const listed = sandbox.ok(["list", "--json"]);
      expect(listed).toContain('"name": "testtool"');
      // The plugin published a digest, so this was verified rather than trusted
      // on first use.
      expect(listed).toContain('"checksum_verified": true');
      // The asset naming this machine's architecture beat the Linux decoy.
      expect(listed).toContain(`testtool-1.0.0-${hostArch()}-apple-darwin.tar.gz`);
    },
    TIMEOUT,
  );

  it(
    "an app bundle is placed whole and removed again",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const asset = sandbox.asset("TestApp-1.0.0-macos.zip", appArchive("1.0.0"));
      sandbox.publish("testapp", [new Release("1.0.0", [asset])]);

      sandbox.ok(["install", "test:testapp", "--yes"]);

      // The bundle is the payload. Unwrapping it as though it were a wrapper
      // directory would place `Contents` and leave no app at all.
      const app = path.join(sandbox.apps(), "TestApp.app");
      expect(isDir(app), `expected ${app} to exist`).toBe(true);
      expect(runProgram(path.join(app, "Contents/MacOS/TestApp"))).toBe("app 1.0.0");
      expect(isFile(path.join(app, "Contents/Info.plist"))).toBe(true);

      // An app is not a command-line tool: its executables stay out of PATH.
      expect(fs.existsSync(path.join(sandbox.bin(), "TestApp"))).toBe(false);

      sandbox.ok(["uninstall", "testapp", "--yes"]);
      expect(fs.existsSync(app), `${app} outlived its package`).toBe(false);
    },
    TIMEOUT,
  );

  it(
    "a download that does not match its checksum installs nothing",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const arch = hostArch();
      const tampered = sandbox
        .asset(`testtool-1.0.0-${arch}-apple-darwin.tar.gz`, toolArchive("1.0.0"))
        .withWrongDigest();
      sandbox.publish("testtool", [new Release("1.0.0", [tampered])]);

      const stderr = sandbox.fail(["install", "test:testtool", "--yes"]);
      expect(stderr.toLowerCase()).toContain("checksum");

      // A refused install leaves nothing behind: no link, no store directory,
      // and nothing recorded.
      expect(fs.existsSync(path.join(sandbox.bin(), "testtool"))).toBe(false);
      expect(fs.existsSync(path.join(sandbox.store(), "testtool"))).toBe(false);
      expect(sandbox.ok(["list"])).toContain("nothing installed");
    },
    TIMEOUT,
  );

  it(
    "uninstall removes every trace of a tool",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool", "--yes"]);

      sandbox.ok(["uninstall", "testtool", "--yes"]);

      expect(fs.existsSync(path.join(sandbox.bin(), "testtool"))).toBe(false);
      expect(fs.existsSync(path.join(sandbox.store(), "testtool"))).toBe(false);
      expect(sandbox.ok(["list"])).toContain("nothing installed");
    },
    TIMEOUT,
  );

  it(
    "a binary the user put there is never overwritten",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");

      // Something already occupies the name ketch wants.
      fs.mkdirSync(sandbox.bin(), { recursive: true });
      const squatter = path.join(sandbox.bin(), "testtool");
      const mine = "#!/bin/sh\necho 'not ketch'\n";
      fs.writeFileSync(squatter, mine);

      const stderr = sandbox.fail(["install", "test:testtool", "--yes"]);
      expect(stderr).toContain("not installed by ketch");
      expect(fs.readFileSync(squatter, "utf8")).toBe(mine);
    },
    TIMEOUT,
  );

  it(
    "relink rebuilds a link that was deleted by hand",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool", "--yes"]);

      const link = path.join(sandbox.bin(), "testtool");
      fs.rmSync(link);

      sandbox.ok(["link", "testtool"]);
      expect(runProgram(link)).toBe("testtool 1.0.0");
    },
    TIMEOUT,
  );

  /**
   * A batch install runs its downloads concurrently, so this proves the part
   * that concurrency could break: every package ends up placed, runnable and
   * recorded, and the results are reported in the order they were asked for.
   */
  it(
    "a batch installs every package and reports them in the order asked",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const names = ["delta", "alpha", "charlie", "bravo"];
      for (const name of names) {
        publishNamed(sandbox, name, "1.0.0");
      }

      const out = sandbox.run([
        "install",
        "test:delta",
        "test:alpha",
        "test:charlie",
        "test:bravo",
        "--yes",
      ]);
      expect(out.status, `install failed\n${out.stderr}`).toBe(0);

      const reported = lines(out.stderr).flatMap((line) => {
        const second = line.trim().split(/\s+/)[1];
        return line.includes("installed") && second !== undefined ? [second] : [];
      });
      expect(reported, `reported out of order:\n${out.stderr}`).toEqual(names);

      for (const name of names) {
        expect(runProgram(path.join(sandbox.bin(), name))).toBe(`${name} 1.0.0`);
      }
      const installed = lines(sandbox.ok(["list", "--names-only"])).toSorted();
      expect(installed).toEqual(["alpha", "bravo", "charlie", "delta"]);

      // Every download and every unpack is staged in the cache. Concurrency is
      // exactly where a leaked staging directory would start being invisible.
      const left = fs.readdirSync(path.join(sandbox.root(), "cache"));
      expect(left, `left behind in the cache: ${left.join(", ")}`).toEqual([]);
    },
    TIMEOUT,
  );

  /**
   * Two spellings of one package in a single batch. They resolve to the same
   * name and the same asset, so before each download was staged in a directory
   * of its own they raced for one path in the cache.
   */
  it(
    "the same package asked for two ways installs cleanly",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");

      sandbox.ok(["install", "test:testtool", "test:testtool@1.0.0", "--yes"]);
      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 1.0.0");
      expect(sandbox.ok(["list", "--names-only"]).trim()).toBe("testtool");
    },
    TIMEOUT,
  );

  /**
   * A batch is not all-or-nothing: the packages that resolved are installed and
   * the one that did not is named.
   */
  it(
    "one bad package in a batch does not lose the good ones",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishNamed(sandbox, "alpha", "1.0.0");
      publishNamed(sandbox, "bravo", "1.0.0");

      const err = sandbox.fail([
        "install",
        "test:alpha",
        "test:nothing-published-here",
        "test:bravo",
        "--yes",
      ]);
      expect(err).toContain("nothing-published-here");

      const installed = lines(sandbox.ok(["list", "--names-only"])).toSorted();
      expect(installed, "good packages were lost").toEqual(["alpha", "bravo"]);
    },
    TIMEOUT,
  );

  /** `--jobs 1` is the escape hatch, and has to install exactly the same tree. */
  it(
    "a batch with one job installs the same thing",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishNamed(sandbox, "alpha", "1.0.0");
      publishNamed(sandbox, "bravo", "1.0.0");

      sandbox.ok(["install", "test:alpha", "test:bravo", "--jobs", "1", "--yes"]);
      expect(runProgram(path.join(sandbox.bin(), "alpha"))).toBe("alpha 1.0.0");
      expect(runProgram(path.join(sandbox.bin(), "bravo"))).toBe("bravo 1.0.0");
    },
    TIMEOUT,
  );

  it(
    "a link that was repointed is left alone, and says so under --verbose",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool", "--yes"]);

      // Something else has taken the name over since. Removing the link would
      // break whoever owns it now, so uninstall leaves it — and the only way
      // anyone learns that is the platform's debug line.
      const link = path.join(sandbox.bin(), "testtool");
      const other = path.join(sandbox.root(), "somewhere-else");
      fs.writeFileSync(other, "#!/bin/sh\necho other\n", { mode: 0o755 });
      fs.unlinkSync(link);
      fs.symlinkSync(other, link);

      const out = sandbox.run(["uninstall", "testtool", "--yes", "--verbose"]);

      expect(out.status).toBe(0);
      expect(out.stderr).toMatch(/leaving .*testtool: it no longer points at/);
      expect(fs.readlinkSync(link)).toBe(other);
    },
    TIMEOUT,
  );

  it(
    "--require-checksum refuses a release that publishes none",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const unsigned = sandbox
        .asset(`testtool-1.0.0-${hostArch()}-apple-darwin.tar.gz`, toolArchive("1.0.0"))
        .withoutDigest();
      sandbox.publish("testtool", [new Release("1.0.0", [unsigned])]);

      const stderr = sandbox.fail(["install", "test:testtool", "--yes", "--require-checksum"]);
      expect(stderr.toLowerCase()).toContain("checksum");
      expect(fs.existsSync(path.join(sandbox.bin(), "testtool"))).toBe(false);
      expect(sandbox.ok(["list"])).toContain("nothing installed");
    },
    TIMEOUT,
  );

  it(
    "require_checksums in config.json refuses it too",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      // The flag and the setting are two doors onto the same decision, and a
      // setting that quietly did nothing would be the worse of the two to get
      // wrong: nobody retypes it on the next install to check.
      sandbox.configure({ require_checksums: true });
      const unsigned = sandbox
        .asset(`testtool-1.0.0-${hostArch()}-apple-darwin.tar.gz`, toolArchive("1.0.0"))
        .withoutDigest();
      sandbox.publish("testtool", [new Release("1.0.0", [unsigned])]);

      const stderr = sandbox.fail(["install", "test:testtool", "--yes"]);
      expect(stderr.toLowerCase()).toContain("checksum");
      expect(sandbox.ok(["list"])).toContain("nothing installed");
    },
    TIMEOUT,
  );

  it(
    "without either, an unchecksummed release is recorded on first use",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const unsigned = sandbox
        .asset(`testtool-1.0.0-${hostArch()}-apple-darwin.tar.gz`, toolArchive("1.0.0"))
        .withoutDigest();
      sandbox.publish("testtool", [new Release("1.0.0", [unsigned])]);

      sandbox.ok(["install", "test:testtool", "--yes"]);

      // Installed, but the record says plainly that nothing vouched for it.
      expect(sandbox.ok(["list", "--json"])).toContain('"checksum_verified": false');
    },
    TIMEOUT,
  );
});
