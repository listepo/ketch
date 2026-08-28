/**
 * End-to-end: reading a client app's changelog.
 *
 * Port of the changelog tests in `tests/install.rs`. Both claims are about
 * *which* text gets printed: the file a package ships beats the notes on the
 * release, and the section printed is the one for the version installed.
 */

import { describe, expect, it, onTestFinished } from "vitest";
import { publishTool, Sandbox } from "./support.ts";

/** Each test drives several ketch processes, and each one starts a runtime. */
const TIMEOUT = 120_000;

describe("changelog", () => {
  it(
    "changelog prints the section for the installed version",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool", "--yes"]);

      const out = sandbox.ok(["changelog", "testtool"]);
      expect(out, `no 1.0.0 section:\n${out}`).toContain("the first one");
      expect(out, `ran past 1.0.0 into 2.0.0:\n${out}`).not.toContain("the second one");

      const notes = sandbox.ok(["changelog", "testtool", "--release"]);
      expect(notes, `release notes not reached:\n${notes}`).toContain("published notes for 1.0.0");
      expect(notes, `read the file:\n${notes}`).not.toContain("the first one");
    },
    TIMEOUT,
  );

  /**
   * A package that is not installed has no file to read, so `--file` says so
   * rather than quietly printing the notes instead.
   */
  it(
    "changelog for a package with no file says where to look",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      const err = sandbox.fail(["changelog", "test:testtool", "--file"]);
      expect(err).toContain("not installed");
    },
    TIMEOUT,
  );
});
