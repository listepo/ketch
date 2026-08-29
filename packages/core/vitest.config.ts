import { defineConfig } from "vitest/config";
export default defineConfig({
  // Only the sources: `tsc --build` also compiles the colocated tests into
  // dist/, and running those copies would double every suite.
  test: { include: ["src/**/*.test.ts"] },
});
