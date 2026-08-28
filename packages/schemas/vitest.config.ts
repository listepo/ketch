import { defineConfig } from "vitest/config";

// Only the sources: `tsc --build` mirrors the tests into dist/, and running
// those copies would double every test.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
  },
});
