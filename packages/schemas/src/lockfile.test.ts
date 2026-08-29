/** Ports of the validation tests from the Rust lockfile.rs, on the JSON shape. */

import { describe, expect, it } from "vitest";
import { parseLockfile } from "./lockfile.ts";

function entry(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    name: "fd",
    source: "github:sharkdp/fd",
    version: "10.2.0",
    tag: "v10.2.0",
    target: "macos-aarch64",
    asset: "fd.tar.gz",
    sha256: "a".repeat(64),
    ...overrides,
  };
}

describe("parseLockfile", () => {
  it("a well-formed lockfile passes", () => {
    const lock = parseLockfile({ version: 1, package: [entry()] });
    expect(lock.package).toHaveLength(1);
    expect(lock.package[0]?.pinned).toBe(false);
  });

  it("a name that would escape the store is refused", () => {
    expect(() => parseLockfile({ version: 1, package: [entry({ name: "../../.zshrc" })] })).toThrow(
      "is not a usable package name",
    );
  });

  it("a source that is not a repo is refused", () => {
    expect(() =>
      parseLockfile({ version: 1, package: [entry({ source: "github:../../etc" })] }),
    ).toThrow("is not a GitHub repository");
  });

  it("a hash that is not a hash is refused", () => {
    expect(() => parseLockfile({ version: 1, package: [entry({ sha256: "not-a-hash" })] })).toThrow(
      "where a sha256 belongs",
    );
  });

  it("a package listed twice is refused", () => {
    expect(() => parseLockfile({ version: 1, package: [entry(), entry()] })).toThrow(
      "is listed twice",
    );
  });

  it("duplicate detection ignores ascii case", () => {
    expect(() => parseLockfile({ version: 1, package: [entry(), entry({ name: "FD" })] })).toThrow(
      "is listed twice",
    );
  });

  it("an unknown key is refused rather than ignored", () => {
    expect(() => parseLockfile({ version: 1, package: [entry({ surprise: true })] })).toThrow();
  });

  it("a lockfile from a newer ketch is refused", () => {
    expect(() => parseLockfile({ version: 99, package: [] })).toThrow("written by a newer ketch");
  });

  it("an empty lockfile is valid and means nothing installed", () => {
    expect(parseLockfile({ version: 1 }).package).toEqual([]);
  });

  it("an empty tag has nothing to resolve", () => {
    expect(() => parseLockfile({ version: 1, package: [entry({ tag: "  " })] })).toThrow(
      "has no tag to resolve",
    );
  });

  it("a non-github source only needs a non-empty id", () => {
    const lock = parseLockfile({ version: 1, package: [entry({ source: "gitlab:group/proj" })] });
    expect(lock.package[0]?.source).toBe("gitlab:group/proj");
  });

  it("the pinned flag round-trips", () => {
    const lock = parseLockfile({ version: 1, package: [entry({ pinned: true })] });
    expect(lock.package[0]?.pinned).toBe(true);
  });

  it("a $schema pointer is allowed at the top level only", () => {
    expect(() => parseLockfile({ $schema: "https://x", version: 1, package: [] })).not.toThrow();
    expect(() =>
      parseLockfile({ version: 1, package: [entry({ $schema: "https://x" })] }),
    ).toThrow();
  });
});
