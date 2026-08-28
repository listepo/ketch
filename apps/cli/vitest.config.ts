import { defineConfig } from "vitest/config";

// Only the sources: `tsc --build` compiles the unit tests and the e2e suite
// into dist/, and running those copies would double every test — and fail,
// since the e2e harness resolves the CLI relative to its own file.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts", "tests/**/*.test.ts"],
  },
});
