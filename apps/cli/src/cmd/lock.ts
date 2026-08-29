//! `ketch lock` and `ketch sync`: writing a reproducible record of what is
//! installed, and making a machine match one.
//!
//! Thin, like every other command body. The file's shape and its checks live
//! in `lockfile.ts`; the installing is the same pipeline `install` uses.

import {
  batch,
  type Config,
  type InstallRequest,
  KetchError,
  Lockfile,
  type LockedPackage,
  lockfilePath,
  PackageSpec,
  type Plan,
  plan as planFor,
  Resolver,
  State,
  targetString,
  uninstall,
} from "@ketch/core";
import * as ui from "../ui.ts";
import { locked, manifests, sources } from "./shared.ts";

/** Arguments for `ketch lock`. */
export interface LockArgs {
  file: string | null;
  check: boolean;
}

/** Write the lockfile, or compare it against the tree. */
export async function write(cfg: Config, args: LockArgs): Promise<void> {
  const path = lockfilePath(args.file ?? undefined);
  const state = await State.load(cfg);

  if (args.check) {
    const plan = planFor(await Lockfile.load(path), state);
    reportPlan(plan);
    if (!plan.isClean(false)) {
      throw KetchError.msg(
        `${path} does not describe what is installed; run \`ketch sync\` or \`ketch lock\``,
      );
    }
    ui.success("clean", `${path} matches`);
    return;
  }

  const lock = Lockfile.fromState(state);
  await lock.save(path);
  ui.success("wrote", `${path} (${lock.packages.length} packages)`);
}

/** Arguments for `ketch sync`. */
export interface SyncArgs {
  file: string | null;
  prune: boolean;
  dryRun: boolean;
  jobs: number | undefined;
  yes: boolean;
}

/** Bring the tree in line with the lockfile. */
export async function sync(cfg: Config, args: SyncArgs): Promise<void> {
  const path = lockfilePath(args.file ?? undefined);
  const lock = await Lockfile.load(path);

  await locked(cfg, async () => {
    const registry = await sources(cfg);
    const state = await State.load(cfg);
    const plan = planFor(lock, state);

    if (plan.isClean(args.prune)) {
      ui.success("in sync", `${plan.matched} packages match ${path}`);
      return;
    }

    reportPlan(plan);
    if (args.dryRun) {
      return;
    }
    // Removing something the lockfile merely fails to mention is the one part
    // of a sync that can lose work, so it is asked about rather than assumed.
    const prune =
      args.prune &&
      plan.extra.length > 0 &&
      (await ui.confirmOrYes(
        args.yes,
        `remove ${plan.extra.length} packages not in the lockfile?`,
        false,
      ));

    const target = targetString(cfg.target);
    const wanted: LockedPackage[] = [...plan.missing, ...plan.changed.map(([entry]) => entry)];

    const failed: string[] = [];
    let done = 0;

    const resolver = manifests(cfg);
    const reqs = wanted.map((entry) => requestFor(cfg, entry, target));
    const bars = ui.bars();
    let outcomes;
    try {
      outcomes = await batch(
        cfg,
        registry,
        resolver,
        state,
        reqs,
        ui.jobs(cfg, args.jobs),
        (label) => bars.sink(label),
        ui.reporter,
      );
    } finally {
      bars.done();
    }

    for (const [index, outcome] of outcomes.entries()) {
      const entry = wanted[index];
      if (entry === undefined) {
        continue;
      }
      if (!outcome.ok) {
        ui.error(outcome.error);
        failed.push(entry.name);
        continue;
      }
      done += 1;
      // A pin is part of what the lockfile records, so restore it rather than
      // leaving the fresh install quietly upgradeable.
      const installed = state.get(outcome.installed.package.name);
      if (installed !== undefined) {
        installed.pinned = entry.pinned;
      }
      ui.success(
        "installed",
        `${outcome.installed.package.name} ${outcome.installed.package.version.raw}`,
      );
    }

    if (prune) {
      /* oxlint-disable no-await-in-loop -- uninstalls touch the same state and
         bin directory, so they run one at a time */
      for (const name of plan.extra) {
        try {
          const pkg = await uninstall(cfg, state, name);
          done += 1;
          ui.success("removed", `${pkg.name} ${pkg.version.raw}`);
        } catch (cause) {
          ui.error(cause);
          failed.push(name);
        }
      }
    }

    // Saved even on a partial failure, so the file matches what is on disk.
    if (done > 0) {
      await state.save(cfg);
    }
    if (failed.length > 0) {
      throw KetchError.msg(
        `${failed.length} of ${wanted.length + plan.extra.length} packages failed: ${failed.join(", ")}`,
      );
    }
    ui.success("synced", `${done} packages`);
  });
}

/** One install request for a locked entry, at exactly the tag recorded for it. */
function requestFor(cfg: Config, entry: LockedPackage, target: string): InstallRequest {
  // The recorded hash describes an asset for the machine that wrote the lock.
  // Somewhere else it names a file this host cannot even run, so holding the
  // download to it would fail every cross-platform sync.
  const onTarget = entry.matchesTarget(target);
  if (!onTarget) {
    ui.debug(`${entry.name}: locked on ${entry.target}, re-selecting the asset for ${target}`);
  }
  return {
    spec: specFor(cfg, entry),
    force: false,
    prerelease: false,
    link: true,
    requireChecksum: cfg.requireChecksums,
    assetOverride: null,
    expectedSha256: onTarget ? entry.sha256 : null,
  };
}

/**
 * How to ask for a locked package.
 *
 * The name goes first because that is how the package was found originally,
 * and which manifest tier answers decides what gets linked and under what
 * names — resolving `github:BurntSushi/ripgrep` straight from the source would
 * fall through to inference and could expose different binaries than the
 * registry entry the user actually installed.
 *
 * It is used only while it still means the same project. A name that now
 * resolves somewhere else must not quietly install that instead, so the source
 * the lock recorded wins.
 */
function specFor(cfg: Config, entry: LockedPackage): PackageSpec {
  const byName = PackageSpec.exact(entry.name, entry.tag);
  try {
    const [manifest] = Resolver.create(cfg).resolve(byName);
    if (manifest.source === entry.source.toString()) {
      return byName;
    }
  } catch {
    // No manifest under that name any more; the recorded source answers.
  }
  return PackageSpec.exact(entry.source.toString(), entry.tag);
}

function reportPlan(plan: Plan): void {
  for (const entry of plan.missing) {
    ui.out(`${ui.green("+")} ${entry.name} ${ui.dim(entry.tag)}`);
  }
  for (const [entry, have] of plan.changed) {
    ui.out(`${ui.yellow("~")} ${entry.name} ${ui.dim(`${have} -> ${entry.tag}`)}`);
  }
  for (const name of plan.extra) {
    ui.out(`${ui.red("-")} ${name} ${ui.dim("not in the lockfile")}`);
  }
  if (plan.matched > 0) {
    ui.out(ui.dim(`${plan.matched} already match`));
  }
}
