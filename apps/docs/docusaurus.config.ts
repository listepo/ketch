import type { Config } from "@docusaurus/types";
import type * as Preset from "@docusaurus/preset-classic";
import { themes } from "prism-react-renderer";

/**
 * The documentation site.
 *
 * Docs-only: there is no blog and no separate landing page here, because the
 * landing page is the Astro build that gets deployed alongside this one at
 * the site root. This build owns `/ketch/docs/` and nothing above it.
 *
 * Pages come from `docs/` in the repository, copied in by `sync-docs.mjs` so
 * the Markdown that ships with the source is the only copy anyone edits.
 */
const config: Config = {
  title: "ketch",
  tagline: "Catch releases straight from GitHub.",
  favicon: "img/favicon.svg",
  url: "https://listepo.github.io",
  baseUrl: "/ketch/docs/",
  organizationName: "listepo",
  projectName: "ketch",
  trailingSlash: true,
  onBrokenLinks: "throw",
  onBrokenMarkdownLinks: "throw",
  i18n: { defaultLocale: "en", locales: ["en"] },

  presets: [
    [
      "classic",
      {
        docs: {
          routeBasePath: "/",
          sidebarPath: "./sidebars.ts",
          // Straight to the file that is actually the source, not to the
          // generated copy this build reads.
          editUrl: ({ docPath }) =>
            `https://github.com/listepo/ketch/edit/main/${docPath.replace(/^/, "docs/")}`,
        },
        blog: false,
        theme: { customCss: "./src/css/custom.css" },
        sitemap: { changefreq: "weekly", priority: 0.5 },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: { defaultMode: "dark", respectPrefersColorScheme: true },
    navbar: {
      title: "ketch",
      items: [
        { type: "docSidebar", sidebarId: "docs", position: "left", label: "Docs" },
        // The landing page is the Astro build deployed above this one, so it can
        // only be linked absolutely — it does not exist in this build.
        { href: "https://listepo.github.io/ketch/", label: "Home", position: "right" },
        { href: "https://github.com/listepo/ketch", label: "GitHub", position: "right" },
      ],
    },
    footer: {
      style: "dark",
      links: [
        {
          title: "Docs",
          items: [
            { label: "Manifests", to: "/manifests" },
            { label: "Registry", to: "/registry" },
            { label: "Plugins", to: "/plugins" },
            { label: "Lockfile", to: "/lockfile" },
          ],
        },
        {
          title: "Project",
          items: [
            { label: "GitHub", href: "https://github.com/listepo/ketch" },
            { label: "Registry repo", href: "https://github.com/listepo/ketch-registry" },
            { label: "Roadmap", to: "/roadmap" },
          ],
        },
      ],
      copyright: "MIT licensed.",
    },
    prism: {
      theme: themes.github,
      darkTheme: themes.dracula,
      additionalLanguages: ["bash", "json", "toml"],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
