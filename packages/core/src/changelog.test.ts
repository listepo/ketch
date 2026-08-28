/** Ports of the changelog.rs unit tests, one claim per test. */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { atxLevel, findFile, fromRelease, sanitize, section } from "./changelog.ts";

const KEEP_A_CHANGELOG = `# Changelog

## [Unreleased]

- something in flight

## [1.2.3] - 2024-05-01

### Added

- the thing everybody wanted

### Fixed

- the thing nobody noticed

## [1.2.2] - 2024-04-01

- an older fix
`;

function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "ketch-changelog-"));
}

describe("changelog", () => {
  it("a Keep a Changelog section stops at the next release", () => {
    const found = section(KEEP_A_CHANGELOG, "1.2.3");
    expect(found).not.toBeNull();
    const { heading, body } = found ?? { heading: "", body: "" };
    expect(heading).toBe("## [1.2.3] - 2024-05-01");
    expect(body).toContain("the thing everybody wanted");
    // Sub-headings stay in.
    expect(body).toContain("### Fixed");
    // Must not run into 1.2.2.
    expect(body).not.toContain("an older fix");
    expect(body).not.toContain("something in flight");
  });

  it("a tag and its version find the same section", () => {
    const byTag = section(KEEP_A_CHANGELOG, "v1.2.3");
    const byVersion = section(KEEP_A_CHANGELOG, "1.2.3");
    expect(byTag).not.toBeNull();
    expect(byVersion).not.toBeNull();
    expect(byTag?.heading).toBe(byVersion?.heading);
  });

  it("the last section in a file runs to the end", () => {
    const found = section(KEEP_A_CHANGELOG, "1.2.2");
    expect(found?.body).toBe("- an older fix");
  });

  it("every way people write a version heading is recognised", () => {
    for (const heading of [
      "## 1.2.3",
      "## v1.2.3",
      "## [1.2.3]",
      "## [1.2.3] - 2024-05-01",
      "## 1.2.3 (2024-05-01)",
      "## Release 1.2.3",
      "### v1.2.3 — codename",
      "# 1.2.3",
    ]) {
      const text = `${heading}\n\n- a change\n`;
      // The message names the heading that failed to match.
      expect(section(text, "1.2.3"), `did not match \`${heading}\``).not.toBeNull();
    }
  });

  it("a version that is a prefix of another is not matched", () => {
    const text = "## 1.2.30\n\n- not this one\n";
    expect(section(text, "1.2.3")).toBeNull();
  });

  it("a version nobody wrote about has no section", () => {
    expect(section(KEEP_A_CHANGELOG, "9.9.9")).toBeNull();
    expect(section(KEEP_A_CHANGELOG, "")).toBeNull();
  });

  it("a deeper heading does not end a section but a shallower one does", () => {
    const text = "## 1.0.0\n\n### Added\n- a\n\n# 0.9.0\n- old\n";
    const found = section(text, "1.0.0");
    expect(found?.body).toContain("### Added");
    expect(found?.body).not.toContain("old");
  });

  it("a run of hashes that is not a heading is not treated as one", () => {
    expect(atxLevel("#tag")).toBeNull();
    expect(atxLevel("####### too deep")).toBeNull();
    expect(atxLevel("## fine")).toBe(2);
    expect(atxLevel("not a heading")).toBeNull();
  });

  it("an underlined version is a heading and the underline is not in the body", () => {
    const text =
      "15.2.0 (2026-07-15)\n===================\nPlatform support:\n\n* a thing\n\n" +
      "15.1.0 (2026-05-01)\n===================\nolder\n";
    const found = section(text, "15.2.0");
    expect(found?.heading).toBe("15.2.0 (2026-07-15)");
    expect(found?.body.startsWith("Platform support:")).toBe(true);
    // Must not run into 15.1.0.
    expect(found?.body).not.toContain("older");
    // Must not keep the underline.
    expect(found?.body).not.toContain("===");
  });

  it("a rule between releases never swallows the lines above it", () => {
    // `---` is a Setext underline in Markdown, but in a changelog it is
    // nearly always a separator; treating it as a heading would cut every
    // section short at the line before the rule.
    const text = "## 1.2.3\n\n- the change\n\n---\n\n## 1.2.2\n";
    const found = section(text, "1.2.3");
    expect(found?.body, "truncated at the rule").toContain("- the change");
  });

  it("a changelog cannot drive the terminal it is printed to", () => {
    const hostile = "\u001b[2J\u001b]0;pwned\u0007real\rtext\u202Ereversed\u009bm";
    const entry = fromRelease(hostile);
    expect(entry).not.toBeNull();
    expect(entry?.body).toBe("[2J]0;pwnedrealtextreversedm");
    expect(entry?.body).not.toContain("\u001b");
    expect(entry?.body).not.toContain("\r");
  });

  it("sanitising keeps the shape of the prose", () => {
    expect(sanitize("## 1.0\n\n\t- a\r\n- b\n")).toBe("## 1.0\n\n\t- a\n- b\n");
  });

  it("a release with no notes produces nothing rather than a blank entry", () => {
    expect(fromRelease(null)).toBeNull();
    expect(fromRelease("   \n ")).toBeNull();
    expect(fromRelease("real notes")).not.toBeNull();
  });

  it("a changelog is found where projects actually put it", () => {
    const dir = tempDir();
    try {
      const nested = path.join(dir, "share/doc/testtool");
      fs.mkdirSync(nested, { recursive: true });
      fs.writeFileSync(path.join(nested, "CHANGELOG.md"), "# Changelog\n");
      expect(findFile(dir)).toBe(path.join(nested, "CHANGELOG.md"));
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("a payload with no changelog reports none", () => {
    const dir = tempDir();
    try {
      fs.writeFileSync(path.join(dir, "README.md"), "# hi\n");
      expect(findFile(dir)).toBeNull();
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it("the root is preferred over a copy buried in docs", () => {
    const dir = tempDir();
    try {
      fs.mkdirSync(path.join(dir, "docs"), { recursive: true });
      fs.writeFileSync(path.join(dir, "CHANGELOG.md"), "root\n");
      fs.writeFileSync(path.join(dir, "docs/CHANGELOG.md"), "docs\n");
      expect(findFile(dir)).toBe(path.join(dir, "CHANGELOG.md"));
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });
});
