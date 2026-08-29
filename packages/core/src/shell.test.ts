/** Ports of the shell.rs unit tests, one claim per test. */

import process from "node:process";
import { describe, expect, it } from "vitest";
import {
  BEGIN,
  block,
  blockSpan,
  candidates,
  END,
  exportLine,
  fromProgram,
  mentions,
  quoteFish,
  quotePosix,
  splice,
  unsplice,
} from "./shell.ts";

const BIN = "/home/u/.ketch/bin";

function zshBlock(): string {
  return block("zsh", BIN);
}

/** Mirrors Rust's `.expect(msg)`: unwraps a spliced/unspliced result or fails with `msg`. */
function expectSome(value: string | null, msg: string): string {
  if (value === null) {
    throw new Error(msg);
  }
  return value;
}

describe("shell", () => {
  it("a login shell argv name still identifies the shell", () => {
    expect(fromProgram("-zsh")).toBe("zsh");
    expect(fromProgram("/bin/bash")).toBe("bash");
    expect(fromProgram("/opt/homebrew/bin/fish")).toBe("fish");
    expect(fromProgram("/usr/bin/tcsh")).toBeNull();
    expect(fromProgram("")).toBeNull();
  });

  it("a quote in the path cannot end the quoting", () => {
    const line = exportLine("zsh", "/home/o'brien/.ketch/bin");
    expect(line).toBe("export PATH='/home/o'\\''brien/.ketch/bin':\"$PATH\"");
  });

  it("fish escapes the backslash that posix leaves alone", () => {
    expect(quotePosix("a\\b")).toBe("'a\\b'");
    expect(quoteFish("a\\b")).toBe("'a\\\\b'");
    expect(quoteFish("o'brien")).toBe("'o\\'brien'");
  });

  it("a dollar in the path is not expanded", () => {
    expect(exportLine("bash", "/home/$USER/bin")).toContain("'/home/$USER/bin'");
  });

  it("the block is added once and then left alone", () => {
    const first = expectSome(splice("# mine\n", zshBlock()), "first write");
    expect(first.startsWith("# mine\n\n")).toBe(true);
    expect(first).toContain(BIN);
    expect(splice(first, zshBlock())).toBeNull();
  });

  it("a moved bin dir rewrites the block in place", () => {
    const before = expectSome(splice("# mine\n", zshBlock()), "first write");
    const after = expectSome(splice(before, block("zsh", "/elsewhere/bin")), "rewrite");
    expect(after).toContain("/elsewhere/bin");
    expect(after).not.toContain(BIN);
    expect(after.split(BEGIN).length - 1).toBe(1);
  });

  it("removing the block restores the file byte for byte", () => {
    const original = "# mine\nexport EDITOR=vi\n";
    const withBlock = expectSome(splice(original, zshBlock()), "write");
    expect(unsplice(withBlock)).toBe(original);
  });

  it("removing a block that was never there changes nothing", () => {
    expect(unsplice("# mine\n")).toBeNull();
  });

  it("install and uninstall cannot grow the file", () => {
    const original = "# mine\n";
    let text = original;
    for (let i = 0; i < 3; i++) {
      text = splice(text, zshBlock()) ?? text;
      text = unsplice(text) ?? text;
    }
    expect(text).toBe(original);
  });

  it("a marker that is not a whole line is not the block", () => {
    // Somebody's own script that merely prints the marker.
    const text = `echo "${BEGIN} here"\n${END} trailing\n`;
    expect(blockSpan(text)).toBeNull();
  });

  it("a block at the very start of a file is found", () => {
    const text = `${zshBlock()}rest\n`;
    expect(unsplice(text)).toBe("rest\n");
  });

  it("a path the user added by hand counts as configured", () => {
    expect(mentions('export PATH="/home/u/.ketch/bin:$PATH"\n', BIN)).toBe(true);
  });

  it("a commented out line does not count as configured", () => {
    expect(mentions('  # export PATH="/home/u/.ketch/bin:$PATH"\n', BIN)).toBe(false);
    expect(mentions("", BIN)).toBe(false);
  });

  it("a file without a trailing newline still gets a clean block", () => {
    const text = expectSome(splice("# mine", zshBlock()), "write");
    expect(text.startsWith("# mine\n\n")).toBe(true);
    expect(text.endsWith(`${END}\n`)).toBe(true);
  });

  it("an empty file gets the block with no leading blank line", () => {
    const text = expectSome(splice("", zshBlock()), "write");
    expect(text.startsWith(BEGIN)).toBe(true);
  });

  it("each shell gets the syntax it can actually run", () => {
    expect(exportLine("bash", BIN).startsWith("export PATH=")).toBe(true);
    expect(exportLine("zsh", BIN).startsWith("export PATH=")).toBe(true);
    expect(exportLine("fish", BIN)).toBe("set -gx PATH '/home/u/.ketch/bin' $PATH");
  });

  it("bash prefers a file the host actually reads", () => {
    const home = "/home/u";
    const first = candidates("bash", home)[0];
    if (process.platform === "darwin") {
      expect(first?.endsWith(".bash_profile")).toBe(true);
    } else {
      expect(first?.endsWith(".bashrc")).toBe(true);
    }
  });
});
