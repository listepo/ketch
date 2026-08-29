/**
 * The GitHub releases source.
 *
 * Works unauthenticated; a token only raises the rate limit and unlocks
 * private repositories.
 */

import process from "node:process";
import { asciiLowercase, validateRepo } from "@ketch/schemas";
import { KetchError } from "../errors.ts";
import type { Http } from "../http.ts";
import type { Release, ReleaseAsset, SourceInfo, VersionSpec } from "../model.ts";
import { sha256Checksum, Version } from "../model.ts";
import { isSidecar } from "../platform/platform.ts";
import type { ProgressSink } from "../progress.ts";
import type { ListOpts, Source } from "./source.ts";
import { optsFor, pick } from "./source.ts";

/** Override for GitHub Enterprise, via `KETCH_GITHUB_API`. */
export const DEFAULT_API = "https://api.github.com";

/**
 * How many checksum files one release may cost us in requests.
 *
 * A release with fifty assets would otherwise mean fifty extra round trips
 * before the first byte of the download.
 */
const MAX_CHECKSUM_FETCHES = 12;

export class GitHubSource implements Source {
  readonly scheme = "github";
  private readonly http: Http;
  private readonly api: string;

  constructor(http: Http) {
    this.http = http;
    const envApi = process.env["KETCH_GITHUB_API"]?.trim();
    const api = envApi !== undefined && envApi !== "" ? envApi : DEFAULT_API;
    this.api = trimEndMatches(api, "/");
  }

  private repoUrl(id: string, suffix: string): string {
    let repo: string;
    try {
      repo = validateRepo("GitHub repository", id);
    } catch (cause) {
      throw new KetchError({ kind: "config", text: asError(cause).message });
    }
    return `${this.api}/repos/${repo}${suffix}`;
  }

  async describe(id: string): Promise<SourceInfo | null> {
    const repo = await this.http.getJsonOpt<GhRepo>(this.repoUrl(id, ""), true);
    return repo === null ? null : sourceInfoFromRepo(repo);
  }

  async listReleases(id: string, opts: ListOpts): Promise<Release[]> {
    const perPage = clamp(opts.limit, 1, 100);
    const url = this.repoUrl(id, `/releases?per_page=${perPage}`);
    const raw = await this.http.getJson<GhRelease[]>(url, true);

    let releases = raw.filter((r) => !(r.draft ?? false)).map(releaseFromGh);

    // Prereleases are dropped only when there is something stable to drop
    // them in favour of; plenty of projects have never cut a stable tag,
    // and `pick` handles that fallback if the list still holds them.
    if (!opts.includePrerelease && releases.some((r) => !r.prerelease)) {
      releases = releases.filter((r) => !r.prerelease);
    }
    return releases;
  }

  async resolve(id: string, want: VersionSpec, opts: ListOpts): Promise<Release> {
    // Both fast paths are a single request against an endpoint that does the
    // selection server-side; the listing walk is the fallback.
    let direct: string | undefined;
    if (want.kind === "latest" && !opts.includePrerelease) {
      direct = "/releases/latest";
    } else if (want.kind === "exact") {
      direct = `/releases/tags/${urlencodePathSegment(want.value)}`;
    }
    if (direct !== undefined) {
      const found = await this.http.getJsonOpt<GhRelease>(this.repoUrl(id, direct), true);
      if (found !== null && !(found.draft ?? false)) {
        return releaseFromGh(found);
      }
    }
    const widened = optsFor(want, opts);
    const releases = await this.listReleases(id, widened);
    return pick(id, releases, want, widened);
  }

  async checksums(_id: string, release: Release, wanted: string): Promise<Map<string, string>> {
    const out = new Map<string, string>();

    // Whatever the API already told us costs nothing.
    for (const asset of release.assets) {
      if (asset.digest !== null) {
        out.set(asset.name, asset.digest.hex);
      }
    }

    // The sidecar for the asset actually being installed goes first. With a
    // cap on how many are worth fetching, the one file that decides this
    // install must never be the one left out — and a release can easily
    // publish thirty sidecars with ours near the end.
    const wantedSidecar = `${wanted}.sha256`;
    const candidates = release.assets.toSorted((a, b) => {
      const aFirst = a.name === wantedSidecar ? 0 : 1;
      const bFirst = b.name === wantedSidecar ? 0 : 1;
      return aFirst - bFirst;
    });

    let fetches = 0;
    for (const asset of candidates) {
      const sidecar = asset.name.endsWith(".sha256");
      if (!sidecar && !isAggregateChecksumFile(asset.name)) {
        continue;
      }
      // Aggregates count too: the heuristic that spots them is a name
      // match, and an unbounded number of name matches is an unbounded
      // number of downloads.
      if (fetches >= MAX_CHECKSUM_FETCHES) {
        break;
      }
      fetches += 1;
      // A checksum file that will not download is not a reason to fail the
      // install; it just means we fall back to whatever else we have.
      let body: string;
      try {
        body = await this.http.getText(asset.url, false);
      } catch {
        continue;
      }
      if (sidecar) {
        const target = trimEndMatches(asset.name, ".sha256");
        const parsed = parseChecksumFile(body);
        let hex = parsed.values().next().value;
        if (hex === undefined) {
          const first = body.trim().split(/\s+/)[0] ?? "";
          hex = isSha256(first) ? asciiLowercase(first) : undefined;
        }
        if (hex !== undefined && !out.has(target)) {
          out.set(target, hex);
        }
      } else {
        for (const [name, hex] of parseChecksumFile(body)) {
          if (!out.has(name)) {
            out.set(name, hex);
          }
        }
      }
    }
    return out;
  }

  async download(asset: ReleaseAsset, dest: string, progress: ProgressSink): Promise<string> {
    // Anonymous on purpose: see the note on `assetFromGh`.
    return this.http.download(asset.url, dest, asset.headers, false, progress);
  }

  async search(query: string, limit: number): Promise<SourceInfo[]> {
    const trimmed = query.trim();
    if (trimmed === "") {
      return [];
    }
    const url = `${this.api}/search/repositories?q=${urlencode(trimmed)}&per_page=${clamp(limit, 1, 100)}`;
    const found = await this.http.getJson<GhSearch>(url, true);
    return (found.items ?? []).map(sourceInfoFromRepo);
  }

  webUrl(id: string): string | null {
    let repo: string;
    try {
      repo = validateRepo("GitHub repository", id);
    } catch {
      return null;
    }
    const host = this.api.startsWith("https://api.")
      ? `https://${this.api.slice("https://api.".length)}`
      : trimEndMatches(this.api, "/api/v3");
    return `${host}/${repo}`;
  }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

interface GhAsset {
  name: string;
  browser_download_url: string;
  size?: number;
  content_type?: string | null;
  /** Present on newer releases as `sha256:<hex>`. */
  digest?: string | null;
}

interface GhRelease {
  tag_name: string;
  name?: string | null;
  prerelease?: boolean;
  draft?: boolean;
  published_at?: string | null;
  body?: string | null;
  assets?: GhAsset[];
}

interface GhLicense {
  spdx_id?: string | null;
}

interface GhRepo {
  full_name: string;
  description?: string | null;
  homepage?: string | null;
  stargazers_count?: number | null;
  archived?: boolean;
  license?: GhLicense | null;
}

interface GhSearch {
  items?: GhRepo[];
}

function assetFromGh(asset: GhAsset): ReleaseAsset {
  const digest = asset.digest ?? null;
  const hex = digest === null ? null : parseDigest(digest);
  return {
    name: asset.name,
    // Deliberately the public URL rather than the API one: the API
    // redirects to a different host, and following that redirect with an
    // Authorization header would hand the token to a CDN.
    url: asset.browser_download_url,
    size: asset.size ?? 0,
    content_type: asset.content_type ?? null,
    digest: hex === null ? null : sha256Checksum(hex),
    headers: {},
  };
}

function releaseFromGh(release: GhRelease): Release {
  // The tag is the authority; `name` is often decorative ("July build").
  return {
    version: Version.parse(release.tag_name),
    tag: release.tag_name,
    prerelease: release.prerelease ?? false,
    draft: release.draft ?? false,
    published_at: release.published_at ?? null,
    notes: release.body ?? release.name ?? null,
    assets: (release.assets ?? []).map(assetFromGh),
  };
}

function sourceInfoFromRepo(repo: GhRepo): SourceInfo {
  const lastSlash = repo.full_name.lastIndexOf("/");
  const name = lastSlash === -1 ? repo.full_name : repo.full_name.slice(lastSlash + 1);
  const homepage = repo.homepage;
  return {
    id: repo.full_name,
    name,
    description: repo.description ?? null,
    homepage:
      homepage !== undefined && homepage !== null && homepage.trim() !== "" ? homepage : null,
    stars: repo.stargazers_count ?? null,
    license: repo.license?.spdx_id ?? null,
    archived: repo.archived ?? false,
  };
}

// ---------------------------------------------------------------------------
// Checksums
// ---------------------------------------------------------------------------

/**
 * Container extensions an aggregate checksum list never has. It is a text
 * file; anything packaged or signed is a different artefact that happens to
 * mention checksums in its name.
 */
const NOT_A_CHECKSUM_LIST: readonly string[] = [
  ".tar",
  ".gz",
  ".tgz",
  ".xz",
  ".txz",
  ".bz2",
  ".zip",
  ".dmg",
  ".pkg",
  ".exe",
  ".jar",
  ".7z",
];

/**
 * Names that hold checksums for several assets at once.
 *
 * Matching on the name alone is unavoidable — nothing else distinguishes the
 * file before it is downloaded — so the negative half matters as much as the
 * positive one. `checksum-verifier-darwin-arm64.tar.gz` and
 * `checksums.txt.sig` both contain "checksum" and neither is a list of them;
 * fetching either as text costs a whole asset transfer and yields nothing.
 */
export function isAggregateChecksumFile(name: string): boolean {
  const lower = asciiLowercase(name);
  const claimsChecksums =
    lower.includes("sha256sum") ||
    lower.includes("sha256_sums") ||
    lower.includes("checksum") ||
    lower === "sums.txt";
  return (
    claimsChecksums && !isSidecar(lower) && !NOT_A_CHECKSUM_LIST.some((s) => lower.endsWith(s))
  );
}

function isSha256(hex: string): boolean {
  return hex.length === 64 && /^[0-9a-fA-F]+$/.test(hex);
}

/** Pull the hex digest out of a GitHub API `digest` field (`sha256:<hex>`). */
export function parseDigest(raw: string): string | null {
  if (!raw.startsWith("sha256:")) {
    return null;
  }
  const hex = raw.slice("sha256:".length).trim();
  return isSha256(hex) ? asciiLowercase(hex) : null;
}

/**
 * Parse the `sha256sum` output format: `<hex><space><space|*><name>`.
 *
 * Names may carry a leading `./` or a directory prefix, so only the file name
 * is kept — that is what the asset list is keyed by. Returned sorted by name
 * (like the Rust port's `BTreeMap`), so taking the first entry is
 * deterministic rather than "whichever line happened to come first".
 */
export function parseChecksumFile(body: string): Map<string, string> {
  const collected = new Map<string, string>();
  for (const rawLine of body.split(/\r\n|\n/)) {
    const line = rawLine.trim();
    if (line === "" || line.startsWith("#")) {
      continue;
    }
    const parts = line.split(/\s+/);
    const hex = parts[0];
    const rawName = parts[1];
    if (hex === undefined || rawName === undefined || !isSha256(hex)) {
      continue;
    }
    const stripped = trimStartMatches(trimStartMatches(rawName, "*"), "./");
    const lastSlash = stripped.lastIndexOf("/");
    const name = lastSlash === -1 ? stripped : stripped.slice(lastSlash + 1);
    collected.set(name, asciiLowercase(hex));
  }
  return new Map([...collected].toSorted(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)));
}

// ---------------------------------------------------------------------------
// Small string helpers
// ---------------------------------------------------------------------------

/** Strip every leading occurrence of `prefix`, like Rust `trim_start_matches`. */
function trimStartMatches(text: string, prefix: string): string {
  let out = text;
  while (out.startsWith(prefix)) {
    out = out.slice(prefix.length);
  }
  return out;
}

/** Strip every trailing occurrence of `suffix`, like Rust `trim_end_matches`. */
function trimEndMatches(text: string, suffix: string): string {
  let out = text;
  while (out.endsWith(suffix)) {
    out = out.slice(0, -suffix.length);
  }
  return out;
}

function clamp(n: number, min: number, max: number): number {
  return Math.min(Math.max(n, min), max);
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

const UNRESERVED = /[A-Za-z0-9\-_.~]/;

/**
 * Percent-encode a search query. Only the handful of characters that
 * actually appear in package searches need escaping, so this stays a few
 * lines instead of a dependency.
 */
function urlencode(raw: string): string {
  let out = "";
  for (const byte of new TextEncoder().encode(raw)) {
    const ch = String.fromCharCode(byte);
    if (UNRESERVED.test(ch)) {
      out += ch;
    } else if (byte === 0x20) {
      out += "+";
    } else {
      out += `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
    }
  }
  return out;
}

/**
 * Percent-encode a value placed in one URL path segment. Unlike a query,
 * spaces are `%20` and slashes must not become path separators: release tags
 * are allowed to contain both.
 */
export function urlencodePathSegment(raw: string): string {
  let out = "";
  for (const byte of new TextEncoder().encode(raw)) {
    const ch = String.fromCharCode(byte);
    out += UNRESERVED.test(ch) ? ch : `%${byte.toString(16).toUpperCase().padStart(2, "0")}`;
  }
  return out;
}
