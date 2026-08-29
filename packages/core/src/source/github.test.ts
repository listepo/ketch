/** Ports of the source/github.rs tests, one claim per test. */

import { afterEach, describe, expect, it, vi } from "vitest";
import { Http } from "../http.ts";
import {
  GitHubSource,
  isAggregateChecksumFile,
  parseChecksumFile,
  parseDigest,
  urlencodePathSegment,
} from "./github.ts";
import { defaultListOpts } from "./source.ts";

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("parseChecksumFile", () => {
  it("parses sha256sum files in their usual shapes", () => {
    const body = `# generated
9f2b1e0000000000000000000000000000000000000000000000000000000abc  rg-14.tar.gz
5c3d000000000000000000000000000000000000000000000000000000000def *./dist/rg-14.zip
not-a-hash                                                          junk.txt
`;
    const map = parseChecksumFile(body);
    expect(map.size).toBe(2);
    expect(map.get("rg-14.tar.gz")).toBe(
      "9f2b1e0000000000000000000000000000000000000000000000000000000abc",
    );
    // Directory prefixes and the binary-mode star are both stripped.
    expect(map.has("rg-14.zip")).toBe(true);
  });
});

describe("parseDigest", () => {
  it("reads the api digest field", () => {
    const hex = "a".repeat(64);
    expect(parseDigest(`sha256:${hex}`)).toBe(hex);
    expect(parseDigest("md5:abc")).toBeNull();
    expect(parseDigest("sha256:tooshort")).toBeNull();
  });
});

describe("isAggregateChecksumFile", () => {
  it("recognises aggregate checksum assets", () => {
    expect(isAggregateChecksumFile("SHA256SUMS")).toBe(true);
    expect(isAggregateChecksumFile("tool_1.0_checksums.txt")).toBe(true);
    expect(isAggregateChecksumFile("rg-14.tar.gz")).toBe(false);
  });
});

describe("GitHubSource", () => {
  it("derives a browsable url from the api base", () => {
    vi.stubEnv("KETCH_GITHUB_API", "");
    const source = new GitHubSource(Http.anonymous());
    expect(source.webUrl("BurntSushi/ripgrep")).toBe("https://github.com/BurntSushi/ripgrep");
  });

  it("validates repository ids before building api urls", async () => {
    vi.stubEnv("KETCH_GITHUB_API", "");
    const source = new GitHubSource(Http.anonymous());
    await expect(
      source.listReleases("https://attacker.invalid/x", defaultListOpts()),
    ).rejects.toThrow();
  });
});

describe("urlencodePathSegment", () => {
  it("encodes release tags as one path segment", () => {
    expect(urlencodePathSegment("release/v1 beta?")).toBe("release%2Fv1%20beta%3F");
  });
});
