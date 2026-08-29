/**
 * End-to-end: replacing what is installed with a newer release, and the one
 * package that must be left alone while it happens.
 *
 * Port of the upgrade tests in `tests/install.rs`. An upgrade is the operation
 * with the most to lose — the old payload is still what the link points at
 * until the new one is in place — so both claims are about what survives it.
 */

import path from "node:path";
import { describe, expect, it, onTestFinished } from "vitest";
import { publishTool, runProgram, Sandbox } from "./support.ts";

/** Each test drives several ketch processes, and each one starts a runtime. */
const TIMEOUT = 120_000;

describe("upgrade", () => {
  it(
    "an upgrade replaces the payload and the link still works",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool@1.0.0", "--yes"]);

      const link = path.join(sandbox.bin(), "testtool");
      expect(runProgram(link)).toBe("testtool 1.0.0");

      publishTool(sandbox, "2.0.0");
      expect(sandbox.ok(["outdated"])).toContain("2.0.0");
      sandbox.ok(["upgrade", "--yes"]);

      expect(runProgram(link)).toBe("testtool 2.0.0");
      expect(sandbox.ok(["list", "--json"])).toContain('"version": "2.0.0"');
    },
    TIMEOUT,
  );

  it(
    "a pinned package is left where it is",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool", "--yes"]);
      sandbox.ok(["pin", "testtool"]);

      publishTool(sandbox, "2.0.0");
      sandbox.ok(["upgrade", "--yes"]);

      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 1.0.0");
    },
    TIMEOUT,
  );

  it(
    "an upgrade to a tag with a slash in it installs that tag",
    () => {
      const sandbox = new Sandbox();
      onTestFinished(() => sandbox.dispose());
      publishTool(sandbox, "1.0.0");
      sandbox.ok(["install", "test:testtool@1.0.0", "--yes"]);

      // A monorepo tag, which is a shape ketch does not get to rule out. The
      // plan reports the tag and the install has to ask for that exact string:
      // rebuilding `source@tag` and re-parsing it reads the tag's own slash as
      // part of the package id and loses the pin.
      publishTool(sandbox, "2.0.0", "cli/v2.0.0");
      sandbox.ok(["upgrade", "--yes"]);

      expect(runProgram(path.join(sandbox.bin(), "testtool"))).toBe("testtool 2.0.0");
    },
    TIMEOUT,
  );
});
