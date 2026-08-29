/** Ports of the extract/macos.rs unit tests — parsing `hdiutil attach` output. */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { findDevice, findMountPoint } from "./macos.ts";

let dir = "";

beforeEach(() => {
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-macos-"));
});

afterEach(() => {
  fs.rmSync(dir, { recursive: true, force: true });
});

describe("hdiutil listing", () => {
  it("reads the mount point out of hdiutil output", () => {
    const mount = path.join(dir, "dmg.T0hgqZ");
    fs.mkdirSync(mount, { recursive: true });

    const listing =
      "/dev/disk4          \tGUID_partition_scheme\t\n" +
      `/dev/disk4s1        \tApple_HFS            \t${mount}\n`;
    expect(findMountPoint(listing)).toBe(mount);
  });

  it("ignores partitions with no mount point", () => {
    expect(findMountPoint("/dev/disk4\tGUID_partition_scheme\t\n")).toBeNull();
  });

  it("an image that mounts nothing can still be detached", () => {
    // Nothing mounted, so the device node is the only handle left for
    // detaching an image that is nonetheless attached.
    const listing = "/dev/disk4\tGUID_partition_scheme\t\n/dev/disk4s1\tApple_HFS\t\n";
    expect(findMountPoint(listing)).toBeNull();
    expect(findDevice(listing)).toBe("/dev/disk4");
  });
});
