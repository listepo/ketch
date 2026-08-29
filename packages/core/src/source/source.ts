/**
 * Where packages come from.
 *
 * A `Source` turns an opaque id into releases and downloadable assets. GitHub
 * is built in; anything else can be added as an external plugin executable
 * without recompiling ketch. This module owns the interface and the release
 * selection rules every source shares; implementations live beside it.
 */

import { asciiLowercase } from "@ketch/schemas";
import { KetchError } from "../errors.ts";
import type { PackageRef, Release, ReleaseAsset, SourceInfo, VersionSpec } from "../model.ts";
import type { ProgressSink } from "../progress.ts";

/** Knobs that apply to listing releases, independent of the source. */
export interface ListOpts {
  includePrerelease: boolean;
  /** Upper bound on releases fetched. Sources may return fewer. */
  limit: number;
}

export function defaultListOpts(): ListOpts {
  return { includePrerelease: false, limit: 30 };
}

/**
 * A backend that can enumerate and fetch releases.
 *
 * Ketch keeps one instance per scheme for the life of the process. Methods
 * the Rust trait gave default bodies are optional here; `resolveRelease`,
 * `optsFor` and `pick` carry those defaults so every implementation picks
 * versions the same way.
 */
export interface Source {
  /**
   * The scheme this source answers to, e.g. `github`. Must be stable — it
   * appears in user input and in recorded state.
   */
  readonly scheme: string;

  /**
   * Repository-level metadata. Optional: leave undefined when the source has
   * nothing beyond releases.
   */
  describe?(id: string): Promise<SourceInfo | null>;

  /**
   * Releases, newest first. Drafts must be excluded; prereleases are
   * included only when `opts.includePrerelease` is set.
   */
  listReleases(id: string, opts: ListOpts): Promise<Release[]>;

  /**
   * Resolve a version request to one release.
   *
   * `resolveRelease` walks `listReleases`, which is correct for every source.
   * Implement only to use a cheaper endpoint (GitHub does, for `latest`).
   */
  resolve?(id: string, want: VersionSpec, opts: ListOpts): Promise<Release>;

  /**
   * Checksums published alongside a release, keyed by asset file name.
   *
   * `wanted` is the asset actually being installed. A source that pays per
   * file for this — GitHub publishes one sidecar per asset — should look
   * that one up first, so its own request limits can never be what leaves
   * this install unverified.
   */
  checksums?(id: string, release: Release, wanted: string): Promise<Map<string, string>>;

  /** Download one asset to `dest`, returning its SHA-256 as lowercase hex. */
  download(asset: ReleaseAsset, dest: string, progress: ProgressSink): Promise<string>;

  /** Free-text search. Sources that cannot search leave this undefined. */
  search?(query: string, limit: number): Promise<SourceInfo[]>;

  /** A browsable URL for humans, when one exists. */
  webUrl?(id: string): string | null;
}

/**
 * Listing options widened for an exact request.
 *
 * Naming a tag is explicit consent to install that release, prerelease or
 * not. The consent has to be applied to the *listing*: sources drop
 * prereleases before `pick` ever sees them, so filtering afterwards means an
 * exact request for a prerelease could never be satisfied at all.
 */
export function optsFor(want: VersionSpec, opts: ListOpts): ListOpts {
  return {
    ...opts,
    includePrerelease: opts.includePrerelease || want.kind === "exact",
  };
}

/** Shared release-selection logic, so every source picks versions the same way. */
export function pick(id: string, releases: Release[], want: VersionSpec, opts: ListOpts): Release {
  let candidates = releases.filter((r) => !r.draft);
  if (want.kind === "exact") {
    const tag = want.value;
    // No prerelease filter: `optsFor` has already made sure the listing this
    // ran over includes them.
    const found = candidates.find(
      (r) => asciiLowercase(r.tag) === asciiLowercase(tag) || r.version.matchesRequest(tag),
    );
    if (found === undefined) {
      throw new KetchError({ kind: "no_release", id: `${id}@${tag}` });
    }
    return found;
  }
  if (!opts.includePrerelease) {
    const stable = candidates.filter((r) => !r.prerelease && !r.version.isPrerelease());
    if (stable.length > 0) {
      candidates = stable;
    }
  }
  let best: Release | null = null;
  for (const release of candidates) {
    // `>=` keeps the last of equal versions, matching Rust's `max_by`.
    if (best === null || release.version.compare(best.version) >= 0) {
      best = release;
    }
  }
  if (best === null) {
    throw new KetchError({ kind: "no_release", id });
  }
  return best;
}

/** The Rust trait's default `resolve`, for sources that do not implement one. */
export async function resolveRelease(
  source: Source,
  id: string,
  want: VersionSpec,
  opts: ListOpts,
): Promise<Release> {
  if (source.resolve !== undefined) {
    return source.resolve(id, want, opts);
  }
  const widened = optsFor(want, opts);
  return pick(id, await source.listReleases(id, widened), want, widened);
}

/**
 * Every source available this run, resolved by scheme. The type surface only:
 * construction (built-ins plus plugin discovery) lives with the source
 * implementations, because it needs HTTP and plugin subprocess plumbing.
 */
export interface SourceRegistry {
  /** Throws `unknown_scheme` when nothing answers to `scheme`. */
  get(scheme: string): Source;
  forRef(reference: PackageRef): Source;
  schemes(): string[];
  all(): readonly Source[];
}
