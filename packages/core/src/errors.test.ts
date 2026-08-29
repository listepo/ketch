/**
 * The Rust error.rs has no unit tests; these exist because the port rewrote
 * the Display impls as one renderer, and the user-facing strings must not
 * drift from what the Rust binary prints.
 */

import { describe, expect, it } from "vitest";
import { KetchError } from "./errors.ts";

describe("KetchError", () => {
  it("renders messages identical to the Rust Display impls", () => {
    expect(KetchError.msg("plain").message).toBe("plain");
    expect(KetchError.parse("ketch.lock", "bad json").message).toBe(
      "could not parse ketch.lock: bad json",
    );
    expect(KetchError.io("/tmp/x", new Error("denied")).message).toBe("/tmp/x: denied");
    expect(
      new KetchError({ kind: "http", url: "https://x", status: 403, detail: null }).message,
    ).toBe("HTTP 403 from https://x");
    expect(
      new KetchError({ kind: "no_compatible_asset", id: "a/b", tag: "v1", target: "macos-aarch64" })
        .message,
    ).toBe("release `v1` of `a/b` has no asset for macos-aarch64");
    expect(new KetchError({ kind: "unknown_scheme", scheme: "gitlab" }).message).toBe(
      "no source is registered for scheme `gitlab`",
    );
    expect(new KetchError({ kind: "plugin", name: "p", detail: "broke" }).message).toBe(
      "plugin `p`: broke",
    );
  });

  it("details surface checksum expectations and command stderr", () => {
    const mismatch = new KetchError({
      kind: "checksum_mismatch",
      name: "rg",
      expected: "aa",
      actual: "bb",
    });
    expect(mismatch.details()).toEqual(["expected aa", "actual   bb"]);

    const cmd = new KetchError({
      kind: "command",
      cmd: "codesign",
      status: "1",
      stderr: " a\nb \n",
    });
    expect(cmd.details()).toEqual(["a", "b"]);
    const quiet = new KetchError({ kind: "command", cmd: "codesign", status: "1", stderr: "  " });
    expect(quiet.details()).toEqual([]);
  });

  it("hints follow the failure class", () => {
    expect(new KetchError({ kind: "http", url: "u", status: 403, detail: null }).hint()).toContain(
      "rate limit",
    );
    expect(new KetchError({ kind: "http", url: "u", status: 404, detail: null }).hint()).toContain(
      "owner/repo",
    );
    expect(new KetchError({ kind: "unknown_scheme", scheme: "s" }).hint()).toBe(
      "Install a source plugin named `ketch-source-s` on PATH or in the plugins dir.",
    );
    expect(KetchError.msg("x").hint()).toBeNull();
  });

  it("exit codes let scripts branch on failure class", () => {
    expect(new KetchError({ kind: "not_installed", name: "x" }).exitCode()).toBe(4);
    expect(new KetchError({ kind: "pinned", name: "x", version: "1" }).exitCode()).toBe(5);
    expect(new KetchError({ kind: "checksum_missing", name: "x" }).exitCode()).toBe(6);
    expect(new KetchError({ kind: "http", url: "u", status: 500, detail: null }).exitCode()).toBe(
      7,
    );
    expect(new KetchError({ kind: "locked", detail: "pid 1" }).exitCode()).toBe(8);
    expect(KetchError.msg("x").exitCode()).toBe(1);
  });
});
