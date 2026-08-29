//! Read-only commands.
//!
//! These never take the lock and never write. Their data output goes to stdout
//! through `ui.out`/`ui.table` so it can be piped while progress and warnings
//! stay on stderr.

import {
  changelog as changelogFile,
  type Config,
  defaultListOpts,
  hostPlatform,
  inferredManifest,
  installedBinaries,
  installedRecord,
  type InstalledPackage,
  KetchError,
  latestRelease,
  type Manifest,
  type ManifestOrigin,
  PackageRef,
  PackageSpec,
  type Release,
  Resolver,
  resolveRelease,
  type ScoredAsset,
  scoreAssets,
  type SourceInfo,
  State,
  type VersionSpec,
} from "@ketch/core";
import * as ui from "../ui.ts";
import { sources } from "./shared.ts";

/** Arguments for `ketch list`. */
export interface ListArgs {
  json: boolean;
  namesOnly: boolean;
}

/** Show what is installed. */
export async function list(cfg: Config, args: ListArgs): Promise<void> {
  const state = await State.load(cfg);
  const packages = state.all();

  if (args.json) {
    printJson(packages.map(installedRecord));
    return;
  }
  if (args.namesOnly) {
    for (const pkg of packages) {
      ui.out(pkg.name);
    }
    return;
  }
  if (packages.length === 0) {
    ui.out("nothing installed");
    return;
  }

  ui.table(
    ["package", "version", "source"],
    packages.map((pkg) => [
      pkg.name,
      `${pkg.version.raw}${pkg.pinned ? " (pinned)" : ""}`,
      pkg.source.toString(),
    ]),
  );
}

/** Arguments for `ketch outdated`. */
export interface OutdatedArgs {
  json: boolean;
  prerelease: boolean;
}

/** Show which installed packages have a newer release. */
export async function outdated(cfg: Config, args: OutdatedArgs): Promise<void> {
  const state = await State.load(cfg);
  const registry = await sources(cfg);
  const prerelease = args.prerelease || cfg.prerelease;

  const rows: string[][] = [];
  const json: unknown[] = [];
  let checked = 0;
  let unreachable = 0;
  /* oxlint-disable no-await-in-loop -- one source at a time, so a rate limit
     costs one package rather than the whole report */
  for (const pkg of state.all()) {
    ui.step("checking", pkg.name);
    let release: Release;
    try {
      release = await latestRelease(registry, pkg, prerelease);
    } catch (cause) {
      // Reporting is best-effort: one unreachable source must not hide the
      // rest of the answer.
      ui.warn(`${pkg.name}: ${reason(cause)}`);
      unreachable += 1;
      continue;
    }
    checked += 1;
    if (release.version.compare(pkg.version) <= 0) {
      continue;
    }
    rows.push([pkg.name, pkg.version.raw, release.version.raw, pkg.pinned ? "pinned" : ""]);
    json.push({
      name: pkg.name,
      installed: pkg.version.raw,
      latest: release.version.raw,
      tag: release.tag,
      pinned: pkg.pinned,
    });
  }

  // An empty answer because nothing could be reached is not the same answer as
  // an empty answer because nothing is out of date, and `[]` on stdout with an
  // exit status of 0 says the second either way.
  if (checked === 0 && unreachable > 0) {
    throw KetchError.msg(
      `could not check any of the ${unreachable} packages; see the warnings above`,
    );
  }
  if (args.json) {
    printJson(json);
    return;
  }
  if (rows.length === 0) {
    ui.out(
      unreachable > 0
        ? `everything checked is up to date (${unreachable} could not be checked)`
        : "everything is up to date",
    );
    return;
  }
  ui.table(["package", "installed", "latest", ""], rows);
}

/** Arguments for `ketch info`. */
export interface InfoArgs {
  package: string;
  json: boolean;
  assets: boolean;
}

/** One `label: value` line of `ketch info`. */
function field(label: string, value: string): void {
  ui.out(`${ui.dim(label).padEnd(12)} ${value}`);
}

/** Everything known about one package, installed or not. */
export async function info(cfg: Config, args: InfoArgs): Promise<void> {
  const state = await State.load(cfg);
  const installed = state.find(args.package) ?? null;
  const spec = PackageSpec.parse(args.package);
  const manifest = resolveManifest(cfg, spec, installed, true);

  const registry = await sources(cfg);
  const reference = manifestRef(manifest);
  const source = registry.forRef(reference);
  let described: SourceInfo | null = null;
  if (source.describe !== undefined) {
    try {
      described = await source.describe(reference.id);
    } catch (cause) {
      ui.debug(`describe failed: ${reason(cause)}`);
    }
  }

  const opts = { ...defaultListOpts(), includePrerelease: cfg.prerelease || manifest.prerelease };
  let release: Release | null = null;
  try {
    release = await resolveRelease(source, reference.id, { kind: "latest" }, opts);
  } catch (cause) {
    ui.warn(`${manifest.name}: ${reason(cause)}`);
  }

  let scored: ScoredAsset[] = [];
  if (release !== null && args.assets) {
    scored = scoreAssets(cfg, await hostPlatform(), release, manifest.asset);
  }

  const description = manifest.description ?? described?.description ?? null;
  const homepage = manifest.homepage ?? described?.homepage ?? null;

  if (args.json) {
    printJson({
      name: manifest.name,
      source: manifest.source,
      url: source.webUrl === undefined ? null : source.webUrl(reference.id),
      description,
      homepage,
      stars: described?.stars ?? null,
      license: described?.license ?? null,
      archived: described?.archived ?? false,
      latest: release?.version.raw ?? null,
      latest_tag: release?.tag ?? null,
      installed: installed?.version.raw ?? null,
      pinned: installed?.pinned ?? false,
      assets: scored.map((s) => ({
        name: s.asset.name,
        size: s.asset.size,
        score: s.score.score,
        reason: s.score.reason,
        emulated: s.score.emulated,
      })),
    });
    return;
  }

  ui.out(ui.bold(manifest.name));
  if (description !== null) {
    ui.out(description);
  }
  ui.out("");

  field("source", manifest.source);
  const url = source.webUrl === undefined ? null : source.webUrl(reference.id);
  if (url !== null) {
    field("url", url);
  }
  if (homepage !== null) {
    field("homepage", homepage);
  }
  if (described !== null) {
    if (described.stars !== null) {
      field("stars", String(described.stars));
    }
    if (described.license !== null) {
      field("license", described.license);
    }
    if (described.archived) {
      field("archived", "yes — this repository is no longer maintained");
    }
  }
  if (release !== null) {
    field("latest", `${release.version.raw} (${release.assets.length} assets)`);
  }
  if (installed === null) {
    field("installed", "no");
  } else {
    field("installed", `${installed.version.raw}${installed.pinned ? " (pinned)" : ""}`);
    field("prefix", installed.prefix);
    for (const link of installedBinaries(installed)) {
      field("binary", link.link);
    }
  }

  if (args.assets) {
    ui.out("");
    if (scored.length === 0) {
      ui.out("no asset in this release can run on this machine");
    } else {
      ui.table(
        ["asset", "size", "score", "why"],
        scored.map((s) => [
          s.asset.name,
          ui.bytes(s.asset.size),
          String(s.score.score),
          s.score.reason,
        ]),
      );
    }
  }
}

/** Arguments for `ketch changelog`. */
export interface ChangelogArgs {
  package: string;
  latest: boolean;
  file: boolean;
  release: boolean;
}

/**
 * Print what changed in a release.
 *
 * The file the package ships is preferred over the notes the release
 * published: it needs no network, and it is the history of the version
 * actually on disk. A file with no section for that version is not an answer
 * about it, though — plenty of projects cut a release before the heading is
 * written — so that falls through to the notes, and back to the whole file
 * only when there are none to fall through to.
 */
export async function changelog(cfg: Config, args: ChangelogArgs): Promise<void> {
  const state = await State.load(cfg);
  const spec = PackageSpec.parse(args.package);
  const installed = state.find(args.package) ?? null;
  // Only the installed version has a file; any other release is the source's
  // to answer for.
  const elsewhere = args.latest || spec.version.kind === "exact";

  let wholeFile: { entry: changelogFile.Entry; name: string; version: string } | null = null;
  if (!args.release && installed !== null && !elsewhere) {
    const version = installed.version.raw;
    const found = changelogFile.findFile(installed.prefix);
    if (found === null) {
      ui.debug(`no changelog under ${installed.prefix}`);
    } else {
      const entry = changelogFile.fromFile(found, version);
      if (entry.heading !== null || args.file) {
        show(entry, installed.name, version);
        return;
      }
      ui.debug(`${found} has no entry for ${version}`);
      wholeFile = { entry, name: installed.name, version };
    }
  }
  if (args.file) {
    throw KetchError.msg(
      installed === null
        ? `${args.package} is not installed, so there is no file to read`
        : `${installed.name} ships no changelog file; \`ketch changelog ${installed.name} --release\` reads the published notes`,
    );
  }

  try {
    const notes = await publishedNotes(cfg, spec, installed, args.latest);
    show(notes.entry, notes.name, notes.version);
  } catch (cause) {
    if (wholeFile === null) {
      throw cause;
    }
    ui.warn(reason(cause));
    show(wholeFile.entry, wholeFile.name, wholeFile.version);
  }
}

/** The notes the source published for the release being asked about. */
async function publishedNotes(
  cfg: Config,
  spec: PackageSpec,
  installed: InstalledPackage | null,
  latest: boolean,
): Promise<{ name: string; version: string; entry: changelogFile.Entry }> {
  const manifest = resolveManifest(cfg, spec, installed, false);

  // Without `--latest` or an explicit version, the notes wanted are the ones
  // for the release that is installed, not whatever is newest.
  let want: VersionSpec = { kind: "latest" };
  if (spec.version.kind === "exact") {
    want = spec.version;
  } else if (!latest && installed !== null) {
    want = { kind: "exact", value: installed.tag };
  }

  const registry = await sources(cfg);
  const reference = manifestRef(manifest);
  const source = registry.forRef(reference);
  const opts = { ...defaultListOpts(), includePrerelease: cfg.prerelease || manifest.prerelease };
  const release = await resolveRelease(source, reference.id, want, opts);
  const version = release.version.raw;
  const entry = changelogFile.fromRelease(release.notes);
  if (entry === null) {
    throw KetchError.msg(`${manifest.name} ${version} published no release notes`);
  }
  return { name: manifest.name, version, entry };
}

/**
 * The changelog itself goes to stdout; where it came from goes to stderr, so
 * `ketch changelog rg > NOTES.md` leaves nothing but the markdown.
 */
function show(entry: changelogFile.Entry, name: string, version: string): void {
  if (entry.origin.kind === "file") {
    ui.step("changelog", `${name} ${version} · ${entry.origin.path}`);
    // Saying nothing here would pass a whole file off as one release.
    if (entry.heading === null) {
      ui.warn(`no entry for ${version} in ${entry.origin.path}; showing the whole file`);
    }
  } else {
    ui.step("changelog", `${name} ${version} · release notes`);
  }
  if (entry.heading !== null) {
    ui.out(ui.bold(entry.heading));
    ui.out("");
  }
  ui.out(entry.body === "" ? ui.dim("(nothing recorded)") : entry.body);
}

/** Arguments for `ketch search`. */
export interface SearchArgs {
  query: string[];
  limit: number;
}

/** Search the curated manifests, then every source. */
export async function search(cfg: Config, args: SearchArgs): Promise<void> {
  const query = args.query.join(" ").trim();
  if (query === "") {
    throw KetchError.msg("nothing to search for");
  }

  // Curated manifests first: they install with better names and known
  // binaries, so they are the answer whenever one matches.
  const known = Resolver.create(cfg, ui.warn).search(query);
  if (known.length > 0) {
    ui.out(ui.bold("known packages"));
    ui.table(
      ["package", "source", "description"],
      known
        .slice(0, args.limit)
        .map((m) => [m.name, m.source, ui.truncate(m.description ?? "", 60)]),
    );
    ui.out("");
  }

  const registry = await sources(cfg);
  const rows: string[][] = [];
  /* oxlint-disable no-await-in-loop -- sequential so one slow source does not
     interleave its warning with another's results */
  for (const source of registry.all()) {
    if (source.search === undefined) {
      continue;
    }
    let hits: SourceInfo[];
    try {
      hits = await source.search(query, args.limit);
    } catch (cause) {
      ui.warn(`${source.scheme}: ${reason(cause)}`);
      continue;
    }
    for (const hit of hits) {
      rows.push([
        `${source.scheme}:${hit.id}`,
        hit.stars === null ? "" : String(hit.stars),
        ui.truncate(hit.description ?? "", 60),
      ]);
    }
  }

  if (rows.length === 0) {
    if (known.length === 0) {
      ui.out(`no results for \`${query}\``);
    }
    return;
  }
  ui.out(ui.bold("repositories"));
  ui.table(["package", "stars", "description"], rows.slice(0, args.limit));
}

/**
 * The manifest to answer about, falling back to what the install recorded.
 *
 * An installed package always has an answer, even when the registry has
 * forgotten the name it was installed under.
 */
function resolveManifest(
  cfg: Config,
  spec: PackageSpec,
  installed: InstalledPackage | null,
  trace: boolean,
): Manifest {
  try {
    const [manifest, origin] = Resolver.create(cfg, ui.warn).resolve(spec);
    if (trace) {
      ui.debug(`manifest from ${describeOrigin(origin)}`);
    }
    return manifest;
  } catch (cause) {
    if (installed === null) {
      throw cause;
    }
    return installed.manifest ?? inferredManifest(installed.source);
  }
}

/** A manifest's `source` field as the reference the registry dispatches on. */
function manifestRef(manifest: Manifest): PackageRef {
  const reference = PackageRef.parse(manifest.source);
  if (reference === null) {
    throw KetchError.msg(`${manifest.name} names an unreadable source: ${manifest.source}`);
  }
  return reference;
}

function describeOrigin(origin: ManifestOrigin): string {
  if (origin === "builtin") {
    return "the built-in registry";
  }
  if (origin === "inferred") {
    return "inference";
  }
  return "registry" in origin ? origin.registry : origin.user;
}

function reason(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}

function printJson(value: unknown): void {
  ui.out(JSON.stringify(value, null, 2));
}
