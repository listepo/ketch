/**
 * Error type shared by every module.
 *
 * One discriminated union keeps the plumbing honest: any module may construct
 * any variant, and the CLI entry point renders them uniformly. Variants carry
 * the data needed to write a message a user can act on — never a bare string
 * where a path or URL exists.
 */

/** Every failure ketch can report, mirroring the Rust `Error` enum variants. */
export type ErrorData =
  | { kind: "msg"; text: string }
  | { kind: "io"; path: string; cause: Error }
  | { kind: "plain_io"; cause: Error }
  | { kind: "http"; url: string; status: number; detail: string | null }
  | { kind: "network"; url: string; cause: Error }
  | { kind: "parse"; what: string; detail: string }
  | { kind: "unknown_scheme"; scheme: string }
  | { kind: "not_installed"; name: string }
  | { kind: "already_installed"; name: string; version: string }
  | { kind: "pinned"; name: string; version: string }
  | { kind: "no_release"; id: string }
  | { kind: "no_compatible_asset"; id: string; tag: string; target: string }
  | { kind: "checksum_mismatch"; name: string; expected: string; actual: string }
  | { kind: "checksum_missing"; name: string }
  | { kind: "empty_payload"; path: string }
  | { kind: "unsupported_archive"; path: string }
  | { kind: "command"; cmd: string; status: string; stderr: string }
  | { kind: "plugin"; name: string; detail: string }
  | { kind: "config"; text: string }
  | { kind: "locked"; detail: string };

/** The headline message. Must not drift from the Rust `Display` impls. */
function render(data: ErrorData): string {
  switch (data.kind) {
    case "msg":
      return data.text;
    case "io":
      return `${data.path}: ${data.cause.message}`;
    case "plain_io":
      return data.cause.message;
    case "http":
      return `HTTP ${data.status} from ${data.url}`;
    case "network":
      return `network error requesting ${data.url}`;
    case "parse":
      return `could not parse ${data.what}: ${data.detail}`;
    case "unknown_scheme":
      return `no source is registered for scheme \`${data.scheme}\``;
    case "not_installed":
      return `\`${data.name}\` is not installed`;
    case "already_installed":
      return `\`${data.name}\` ${data.version} is already installed`;
    case "pinned":
      return `\`${data.name}\` is pinned to ${data.version}`;
    case "no_release":
      return `no release found for \`${data.id}\``;
    case "no_compatible_asset":
      return `release \`${data.tag}\` of \`${data.id}\` has no asset for ${data.target}`;
    case "checksum_mismatch":
      return `checksum mismatch for ${data.name}`;
    case "checksum_missing":
      return `no published checksum for ${data.name}`;
    case "empty_payload":
      return `no installable files found in ${data.path}`;
    case "unsupported_archive":
      return `unsupported archive format: ${data.path}`;
    case "command":
      return `\`${data.cmd}\` failed (${data.status})`;
    case "plugin":
      return `plugin \`${data.name}\`: ${data.detail}`;
    case "config":
      return data.text;
    case "locked":
      return `another ketch process holds the lock (${data.detail})`;
  }
}

/** The one error class every module throws. */
export class KetchError extends Error {
  readonly data: ErrorData;

  constructor(data: ErrorData) {
    super(render(data));
    this.name = "KetchError";
    this.data = data;
    if (data.kind === "io" || data.kind === "plain_io" || data.kind === "network") {
      this.cause = data.cause;
    }
  }

  get kind(): ErrorData["kind"] {
    return this.data.kind;
  }

  static msg(text: string): KetchError {
    return new KetchError({ kind: "msg", text });
  }

  static io(path: string, cause: Error): KetchError {
    return new KetchError({ kind: "io", path, cause });
  }

  static parse(what: string, detail: string): KetchError {
    return new KetchError({ kind: "parse", what, detail });
  }

  /**
   * Extra lines shown under the headline message. Keeps `render` short while
   * still surfacing server bodies, diffs and stderr.
   */
  details(): string[] {
    const d = this.data;
    switch (d.kind) {
      case "http":
        return d.detail === null ? [] : [d.detail];
      case "command":
        return d.stderr.trim() === "" ? [] : d.stderr.trim().split(/\r?\n/);
      case "checksum_mismatch":
        return [`expected ${d.expected}`, `actual   ${d.actual}`];
      default:
        return [];
    }
  }

  /** A short, actionable next step, when one exists. */
  hint(): string | null {
    const d = this.data;
    switch (d.kind) {
      case "http":
        if (d.status === 403 || d.status === 429) {
          return "GitHub rate limit. Set GITHUB_TOKEN (or `gh auth token`) to raise it.";
        }
        if (d.status === 404) {
          return "Check the owner/repo spelling, or the repo may be private.";
        }
        return null;
      case "no_compatible_asset":
        return "Run `ketch info <pkg>` to list assets, then pin one with `asset.include` in a manifest.";
      case "checksum_mismatch":
        return "Refusing to install. Re-run to retry the download.";
      case "already_installed":
        return "Use --force to reinstall.";
      case "pinned":
        return "Run `ketch unpin <pkg>` first.";
      case "unknown_scheme":
        return `Install a source plugin named \`ketch-source-${d.scheme}\` on PATH or in the plugins dir.`;
      default:
        return null;
    }
  }

  /** Process exit code. Distinct codes let scripts branch on failure class. */
  exitCode(): number {
    switch (this.data.kind) {
      case "not_installed":
      case "no_release":
        return 4;
      case "already_installed":
      case "pinned":
        return 5;
      case "checksum_mismatch":
      case "checksum_missing":
        return 6;
      case "http":
      case "network":
        return 7;
      case "locked":
        return 8;
      default:
        return 1;
    }
  }
}
