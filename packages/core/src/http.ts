/**
 * HTTP access.
 *
 * Fetch-based, one client per process (well, one per `Config`/`Http`
 * instance, which is the same thing in practice). Downloads hash while they
 * stream to disk, so verification costs no extra read of the file.
 */

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import type { Config } from "./config.ts";
import { KetchError } from "./errors.ts";
import type { ProgressSink } from "./progress.ts";

/**
 * Cap on API response bodies. Release lists are small; anything larger is a
 * sign we are being fed something we should not buffer.
 */
const MAX_API_BODY = 32 * 1024 * 1024;

/**
 * `fetch` has no separate connect/read phases the way `ureq` does — one
 * `AbortSignal` timeout covers a request from connect through the body being
 * fully read.
 *
 * ponytail: this is a single total-duration timeout, not `ureq`'s
 * inactivity/idle timeout — a download that is still trickling data after
 * `READ_TIMEOUT_MS` is aborted here even though the Rust client would let it
 * keep going. Upgrade to a per-chunk idle-reset timer (reset on every read)
 * if large-but-slow downloads start timing out in practice.
 */
const READ_TIMEOUT_MS = 120_000;

/** `ketch/<version>`, read once from this package's own `package.json`. */
export const USER_AGENT = `ketch/${corePackageVersion()}`;

/** GET and deserialize JSON. */
export class Http {
  private readonly token: string | null;

  /** Reads the GitHub token, if any, off `cfg`. Omit `cfg` for `anonymous`. */
  constructor(cfg?: Config) {
    this.token = cfg?.githubToken ?? null;
  }

  /** Without a token, for hosts that are not GitHub. */
  static anonymous(): Http {
    return new Http();
  }

  // Part of the public surface, with no caller in the tree yet.
  hasToken(): boolean {
    return this.token !== null;
  }

  private buildHeaders(accept: string, authed: boolean): Record<string, string> {
    const headers: Record<string, string> = { Accept: accept, "User-Agent": USER_AGENT };
    if (authed && this.token !== null) {
      headers["Authorization"] = `Bearer ${this.token}`;
      headers["X-GitHub-Api-Version"] = "2022-11-28";
    }
    return headers;
  }

  /** Issue the GET and turn any failure into our error type. */
  private async request(
    url: string,
    accept: string,
    authed: boolean,
    extraHeaders?: Record<string, string>,
  ): Promise<Response> {
    const headers = this.buildHeaders(accept, authed);
    if (extraHeaders !== undefined) {
      for (const [key, value] of Object.entries(extraHeaders)) {
        headers[key] = value;
      }
    }
    let res: Response;
    try {
      res = await fetch(url, { headers, signal: AbortSignal.timeout(READ_TIMEOUT_MS) });
    } catch (cause) {
      throw new KetchError({ kind: "network", url, cause: asError(cause) });
    }
    if (!res.ok) {
      throw await classify(url, res);
    }
    return res;
  }

  /** GET and deserialize JSON. */
  async getJson<T>(url: string, authed: boolean): Promise<T> {
    const body = await this.getString(url, "application/vnd.github+json", authed);
    try {
      return JSON.parse(body) as T;
    } catch (cause) {
      throw KetchError.parse(url, asError(cause).message);
    }
  }

  /** GET a text body (checksum files, plain manifests). */
  async getText(url: string, authed: boolean): Promise<string> {
    return this.getString(url, "text/plain, */*", authed);
  }

  private async getString(url: string, accept: string, authed: boolean): Promise<string> {
    const res = await this.request(url, accept, authed);
    if (res.body === null) {
      return "";
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let out = "";
    let total = 0;
    try {
      for (;;) {
        const step = await asIo(url, () => reader.read());
        if (step.done) {
          break;
        }
        total += step.value.byteLength;
        if (total > MAX_API_BODY) {
          const keep = step.value.byteLength - (total - MAX_API_BODY);
          out += decoder.decode(step.value.subarray(0, Math.max(keep, 0)), { stream: true });
          break;
        }
        out += decoder.decode(step.value, { stream: true });
      }
    } finally {
      reader.releaseLock();
    }
    return out + decoder.decode();
  }

  /**
   * Like `getJson`, but `null` on 404 instead of an error. Used where a
   * missing resource is an ordinary answer rather than a failure.
   */
  async getJsonOpt<T>(url: string, authed: boolean): Promise<T | null> {
    try {
      return await this.getJson<T>(url, authed);
    } catch (cause) {
      if (cause instanceof KetchError && cause.data.kind === "http" && cause.data.status === 404) {
        return null;
      }
      throw cause;
    }
  }

  /**
   * Stream a URL to `dest`, hashing as it goes.
   *
   * Returns the lowercase hex SHA-256 of the bytes written. The file is
   * written in full or not at all: we stage next to the destination and
   * rename, so an interrupted download never looks like a complete one.
   */
  async download(
    url: string,
    dest: string,
    headers: Record<string, string>,
    authed: boolean,
    progress: ProgressSink,
  ): Promise<string> {
    const res = await this.request(url, "application/octet-stream", authed, headers);

    const total = contentLengthOf(res);
    const label = path.basename(dest) || "download";
    progress.start(total, label);

    const parent = path.dirname(dest);
    try {
      fs.mkdirSync(parent, { recursive: true });
    } catch (cause) {
      throw KetchError.io(parent, asError(cause));
    }

    const staged = stagedPathFor(dest);
    const hasher = crypto.createHash("sha256");
    let written = 0;

    const handle = await asIo(staged, () => fs.promises.open(staged, "w"));
    try {
      if (res.body !== null) {
        const reader = res.body.getReader();
        try {
          // oxlint-disable-next-line no-await-in-loop -- a download is a stream:
          // the next chunk does not exist until this one has been read, and the
          // hash is only correct if they are written in order.
          for (;;) {
            const step = await asIo(url, () => reader.read());
            if (step.done) {
              break;
            }
            hasher.update(step.value);
            await asIo(staged, () => handle.write(step.value));
            written += step.value.byteLength;
            progress.advance(step.value.byteLength);
          }
        } finally {
          reader.releaseLock();
        }
      }
    } catch (cause) {
      await handle.close().catch(() => {});
      await fs.promises.unlink(staged).catch(() => {});
      throw cause;
    }
    await handle.close();

    // A truncated transfer that still returned 200 would otherwise be
    // indistinguishable from success until the checksum stage.
    if (total !== null && written !== total) {
      await fs.promises.unlink(staged).catch(() => {});
      throw KetchError.msg(`download of ${label} ended early: got ${written} of ${total} bytes`);
    }

    await asIo(dest, () => fs.promises.rename(staged, dest));
    progress.finish(`${label} (${formatBytes(written)})`);
    return hasher.digest("hex");
  }
}

/**
 * Turn a failed fetch response into our error type, keeping the server's own
 * message when it sent one — GitHub's bodies explain rate limits precisely.
 */
async function classify(url: string, res: Response): Promise<KetchError> {
  let detail: string | null = null;
  try {
    detail = extractMessage(await res.text());
  } catch {
    detail = null;
  }
  // `extractMessage` returns `Some("")` for an explicit `"message":""` field;
  // an empty detail line is worse than none, so it is dropped here instead.
  return new KetchError({ kind: "http", url, status: res.status, detail: detail || null });
}

/** Pull `message` out of a JSON error body, else return a trimmed snippet. */
export function extractMessage(body: string): string | null {
  try {
    const value: unknown = JSON.parse(body);
    if (typeof value === "object" && value !== null && "message" in value) {
      const message = (value as Record<string, unknown>)["message"];
      if (typeof message === "string") {
        return message;
      }
    }
  } catch {
    // Not JSON — fall through to the snippet form.
  }
  const snippet = body.trim();
  return snippet === "" ? null : truncate(snippet, 200);
}

function truncate(text: string, width: number): string {
  const chars = Array.from(text);
  if (chars.length <= width) {
    return text;
  }
  return `${chars.slice(0, Math.max(width - 1, 0)).join("")}…`;
}

/** Hash a file that is already on disk. */
export async function sha256File(filePath: string): Promise<string> {
  const hasher = crypto.createHash("sha256");
  try {
    for await (const chunk of fs.createReadStream(filePath)) {
      hasher.update(chunk as Buffer);
    }
  } catch (cause) {
    throw KetchError.io(filePath, asError(cause));
  }
  return hasher.digest("hex");
}

/** Run one fallible IO step, wrapping any failure as `KetchError.io(path, …)`. */
async function asIo<T>(ioPath: string, op: () => Promise<T>): Promise<T> {
  try {
    return await op();
  } catch (cause) {
    throw KetchError.io(ioPath, asError(cause));
  }
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

function contentLengthOf(res: Response): number | null {
  const raw = res.headers.get("content-length");
  if (raw === null) {
    return null;
  }
  const n = Number(raw);
  return Number.isInteger(n) && n > 0 ? n : null;
}

/** A random, hidden staging name next to `dest`, so the final rename is atomic. */
function stagedPathFor(dest: string): string {
  const dir = path.dirname(dest);
  const base = path.basename(dest);
  return path.join(dir, `.${base}.${crypto.randomBytes(6).toString("hex")}.part`);
}

function formatBytes(n: number): string {
  const UNITS = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${UNITS[unit] ?? "TiB"}`;
}

function corePackageVersion(): string {
  try {
    const raw = fs.readFileSync(new URL("../package.json", import.meta.url), "utf8");
    const version = (JSON.parse(raw) as { version?: string }).version;
    return version ?? "0.0.0";
  } catch {
    return "0.0.0";
  }
}
