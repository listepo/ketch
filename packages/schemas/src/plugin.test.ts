/** Wire-protocol shape tests, ported from the parsing side of source/plugin.rs. */

import { describe, expect, it } from "vitest";
import {
  PROTOCOL_VERSION,
  pluginCapabilitiesSchema,
  pluginDescribeSchema,
  pluginReleasesSchema,
  usableScheme,
} from "./plugin.ts";

describe("pluginCapabilitiesSchema", () => {
  it("download and search default to false", () => {
    const caps = pluginCapabilitiesSchema.parse({ protocol: PROTOCOL_VERSION, scheme: "gitlab" });
    expect(caps.download).toBe(false);
    expect(caps.search).toBe(false);
  });

  it("a capabilities document without a scheme is unreadable", () => {
    expect(() => pluginCapabilitiesSchema.parse({ protocol: 1 })).toThrow();
  });
});

describe("usableScheme", () => {
  it("a scheme must be typeable and unambiguous", () => {
    expect(usableScheme("gitlab")).toBe(true);
    expect(usableScheme("my_source-2")).toBe(true);
    expect(usableScheme("")).toBe(false);
    expect(usableScheme("a b")).toBe(false);
    expect(usableScheme("a/b")).toBe(false);
    expect(usableScheme("a:b")).toBe(false);
  });
});

describe("pluginReleasesSchema", () => {
  it("only version, tag and each asset's name and url are required", () => {
    const releases = pluginReleasesSchema.parse([
      {
        version: "1.0.0",
        tag: "v1.0.0",
        assets: [{ name: "tool.tar.gz", url: "https://example.invalid/tool.tar.gz" }],
      },
    ]);
    const release = releases[0];
    expect(release?.draft).toBe(false);
    expect(release?.prerelease).toBe(false);
    expect(release?.assets[0]?.size).toBe(0);
    expect(release?.assets[0]?.headers).toEqual({});
  });

  it("a digest carries both its algorithm and its hex", () => {
    const asset = { name: "a", url: "u", digest: { algo: "sha256" } };
    expect(() =>
      pluginReleasesSchema.parse([{ version: "1", tag: "v1", assets: [asset] }]),
    ).toThrow();
  });
});

describe("pluginDescribeSchema", () => {
  it("describe returns a source info object or null", () => {
    expect(pluginDescribeSchema.parse(null)).toBeNull();
    const info = pluginDescribeSchema.parse({ id: "group/project", name: "project" });
    expect(info?.archived).toBe(false);
  });
});
