/**
 * End-to-end: the parts of ketch that are about the machine rather than a
 * package — `doctor`, the shell startup file `path` writes, and the log every
 * run leaves behind.
 *
 * Port of the environment tests in `tests/install.rs`. One deliberate change:
 * the settings file is `config.json` here, not `config.toml`, so the test that
 * proves a bad setting is refused *by name* looks for the name it now has.
 */

import fs from "node:fs";
import path from "node:path";
import { describe, expect, it, onTestFinished } from "vitest";
import { lines, publishTool, Sandbox } from "./support.ts";

/** Each test drives several ketch processes, and each one starts a runtime. */
const TIMEOUT = 120_000;

describe("doctor and PATH", () => {
  it(
    "doctor reports a healthy tree",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool", "--yes"]);

      // Exit status is the assertion: doctor fails when the tree is broken.
      sandbox.ok(["doctor"]);
    },
    TIMEOUT,
  );

  /**
   * A shell startup file is the one thing ketch writes outside its own root, so
   * the guarantee worth proving end to end is that the user's own file survives
   * being written, rewritten and taken back out.
   */
  it(
    "path install edits a shell config and can undo itself",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const zshrc = path.join(sandbox.home(), ".zshrc");
      const original = "# mine\nexport EDITOR=vi\n";
      fs.writeFileSync(zshrc, original);

      sandbox.ok(["path", "install", "--shell", "zsh"]);
      const after = fs.readFileSync(zshrc, "utf8");
      expect(after.startsWith(original), `the user's own lines moved:\n${after}`).toBe(true);
      expect(after, `bin dir missing from:\n${after}`).toContain(sandbox.bin());

      // Twice must not mean two blocks.
      sandbox.ok(["path", "install", "--shell", "zsh"]);
      const twice = fs.readFileSync(zshrc, "utf8");
      expect(twice, "a second install changed the file").toBe(after);

      sandbox.ok(["path", "uninstall", "--shell", "zsh"]);
      const restored = fs.readFileSync(zshrc, "utf8");
      expect(restored, "uninstall did not restore the file").toBe(original);
    },
    TIMEOUT,
  );

  it(
    "a dry run says what it would do and writes nothing",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      // Progress goes to stderr, so output stays pipeable — see `ui`.
      const out = sandbox.run(["path", "install", "--shell", "fish", "--dry-run"]);
      expect(out.status, out.stderr).toBe(0);
      expect(out.stderr).toContain("would add");
      expect(
        fs.existsSync(path.join(sandbox.home(), ".config/fish/config.fish")),
        "a dry run created the file",
      ).toBe(false);
    },
    TIMEOUT,
  );

  /**
   * The whole point of `--fix`: a PATH that no shell knows about is the one
   * doctor failure the user should not have to act on themselves.
   */
  it(
    "doctor fixes a path no shell knows about",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());

      const failed = sandbox.runOffPath(["doctor"]);
      expect(failed.status, "doctor passed without PATH set up").not.toBe(0);
      expect(failed.stdout).toContain("is not on PATH");

      const fixed = sandbox.runOffPath(["doctor", "--fix"]);
      expect(fixed.status, `doctor --fix still failed:\n${fixed.stdout}\n${fixed.stderr}`).toBe(0);
      // Fixed, but not in *this* process: the check has to say so rather than
      // reporting the same failure it just repaired.
      expect(fixed.stdout).toContain("but not in this shell");

      const zshrc = fs.readFileSync(path.join(sandbox.home(), ".zshrc"), "utf8");
      expect(zshrc).toContain(sandbox.bin());
    },
    TIMEOUT,
  );

  it(
    "a path the user wired up by hand is never duplicated",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      const zshrc = path.join(sandbox.home(), ".zshrc");
      const mine = `export PATH="${sandbox.bin()}:$PATH"\n`;
      fs.writeFileSync(zshrc, mine);

      sandbox.ok(["path", "install", "--shell", "zsh"]);
      expect(
        fs.readFileSync(zshrc, "utf8"),
        "ketch added a second copy of a line the user already had",
      ).toBe(mine);
    },
    TIMEOUT,
  );
});

describe("the log", () => {
  /**
   * The log is what is left after the terminal scrolls away, so a run has to be
   * in it — and a failure has to say where to find it.
   */
  it(
    "every run is logged and a failure says where the log is",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");

      sandbox.ok(["install", "test:testtool", "--yes"]);
      const first = sandbox.log();
      expect(first, `no records:\n${first}`).toContain("INFO");
      expect(first, `no command:\n${first}`).toContain("install test:testtool");
      expect(first, `no result:\n${first}`).toContain("installed testtool 1.0.0");

      const err = sandbox.fail(["install", "test:not-published", "--yes"]);
      expect(err, `no pointer to the log:\n${err}`).toContain("ketch.log");
      const second = sandbox.log();
      expect(second, `the failure was not logged:\n${second}`).toContain("ERROR");
      expect(
        lines(second).every((line) => line !== ""),
        `a record was split across lines:\n${second}`,
      ).toBe(true);
    },
    TIMEOUT,
  );

  /**
   * The other half of "a common log format": JSON Lines, for anything that is
   * not a person reading it.
   */
  it(
    "the log can be JSON Lines instead",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      sandbox.configure({ log_format: "json", log_level: "debug" });
      sandbox.ok(["list"]);

      const log = sandbox.log();
      const [first] = lines(log);
      if (first === undefined) {
        throw new Error(`a record:\n${log}`);
      }
      const parsed = JSON.parse(first) as { level?: unknown; msg?: unknown; time?: unknown };
      expect(parsed.level).toBe("info");
      expect(typeof parsed.msg === "string" && parsed.msg.includes("list")).toBe(true);
      expect(typeof parsed.time === "string" && parsed.time.endsWith("Z")).toBe(true);
      expect(
        lines(log).some((line) => line.includes('"debug"')),
        log,
      ).toBe(true);
    },
    TIMEOUT,
  );

  /** A bad setting is the user's own file, and has to say which one. */
  it(
    "an unreadable log setting is refused by name",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      sandbox.configure({ log_level: "chatty" });
      const err = sandbox.fail(["list"]);
      expect(err).toContain("chatty");
      expect(err).toContain("config.json");
    },
    TIMEOUT,
  );
});
