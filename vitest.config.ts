import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    projects: ["packages/*", "apps/*"],
    // The e2e suite drives the real CLI against a throwaway root; downloads
    // are served by an offline fixture plugin, so no test needs the network.
    testTimeout: 30_000,
  },
});
