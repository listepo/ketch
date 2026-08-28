/**
 * Reading a package's changelog, from either of the two places one lives.
 *
 * Projects record what changed in one of two ways, and most do both
 * inconsistently: a `CHANGELOG.md` committed to the repository and shipped
 * inside the release, and the notes attached to the release itself. ketch
 * already has the second — every `Release` carries `notes` — and the first is
 * sitting in the store next to the binary that was installed from it.
 *
 * The file is preferred when it exists, because reading it needs no network
 * and it is the version the user actually has. Falling back to release notes
 * covers every project that keeps its history only on the forge.
 *
 * Nothing here trusts the file's structure. A changelog is prose someone else
 * wrote; the section matcher takes what it recognises and says plainly when it
 * recognises nothing, rather than guessing a range and printing the wrong
 * release's history.
 */

import fs from "node:fs";
import path from "node:path";
import { asciiLowercase } from "@ketch/schemas";
import { KetchError } from "./errors.ts";

/** File names worth looking for, most conventional first. */
const NAMES = [
  "CHANGELOG.md",
  "CHANGELOG",
  "CHANGELOG.txt",
  "CHANGES.md",
  "CHANGES",
  "HISTORY.md",
  "NEWS.md",
  "RELEASES.md",
  "RELEASE_NOTES.md",
] as const;

/** Directories a project might tuck it into, relative to the payload root. */
const SUBDIRS = ["", "doc", "docs", "share/doc"] as const;

/** Where a changelog came from, so the caller can say so. */
export type Origin =
  /** A file inside the installed payload. */
  | { kind: "file"; path: string }
  /** Notes published with the release. */
  | { kind: "release" };

/** One changelog, ready to print. */
export interface Entry {
  origin: Origin;
  /** The heading the section was found under, when there was one. */
  heading: string | null;
  body: string;
}

/**
 * Find a changelog file inside an installed payload.
 *
 * Only looks a fixed set of places rather than walking the tree: a release
 * payload can contain thousands of files, and the ones that ship a changelog
 * put it where everyone else does.
 */
export function findFile(prefix: string): string | null {
  for (const dir of SUBDIRS) {
    const base = dir === "" ? prefix : path.join(prefix, dir);
    for (const name of NAMES) {
      const file = path.join(base, name);
      if (isFile(file)) {
        return file;
      }
    }
    // Some projects ship `share/doc/<pkg>/CHANGELOG.md`, one level deeper
    // than the directory itself. One level, not a walk.
    let entries: string[];
    try {
      entries = fs.readdirSync(base);
    } catch {
      continue;
    }
    for (const entry of entries) {
      const sub = path.join(base, entry);
      if (!isDir(sub)) {
        continue;
      }
      for (const name of NAMES) {
        const file = path.join(sub, name);
        if (isFile(file)) {
          return file;
        }
      }
    }
  }
  return null;
}

/** `Path::is_file`: stat, following symlinks, and swallow every error. */
function isFile(file: string): boolean {
  try {
    return fs.statSync(file, { throwIfNoEntry: false })?.isFile() ?? false;
  } catch {
    return false;
  }
}

/** `Path::is_dir`, with the same symlink-following stat. */
function isDir(dir: string): boolean {
  try {
    return fs.statSync(dir, { throwIfNoEntry: false })?.isDirectory() ?? false;
  } catch {
    return false;
  }
}

/**
 * Read a changelog file and take the section for `version`, or the whole file
 * when no section matches.
 */
export function fromFile(file: string, version: string | null): Entry {
  let raw: string;
  try {
    raw = fs.readFileSync(file, "utf8");
  } catch (cause) {
    throw KetchError.io(file, asError(cause));
  }
  const text = sanitize(raw);
  const found = version === null ? null : section(text, version);
  return {
    origin: { kind: "file", path: file },
    heading: found === null ? null : found.heading,
    body: found === null ? text.trim() : found.body,
  };
}

/** The notes a release published, if it published any. */
export function fromRelease(notes: string | null): Entry | null {
  if (notes === null) {
    return null;
  }
  const body = sanitize(notes.trim());
  if (body === "") {
    return null;
  }
  return { origin: { kind: "release" }, heading: null, body };
}

/**
 * Strip what a changelog has no business containing.
 *
 * This is a client app's bytes on their way to a terminal. An escape sequence
 * in one can rewrite lines already on screen or drive the terminal itself, and
 * a bidi override can make a line read as the reverse of what it says. Neither
 * belongs in prose, so both are dropped where the text enters rather than
 * where it is printed, which would leave every caller to remember.
 *
 * Exported for its tests; callers get it through `fromFile`/`fromRelease`.
 */
export function sanitize(text: string): string {
  let out = "";
  for (const c of text) {
    if (c === "\n" || c === "\t") {
      out += c;
      continue;
    }
    const cp = c.codePointAt(0) ?? 0;
    // `char::is_control` is C0, DEL and C1 — including U+009B, which some
    // terminals still take as a CSI introducer on its own.
    const control = cp < 0x20 || (cp >= 0x7f && cp <= 0x9f);
    const bidi = (cp >= 0x202a && cp <= 0x202e) || (cp >= 0x2066 && cp <= 0x2069);
    if (!control && !bidi) {
      out += c;
    }
  }
  return out;
}

/**
 * The block of a Markdown changelog belonging to one version.
 *
 * Returns the heading it matched and the lines under it, up to the next
 * heading at the same level or higher. `null` when no heading names this
 * version — a caller that printed the whole file in that case is being
 * honest; one that printed a guessed range would not be.
 */
export function section(text: string, version: string): { heading: string; body: string } | null {
  const wanted = normalise(version);
  if (wanted === "") {
    return null;
  }
  // `str::lines`: split on `\n`, dropping one trailing `\r` per line.
  const lines = text.split("\n").map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
  const heads = headings(lines);
  const at = heads.findIndex((h) => headingNames(h.text, wanted));
  if (at === -1) {
    return null;
  }

  const head = heads[at];
  if (head === undefined) {
    return null; // unreachable: `at` came from findIndex on `heads`
  }
  const next = heads.slice(at + 1).find((h) => h.level <= head.level);
  const end = next === undefined ? lines.length : next.start;

  const body = lines.slice(head.body, end).join("\n").trim();
  return { heading: head.text, body };
}

/** One heading: where its text is, where its body starts, and how deep it sits. */
interface Heading {
  start: number;
  body: number;
  level: number;
  text: string;
}

/** Every heading in the file, in order. */
function headings(lines: readonly string[]): Heading[] {
  const out: Heading[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    const level = atxLevel(line);
    if (level !== null) {
      out.push({ start: i, body: i + 1, level, text: line.trim() });
    } else if (underlined(line, lines[i + 1] ?? "")) {
      out.push({ start: i, body: i + 2, level: 1, text: line.trim() });
      i += 1;
    }
    i += 1;
  }
  return out;
}

/**
 * `##` and friends.
 *
 * Exported for its tests; everything else reaches it through `section`.
 */
export function atxLevel(line: string): number | null {
  let hashes = 0;
  while (hashes < line.length && line[hashes] === "#") {
    hashes += 1;
  }
  // `#####hi` is not a heading; `## 1.2.3` is.
  const after = line[hashes];
  if (hashes >= 1 && hashes <= 6 && (after === undefined || after === " ")) {
    return hashes;
  }
  return null;
}

/**
 * A Setext heading: text with `====` under it, which is how ripgrep and every
 * changelog in its lineage marks a release.
 *
 * Only `=`. The other Setext underline is `---`, and a changelog that puts a
 * `---` rule between releases — a common habit — would have every section
 * truncated at the line above the rule. Missing those headings costs a
 * fallback to the release notes; matching them wrongly costs a silently
 * half-printed release.
 */
function underlined(text: string, next: string): boolean {
  const rule = next.trimEnd();
  return text.trim() !== "" && atxLevel(text) === null && rule.length >= 2 && /^=+$/.test(rule);
}

/**
 * True when a heading names this version.
 *
 * Changelog headings are written every way there is — `## [1.2.3] - 2024-05-01`,
 * `## v1.2.3`, `## 1.2.3 (2024-05-01)`, `## Release 1.2.3` — so the version is
 * matched as a whole token anywhere in the heading rather than by shape.
 */
function headingNames(line: string, wanted: string): boolean {
  return line
    .replace(/^#+/, "")
    .split(/[^0-9A-Za-z.+-]/)
    .some((token) => normalise(token) === wanted);
}

/**
 * Strip the `v` a tag carries and a release's surrounding punctuation, so
 * `v1.2.3`, `[1.2.3]` and `1.2.3` are one version.
 */
function normalise(text: string): string {
  const trimmed = text
    .trim()
    .replace(/^[^0-9A-Za-z]+/, "")
    .replace(/[^0-9A-Za-z]+$/, "");
  const rest =
    trimmed.startsWith("v") && /^[0-9]/.test(trimmed.slice(1)) ? trimmed.slice(1) : trimmed;
  return asciiLowercase(rest);
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}
