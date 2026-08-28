/** Shape tests for config.json; value parsing is tested with the core loader. */

import { describe, expect, it } from "vitest";
import { configFileSchema } from "./config.ts";

describe("configFileSchema", () => {
  it("a partial file is valid because every field is optional", () => {
    expect(() => configFileSchema.parse({})).not.toThrow();
    expect(configFileSchema.parse({ prerelease: true }).prerelease).toBe(true);
  });

  it("an unknown key is refused rather than ignored", () => {
    expect(() => configFileSchema.parse({ prerelase: true })).toThrow();
  });

  it("jobs must be a whole number", () => {
    expect(configFileSchema.parse({ jobs: 8 }).jobs).toBe(8);
    expect(() => configFileSchema.parse({ jobs: 2.5 })).toThrow();
    expect(() => configFileSchema.parse({ jobs: -1 })).toThrow();
  });

  it("every documented key parses", () => {
    const full = configFileSchema.parse({
      $schema: "https://x",
      root: "/tmp/ketch",
      apps_dir: "/Applications",
      github_token: "token",
      prerelease: false,
      allow_emulation: true,
      link_apps: false,
      require_checksums: false,
      strip_quarantine: true,
      self_repo: "listepo/ketch",
      registry: "listepo/ketch-registry",
      jobs: 4,
      log_level: "info",
      log_format: "text",
    });
    expect(full.registry).toBe("listepo/ketch-registry");
  });
});
