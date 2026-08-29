/**
 * The install pipeline.
 *
 * resolve manifest → resolve release → score and pick an asset → download →
 * verify checksum → extract → verify trust → place → record state.
 *
 * Every step is expressed against the `Source`, `Platform` and `Extractor`
 * interfaces, so this file contains no GitHub-specific and no macOS-specific
 * code. Where the Rust tree reached the global `ui`, the caller passes sinks
 * in instead: a `ProgressSink` per download and an `InstallReporter` for the
 * step/warn/debug lines, so nothing below the CLI owns a terminal.
 */

import fs from "node:fs";
import path from "node:path";
import { asciiLowercase, sanitizeComponent } from "@ketch/schemas";
import type { Config } from "./config.ts";
import { packageDir } from "./config.ts";
import { KetchError } from "./errors.ts";
import { extractAuto, unwrapSingleDir } from "./extract/index.ts";
import type {
  AssetSelector,
  InstalledPackage,
  Manifest,
  ManifestOrigin,
  PackageSpec,
  Release,
  ReleaseAsset,
  Version,
} from "./model.ts";
import { globMatch, nowUnix, PackageRef, targetString } from "./model.ts";
import type { AssetScore, Platform, TrustVerdict } from "./platform/platform.ts";
import { hostPlatform, mayStripQuarantine } from "./platform/platform.ts";
import type { ProgressSink } from "./progress.ts";
import type { Source, SourceRegistry } from "./source/source.ts";
import { defaultListOpts, resolveRelease } from "./source/source.ts";
import { State } from "./state.ts";

/**
 * Where the pipeline reports steps, warnings and debug detail — the seam that
 * replaces the Rust tree's global `ui::`. The CLI passes its terminal
 * implementation; everything here defaults to silence, which is what tests
 * want.
 */
export interface InstallReporter {
  /** A pipeline stage beginning, e.g. `resolving` + `ripgrep (github:…)`. */
  step(action: string, detail: string): void;
  /** Something went wrong, but the operation continues. */
  warn(message: string): void;
  /** Detail worth having in the log, invisible by default. */
  debug(message: string): void;
}

/** Discards everything. */
export const silentReporter: InstallReporter = {
  step() {},
  warn() {},
  debug() {},
};

/**
 * What `prepare` needs from the manifest module: a spec resolved to a
 * manifest, and where that manifest came from. An interface rather than an
 * import, because manifest loading is being ported separately; the CLI wires
 * the real resolver in, exactly as it wires sources.
 */
export interface ResolvedManifest {
  manifest: Manifest;
  origin: ManifestOrigin;
}

/** The manifest-resolution seam (`Resolver` in the Rust tree). */
export interface ManifestResolver {
  resolve(spec: PackageSpec): Promise<ResolvedManifest>;
}

/** One install, fully specified. */
export interface InstallRequest {
  spec: PackageSpec;
  /** Reinstall even when the resolved version is already present. */
  force: boolean;
  prerelease: boolean;
  /** Create links in the bin dir (and copy `.app`s) after unpacking. */
  link: boolean;
  /** Fail rather than record a first-seen hash when no checksum is published. */
  requireChecksum: boolean;
  /** Exact asset file name, bypassing scoring. */
  assetOverride: string | null;
  /**
   * SHA-256 this asset is already known to have, from a lockfile. Checked
   * before anything is unpacked.
   */
  expectedSha256: string | null;
}

/** A plain install of one package: latest, linked, nothing overridden. */
export function installRequest(spec: PackageSpec): InstallRequest {
  return {
    spec,
    force: false,
    prerelease: false,
    link: true,
    requireChecksum: false,
    assetOverride: null,
    expectedSha256: null,
  };
}

/** What an install actually did. */
export interface Installed {
  package: InstalledPackage;
  /** The version that was replaced, when this was an upgrade or reinstall. */
  replaced: Version | null;
}

/** An asset and the score that won it the selection. */
export interface ScoredAsset {
  asset: ReleaseAsset;
  score: AssetScore;
}

/**
 * Everything an install downloads, checks and unpacks, before it touches the
 * install tree.
 *
 * Split out from `commit` so a batch can do this part for several packages at
 * once. Nothing here writes outside the cache, so two of these running side by
 * side cannot collide: the store, the bin directory and `state.json` are only
 * reached from `commit`, which stays sequential.
 */
export interface Prepared {
  manifest: Manifest;
  origin: ManifestOrigin;
  release: Release;
  assetName: string;
  sha256: string;
  checksumVerified: boolean;
  /** Carried from the request: `commit` is the only place that links. */
  link: boolean;
  /** Root of the unpacked payload, inside `unpackDir`. */
  payload: string;
  /**
   * Temporary directory holding the unpacked payload. The Rust tree held a
   * `TempDir` so the payload outlived `prepare`; here `commit` deletes it —
   * every `Prepared` must reach `commit`, which removes the directory whether
   * placement succeeded or not.
   */
  unpackDir: string;
}

/**
 * A pinned or explicitly chosen asset outranks every platform score
 * (`i32::MAX` in the Rust tree; platform scores are small integers).
 */
const PINNED_SCORE = 2147483647;

/**
 * Run the pipeline. Mutates `state` in memory; the caller saves it, so a batch
 * install writes `state.json` once.
 */
export async function install(
  cfg: Config,
  sources: SourceRegistry,
  resolver: ManifestResolver,
  state: State,
  req: InstallRequest,
  progress: ProgressSink,
  reporter: InstallReporter = silentReporter,
): Promise<Installed> {
  const prepared = await prepare(cfg, sources, resolver, state, req, progress, reporter);
  return commit(cfg, state, prepared, reporter);
}

/**
 * Resolve, download, verify and unpack — the slow half, and the safe half to
 * run concurrently.
 */
export async function prepare(
  cfg: Config,
  sources: SourceRegistry,
  resolver: ManifestResolver,
  state: State,
  req: InstallRequest,
  progress: ProgressSink,
  reporter: InstallReporter = silentReporter,
): Promise<Prepared> {
  const platform = await hostPlatform();
  const { manifest, origin } = await resolver.resolve(req.spec);
  const sourceRef = PackageRef.tryFrom(manifest.source);
  const source = sources.forRef(sourceRef);

  const opts = {
    ...defaultListOpts(),
    includePrerelease: req.prerelease || cfg.prerelease || manifest.prerelease,
  };
  reporter.step("resolving", `${manifest.name} (${manifest.source})`);
  const release = await resolveRelease(source, sourceRef.id, req.spec.version, opts);

  // Nothing is downloaded until we know the install is actually wanted.
  const existing = state.get(manifest.name);
  if (existing !== undefined) {
    if (existing.pinned && req.spec.version.kind !== "exact") {
      throw new KetchError({
        kind: "pinned",
        name: existing.name,
        version: existing.version.toString(),
      });
    }
    if (existing.tag === release.tag && !req.force) {
      throw new KetchError({
        kind: "already_installed",
        name: existing.name,
        version: existing.version.toString(),
      });
    }
  }

  const chosen = chooseAsset(cfg, platform, release, manifest, req);
  const asset = chosen.asset;
  reporter.debug(`selected ${asset.name} — ${chosen.score.reason}`);
  if (chosen.score.emulated) {
    reporter.warn(`${asset.name} is an ${chosen.score.arch} build and will run under emulation`);
  }

  // --- download -----------------------------------------------------------
  try {
    fs.mkdirSync(cfg.cacheDir, { recursive: true });
  } catch (cause) {
    throw KetchError.io(cfg.cacheDir, asError(cause));
  }
  // A directory of its own, not a name under the cache. Two `prepare`s run
  // side by side, and an alias and a repo path naming the same package would
  // pick the same file name: they would overwrite each other's archive,
  // extract whichever landed last, and delete it from under each other. The
  // archive is staging, never a cache — it is deleted as soon as the payload
  // is unpacked — so a unique directory costs nothing.
  let staging: string;
  try {
    staging = fs.mkdtempSync(path.join(cfg.cacheDir, ".stage-"));
  } catch (cause) {
    throw KetchError.io(cfg.cacheDir, asError(cause));
  }
  try {
    const downloadPath = path.join(staging, sanitizeComponent(asset.name));
    const sha256 = await source.download(asset, downloadPath, progress);

    // --- checksum ---------------------------------------------------------
    // A lockfile's hash is checked first and separately. The source's own
    // checksum says the download was not corrupted; this says the release is
    // still the one that was locked, and a release that changed under a tag it
    // already published is exactly what a lockfile exists to catch.
    const expected = req.expectedSha256;
    if (expected !== null && asciiLowercase(expected) !== asciiLowercase(sha256)) {
      throw KetchError.msg(
        `${manifest.name}: ${asset.name} does not match the lockfile\n` +
          `  locked ${expected}\n` +
          `  got    ${sha256}\n` +
          "The release was replaced after the lock was written. Install it " +
          "deliberately and re-run `ketch lock` rather than accepting a payload " +
          "nobody recorded.",
      );
    }

    const checksumVerified = await verifyChecksum(
      source,
      sourceRef.id,
      release,
      asset,
      sha256,
      req.requireChecksum || cfg.requireChecksums,
      reporter,
    );

    // --- extract ----------------------------------------------------------
    let unpackDir: string;
    try {
      unpackDir = fs.mkdtempSync(path.join(cfg.cacheDir, ".unpack-"));
    } catch (cause) {
      throw KetchError.io(cfg.cacheDir, asError(cause));
    }
    try {
      const format = await extractAuto(downloadPath, unpackDir, platform.extractors());
      reporter.debug(`unpacked ${asset.name} as ${format}`);
      const payload = payloadRoot(unpackDir, manifest.strip_prefix);

      await checkTrust(platform, cfg, payload, manifest.name, reporter);

      return {
        manifest,
        origin,
        release,
        assetName: asset.name,
        sha256,
        checksumVerified,
        link: req.link,
        payload,
        unpackDir,
      };
    } catch (cause) {
      bestEffortRemove(unpackDir);
      throw cause;
    }
  } finally {
    // The archive is staging, never a cache: gone as soon as `prepare` is done
    // with it, whether or not it got as far as an unpacked payload.
    bestEffortRemove(staging);
  }
}

/**
 * Place the payload and record it. The half that touches the install tree, so
 * exactly one `commit` runs at a time.
 */
export async function commit(
  cfg: Config,
  state: State,
  prepared: Prepared,
  reporter: InstallReporter = silentReporter,
): Promise<Installed> {
  const { manifest, origin, release, assetName, sha256, checksumVerified, link, payload } =
    prepared;
  try {
    const platform = await hostPlatform((line) => reporter.debug(line));

    // Read again rather than trusting what `prepare` saw: in a batch, another
    // package may have been placed since.
    const existing = state.get(manifest.name);

    // --- place ------------------------------------------------------------
    const version = release.version.toString();
    const storeDir = packageDir(cfg, manifest.name, version);
    // Reinstalling the same version writes into the directory the current
    // install already occupies, and failing there must not delete it.
    const inPlace = existing !== undefined && existing.prefix === storeDir;
    // Placement moves the payload into the store before creating any link, so
    // a failure after that point — a binary name another package already owns,
    // a payload with nothing runnable in it — would leave a full store
    // directory that no state entry mentions: invisible to `ketch list`, out
    // of reach of `ketch uninstall`, and taken for a finished install by the
    // next run. Until the state entry lands, the store directory is an orphan
    // to be deleted (the Rust tree's `ScopedDir`).
    let orphan: string | null = inPlace ? null : storeDir;
    try {
      const links = await platform.place({
        name: manifest.name,
        version,
        payloadDir: payload,
        storeDir,
        binDir: cfg.binDir,
        appsDir: cfg.appsDir,
        kind: manifest.kind,
        binSpecs: manifest.bin,
        replacing: existing?.links ?? [],
        linkApps: cfg.linkApps,
        link,
      });

      // --- retire the version we replaced ---------------------------------
      if (existing !== undefined) {
        const stale = existing.links.filter((old) => !links.some((now) => now.link === old.link));
        // A failure here leaves a dangling link, not a broken install, so it
        // is reported rather than propagated.
        try {
          await platform.unplace(stale);
        } catch (cause) {
          reporter.warn(`could not remove old links for ${existing.name}: ${message(cause)}`);
        }
        if (existing.prefix !== storeDir) {
          removeStoreDir(cfg, existing.prefix, reporter);
        }
      }

      const pkg: InstalledPackage = {
        name: manifest.name,
        version: release.version,
        source: PackageRef.tryFrom(manifest.source),
        tag: release.tag,
        target: platform.target(),
        asset_name: assetName,
        sha256,
        checksum_verified: checksumVerified,
        installed_at: nowUnix(),
        prefix: storeDir,
        links,
        pinned: existing?.pinned ?? false,
        origin,
        manifest,
      };
      state.insert(pkg);
      orphan = null; // The install got far enough for the store dir to stay.

      return { package: pkg, replaced: existing?.version ?? null };
    } finally {
      if (orphan !== null) {
        bestEffortRemove(orphan);
      }
    }
  } finally {
    // The Rust tree dropped the unpack `TempDir` the moment placement moved
    // the payload out of it; here it lives until `commit` ends, either way.
    bestEffortRemove(prepared.unpackDir);
  }
}

/** One request's outcome in a batch: what Rust spelled `Result<Installed>`. */
export type InstallOutcome = { ok: true; installed: Installed } | { ok: false; error: KetchError };

/**
 * Install several packages: download and unpack them concurrently, place them
 * one at a time. One result per request, in request order.
 *
 * The install tree is reached from `commit` alone, and exactly one caller is
 * ever inside it — so the store, the links and `state.json` see the same
 * sequence of writes a one-at-a-time batch would have made. What overlaps is
 * `prepare`, which writes nothing outside the cache and spends its time
 * waiting on a network.
 *
 * A worker holds its slot until its package is placed, so at most `jobs`
 * unpacked payloads sit in the cache at once rather than the whole batch.
 */
export async function batch(
  cfg: Config,
  sources: SourceRegistry,
  resolver: ManifestResolver,
  state: State,
  reqs: readonly InstallRequest[],
  jobs: number,
  sinks: (label: string) => ProgressSink,
  reporter: InstallReporter = silentReporter,
): Promise<InstallOutcome[]> {
  if (jobs <= 1 || reqs.length <= 1) {
    const results: InstallOutcome[] = [];
    /* oxlint-disable no-await-in-loop -- one at a time is the point here */
    for (const req of reqs) {
      results.push(
        await attempt(() =>
          install(cfg, sources, resolver, state, req, sinks(req.spec.label()), reporter),
        ),
      );
    }
    /* oxlint-enable no-await-in-loop */
    return results;
  }

  // `prepare` reads a snapshot taken before the batch started, which is all
  // it needs: it decides whether an install is wanted at all. `commit` reads
  // the live state again before it places anything.
  const snapshot = new State();
  for (const pkg of state.all()) {
    snapshot.insert(pkg);
  }

  const commitLock = new SerialLock();
  // Indexed by request: every index in `0..reqs.length` is assigned by exactly
  // one worker before `Promise.all` resolves.
  const results: InstallOutcome[] = [];
  let next = 0;

  const worker = async (): Promise<void> => {
    /* oxlint-disable no-await-in-loop -- each worker is sequential by design */
    for (;;) {
      const i = next;
      next += 1;
      const req = reqs[i];
      if (req === undefined) {
        return;
      }
      const sink = sinks(req.spec.label());
      results[i] = await attempt(async () => {
        const prepared = await prepare(cfg, sources, resolver, snapshot, req, sink, reporter);
        return commitLock.run(() => commit(cfg, state, prepared, reporter));
      });
    }
    /* oxlint-enable no-await-in-loop */
  };
  await Promise.all(Array.from({ length: Math.min(jobs, reqs.length) }, () => worker()));

  // Workers finish in whatever order the network allowed. The user asked in a
  // particular one, and `results` is indexed by it — the order everything
  // downstream reports in.
  return results;
}

/** Remove links and the store directory, then drop the state entry. */
export async function uninstall(
  cfg: Config,
  state: State,
  name: string,
  reporter: InstallReporter = silentReporter,
): Promise<InstalledPackage> {
  const pkg = state.find(name);
  if (pkg === undefined) {
    throw new KetchError({ kind: "not_installed", name });
  }
  const platform = await hostPlatform((line) => reporter.debug(line));
  await platform.unplace(pkg.links);
  removeStoreDir(cfg, pkg.prefix, reporter);
  state.remove(pkg.name);
  return pkg;
}

/** Re-create links for an already-installed package. */
export async function relink(
  cfg: Config,
  state: State,
  name: string,
  reporter: InstallReporter = silentReporter,
): Promise<void> {
  const pkg = state.find(name);
  if (pkg === undefined) {
    throw new KetchError({ kind: "not_installed", name });
  }
  if (!isDirectory(pkg.prefix)) {
    throw new KetchError({ kind: "empty_payload", path: pkg.prefix });
  }
  const platform = await hostPlatform((line) => reporter.debug(line));
  await platform.unplace(pkg.links);

  const manifest = pkg.manifest;
  const version = pkg.version.toString();
  const links = await platform.place({
    name: pkg.name,
    version,
    // Already in the store: placement is idempotent over its own output.
    payloadDir: pkg.prefix,
    storeDir: pkg.prefix,
    binDir: cfg.binDir,
    appsDir: cfg.appsDir,
    kind: manifest?.kind ?? "auto",
    binSpecs: manifest?.bin ?? [],
    // `unplace` above only removed links that still pointed at us, so
    // anything left over is ours to reclaim.
    replacing: pkg.links,
    linkApps: cfg.linkApps,
    link: true,
  });

  const entry = state.get(pkg.name);
  if (entry !== undefined) {
    entry.links = links;
  }
}

/** Remove links but keep the package installed. */
export async function unlink(
  _cfg: Config,
  state: State,
  name: string,
  reporter: InstallReporter = silentReporter,
): Promise<void> {
  const pkg = state.find(name);
  if (pkg === undefined) {
    throw new KetchError({ kind: "not_installed", name });
  }
  const platform = await hostPlatform((line) => reporter.debug(line));
  await platform.unplace(pkg.links);
  const entry = state.get(pkg.name);
  if (entry !== undefined) {
    entry.links = [];
  }
}

/** Newest release available for an installed package, for `outdated`/`upgrade`. */
export async function latestRelease(
  sources: SourceRegistry,
  pkg: InstalledPackage,
  prerelease: boolean,
): Promise<Release> {
  const source = sources.forRef(pkg.source);
  const opts = { ...defaultListOpts(), includePrerelease: prerelease };
  return resolveRelease(source, pkg.source.id, { kind: "latest" }, opts);
}

/**
 * Rank a release's assets for this platform, best first. Assets the platform
 * rejects are dropped, so an empty result means nothing here is installable.
 */
export function scoreAssets(
  cfg: Config,
  platform: Platform,
  release: Release,
  selector: AssetSelector,
): ScoredAsset[] {
  const target = targetString(cfg.target);
  const targetPattern = selector.target[target];
  const out: ScoredAsset[] = [];

  for (const asset of release.assets) {
    if (selector.exclude.some((pattern) => globMatch(pattern, asset.name))) {
      continue;
    }

    // A per-target pattern is the user naming the file outright, so it
    // overrides the platform's opinion rather than filtering it.
    if (targetPattern !== undefined) {
      if (globMatch(targetPattern, asset.name)) {
        out.push({
          asset,
          score: {
            score: PINNED_SCORE,
            arch: cfg.target.arch,
            emulated: false,
            reason: `manifest pins \`${targetPattern}\` for ${target}`,
          },
        });
      }
      continue;
    }

    if (
      selector.include.length > 0 &&
      !selector.include.some((pattern) => globMatch(pattern, asset.name))
    ) {
      continue;
    }
    const score = platform.scoreAsset(asset.name, cfg.allowEmulation);
    if (score !== null) {
      out.push({ asset, score });
    }
  }

  // Name is the tie-break so repeated runs pick the same asset.
  out.sort((a, b) => b.score.score - a.score.score || compareNames(a.asset.name, b.asset.name));
  return out;
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

function chooseAsset(
  cfg: Config,
  platform: Platform,
  release: Release,
  manifest: Manifest,
  req: InstallRequest,
): ScoredAsset {
  const wanted = req.assetOverride;
  if (wanted !== null) {
    const asset = release.assets.find((a) => a.name === wanted);
    if (asset === undefined) {
      throw KetchError.msg(`release \`${release.tag}\` has no asset named \`${wanted}\``);
    }
    return {
      asset,
      score: {
        score: PINNED_SCORE,
        arch: cfg.target.arch,
        emulated: false,
        reason: "chosen with --asset",
      },
    };
  }

  const best = scoreAssets(cfg, platform, release, manifest.asset)[0];
  if (best === undefined) {
    throw new KetchError({
      kind: "no_compatible_asset",
      id: manifest.source,
      tag: release.tag,
      target: targetString(cfg.target),
    });
  }
  return best;
}

/** Returns whether the hash was confirmed against a published checksum. */
export async function verifyChecksum(
  source: Source,
  id: string,
  release: Release,
  asset: ReleaseAsset,
  actual: string,
  require: boolean,
  reporter: InstallReporter = silentReporter,
): Promise<boolean> {
  let published: string | undefined;
  if (asset.digest !== null) {
    published = asset.digest.hex;
  } else if (source.checksums !== undefined) {
    // Only worth the extra requests when the asset carries no digest.
    let table: Map<string, string>;
    try {
      table = await source.checksums(id, release, asset.name);
    } catch (cause) {
      reporter.debug(`could not read published checksums: ${message(cause)}`);
      table = new Map();
    }
    published = table.get(asset.name);
  }

  if (published !== undefined) {
    if (asciiLowercase(published) === asciiLowercase(actual)) {
      return true;
    }
    throw new KetchError({
      kind: "checksum_mismatch",
      name: asset.name,
      expected: published,
      actual,
    });
  }
  if (require) {
    throw new KetchError({ kind: "checksum_missing", name: asset.name });
  }
  reporter.debug(
    `${asset.name} publishes no checksum; recording ${actual.slice(0, 12)} on first use`,
  );
  return false;
}

/**
 * Apply `strip_prefix`, or unwrap the single wrapper directory most tarballs
 * use, so the payload root is where the files actually are.
 */
function payloadRoot(unpacked: string, strip: number | null | undefined): string {
  if (strip === undefined || strip === null || strip === 0) {
    return unwrapSingleDir(unpacked);
  }
  let root = unpacked;
  for (let i = 0; i < strip; i += 1) {
    root = unwrapSingleDir(root);
  }
  return root;
}

/**
 * Inspect the payload and strip quarantine only when the platform says the
 * code is genuinely trusted. A failed check never blocks an install the user
 * explicitly asked for; it is reported instead.
 */
async function checkTrust(
  platform: Platform,
  cfg: Config,
  payload: string,
  name: string,
  reporter: InstallReporter,
): Promise<void> {
  let verdict: TrustVerdict;
  try {
    verdict = await platform.verifyTrust(payload);
  } catch (cause) {
    reporter.debug(`trust check failed for ${name}: ${message(cause)}`);
    return;
  }
  switch (verdict.kind) {
    case "trusted":
      reporter.debug(`signed by ${verdict.authority}`);
      break;
    case "weak":
      reporter.debug(`weak signature: ${verdict.detail}`);
      break;
    case "untrusted":
      reporter.debug(`unsigned: ${verdict.detail}`);
      break;
    case "not_applicable":
      break;
  }
  if (cfg.stripQuarantine && mayStripQuarantine(verdict)) {
    try {
      await platform.clearQuarantine(payload);
    } catch (cause) {
      reporter.debug(`could not clear quarantine: ${message(cause)}`);
    }
  }
}

/**
 * Delete a store directory, and its now-empty package parent.
 *
 * Refuses anything outside the store: a corrupted state file must never turn
 * an uninstall into a `rm -rf` of somewhere else.
 */
export function removeStoreDir(
  cfg: Config,
  prefix: string,
  reporter: InstallReporter = silentReporter,
): void {
  // Component-wise containment, like Rust's `Path::starts_with`: `store2` is
  // not inside `store`, and the store itself is never the thing removed.
  const rel = path.relative(cfg.storeDir, prefix);
  if (rel === "" || rel === ".." || rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel)) {
    reporter.warn(`refusing to remove ${prefix} — it is not inside the ketch store`);
    return;
  }
  try {
    fs.rmSync(prefix, { recursive: true });
  } catch (cause) {
    if (errno(cause) !== "ENOENT") {
      reporter.warn(`could not remove ${prefix}: ${message(cause)}`);
    }
  }
  const parent = path.dirname(prefix);
  if (parent !== cfg.storeDir) {
    try {
      fs.rmdirSync(parent); // only succeeds when empty
    } catch {
      // Not empty, or already gone — either way it is not ours to force.
    }
  }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

/**
 * The single-flight mutex a batch runs every `commit` through: tasks execute
 * strictly one after another, in the order they arrived at the lock.
 */
class SerialLock {
  private tail: Promise<unknown> = Promise.resolve();

  run<T>(task: () => Promise<T>): Promise<T> {
    const turn = this.tail.then(task);
    // The chain must survive a failed task, or one bad commit would wedge
    // every commit queued after it.
    this.tail = turn.catch(() => undefined);
    return turn;
  }
}

/** One request's failure is that request's result, never the batch's. */
async function attempt(run: () => Promise<Installed>): Promise<InstallOutcome> {
  try {
    return { ok: true, installed: await run() };
  } catch (cause) {
    return {
      ok: false,
      error: cause instanceof KetchError ? cause : KetchError.msg(message(cause)),
    };
  }
}

/** Cleanup of our own temporary trees, as quiet as Rust's `TempDir::drop`. */
function bestEffortRemove(dir: string): void {
  try {
    fs.rmSync(dir, { recursive: true, force: true });
  } catch {
    // Nothing useful to do; the next run's temp dirs are fresh anyway.
  }
}

/** Plain code-unit order, matching Rust's byte-wise `str` comparison. */
function compareNames(a: string, b: string): number {
  if (a === b) {
    return 0;
  }
  return a < b ? -1 : 1;
}

function isDirectory(p: string): boolean {
  try {
    return fs.statSync(p).isDirectory();
  } catch {
    return false;
  }
}

function message(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

function errno(cause: unknown): string | undefined {
  return typeof cause === "object" && cause !== null && "code" in cause
    ? String((cause as { code: unknown }).code)
    : undefined;
}
