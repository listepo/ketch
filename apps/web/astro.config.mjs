// @ts-check
import tailwind from "@tailwindcss/vite";
import { defineConfig } from "astro/config";

// A GitHub Pages project site: everything lives under /ketch/, and the docs
// are a separate Docusaurus build mounted at /ketch/docs/ by the deploy
// workflow. Astro must not try to own that path.
export default defineConfig({
  site: "https://listepo.github.io",
  base: "/ketch",
  trailingSlash: "always",
  build: { format: "directory" },
  vite: { plugins: [tailwind()] },
});
