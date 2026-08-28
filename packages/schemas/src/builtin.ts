/**
 * Built-in package registry, compiled into the binary.
 *
 * Port of the Rust `builtin.toml`. An entry is only needed when inference
 * gets it wrong — a repository whose release assets are named unusually,
 * ships an app bundle, or is better known by a short alias than by
 * `owner/repo`. Parsed through the manifest schema at module load, so a bad
 * entry is a ketch bug that fails immediately rather than a package that
 * quietly does not resolve.
 */

import { type Manifest, parseManifest } from "./manifest.ts";

const entries: unknown[] = [
  {
    name: "ripgrep",
    source: "github:BurntSushi/ripgrep",
    description: "Recursively search directories for a regex pattern",
    homepage: "https://github.com/BurntSushi/ripgrep",
    bin: [{ name: "rg" }],
    provides: ["rg"],
  },
];

/** Every built-in package, already validated. */
export const builtinPackages: Manifest[] = entries.map(parseManifest);
