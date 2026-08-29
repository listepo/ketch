/**
 * Trust-boundary guards shared by every schema.
 *
 * Ports of `config::validate_repo`, `config::sanitize_component`,
 * `extract::safe_member_path` and `PackageRef::parse` from the Rust tree.
 * They live here rather than in @ketch/core because manifest and lockfile
 * validation depend on them and schemas must not import core; core re-exports
 * them instead.
 */

/** Lowercase ASCII letters only, matching Rust's `to_ascii_lowercase`. */
export function asciiLowercase(text: string): string {
  return text.replace(/[A-Z]/g, (c) => c.toLowerCase());
}

/** Unicode Cc, matching Rust's `char::is_control`. */
function isControl(c: string): boolean {
  const cp = c.codePointAt(0) ?? 0;
  return cp <= 0x1f || (cp >= 0x7f && cp <= 0x9f);
}

/**
 * Make a string safe to use as one path component. Version tags can legally
 * contain `/` (e.g. `release/1.2`), which would otherwise escape the store.
 */
export function sanitizeComponent(raw: string): string {
  const cleaned = [...raw]
    .map((c) => (c === "/" || c === "\\" || c === ":" || isControl(c) ? "-" : c))
    .join("");
  const trimmed = cleaned.replace(/^[-. ]+/, "").replace(/[-. ]+$/, "");
  return trimmed === "" ? "unknown" : trimmed;
}

/**
 * Accept only `owner/repo`, since it is about to become a URL.
 *
 * A `github:` prefix is tolerated because that is how the same repository is
 * written everywhere else in ketch; the returned form drops it.
 */
export function validateRepo(what: string, raw: string): string {
  let repo = raw.trim();
  // Rust's trim_start_matches strips the prefix repeatedly; keep that.
  while (repo.startsWith("github:")) {
    repo = repo.slice("github:".length);
  }
  const parts = repo.split("/");
  const shaped = parts.length === 2 && parts.every((p) => p !== "");
  const printable = /^[A-Za-z0-9\-_./]*$/.test(repo);
  if (shaped && printable && !repo.includes("..")) {
    return repo;
  }
  throw new Error(`${what} \`${raw}\` is not a GitHub repository; expected \`owner/repo\``);
}

/**
 * Reject relative paths that would write outside the directory they are
 * joined onto — the guard against "zip slip" / tar traversal. Absolute paths
 * and `..` components are refused outright; the result is always a relative
 * path safe to join onto a destination.
 */
export function safeMemberPath(raw: string): string {
  if (raw.startsWith("/")) {
    throw new Error(`refusing archive entry that escapes the target directory: ${raw}`);
  }
  const safe: string[] = [];
  for (const part of raw.split("/")) {
    if (part === "" || part === ".") {
      continue;
    }
    if (part === "..") {
      throw new Error(`refusing archive entry that escapes the target directory: ${raw}`);
    }
    // Windows drive-relative and NTFS stream syntax are rejected too, so
    // archives built on Windows cannot smuggle a path.
    if (part.includes(":") || part.includes("\\")) {
      throw new Error(`refusing archive entry with unsafe name: ${raw}`);
    }
    safe.push(part);
  }
  if (safe.length === 0) {
    throw new Error("refusing archive entry with an empty name");
  }
  return safe.join("/");
}

/** A fully-qualified package location: which source, and an id it understands. */
export interface PackageRef {
  scheme: string;
  id: string;
}

/**
 * Parse `scheme:id` or a bare `owner/repo` (which implies GitHub).
 *
 * A bare word with neither `:` nor `/` is *not* a reference — it is an alias
 * to be resolved against the manifest registry, so this returns `null` for it
 * rather than guessing.
 */
export function parsePackageRef(text: string): PackageRef | null {
  const trimmed = text.trim();
  if (trimmed === "") {
    return null;
  }
  // A scheme is alphanumeric and never contains `/`; this keeps
  // `https://host/x` and `owner/repo` from being read as schemes.
  const colon = trimmed.indexOf(":");
  if (colon !== -1) {
    const scheme = trimmed.slice(0, colon);
    const rest = trimmed.slice(colon + 1);
    const looksLikeScheme = scheme !== "" && /^[A-Za-z0-9-]+$/.test(scheme);
    if (looksLikeScheme && rest !== "") {
      return { scheme: asciiLowercase(scheme), id: rest };
    }
  }
  if (trimmed.includes("/")) {
    return { scheme: "github", id: trimmed };
  }
  return null;
}

/** Base of the published raw URLs that runtime files point at via `$schema`. */
export const SCHEMA_BASE_URL =
  "https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas";

/** The published URL of one generated JSON Schema. */
export function schemaUrl(name: string): string {
  return `${SCHEMA_BASE_URL}/${name}.schema.json`;
}
