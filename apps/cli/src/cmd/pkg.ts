//! Commands that change what is installed.
//!
//! Each one takes the lock for the whole batch and writes `state.json` once at
//! the end, so an interrupted run leaves the file either fully old or fully new.

import {
  batch,
  type Config,
  type InstallRequest,
  KetchError,
  latestRelease,
  PackageSpec,
  relink,
  State,
  targetString,
  uninstall as removePackage,
  unlink as unlinkPackage,
} from "@ketch/core";
import * as ui from "../ui.ts";
import { dedupe, locked, manifests, pathHint, report, select, sources } from "./shared.ts";

export interface InstallArgs {
  packages: readonly string[];
  force: boolean;
  prerelease: boolean;
  noLink: boolean;
  requireChecksum: boolean;
  asset: string | null;
  jobs: number | undefined;
  yes: boolean;
}

export async function install(cfg: Config, args: InstallArgs): Promise<void> {
  if (args.asset !== null) {
    if (args.packages.length > 1) {
      throw KetchError.msg("--asset names one file, so it can only be used with a single package");
    }
    // Naming an asset bypasses the check that stops ketch installing a build
    // for another platform, so it is confirmed rather than assumed.
    const question = `install \`${args.asset}\` without checking it runs on ${targetString(cfg.target)}?`;
    if (!(await ui.confirmOrYes(args.yes, question, false))) {
      return;
    }
  }

  await locked(cfg, async () => {
    const registry = await sources(cfg);
    const state = await State.load(cfg);

    // The same package twice would be downloaded twice and placed twice, with
    // the second replacing the first for no reason. Order is the user's.
    const wanted = dedupe(args.packages);
    const reqs: InstallRequest[] = wanted.map((raw) => ({
      spec: PackageSpec.parse(raw),
      force: args.force,
      prerelease: args.prerelease,
      link: !args.noLink,
      requireChecksum: args.requireChecksum || cfg.requireChecksums,
      assetOverride: args.asset,
      expectedSha256: null,
    }));

    const single = reqs.length === 1;
    let done = 0;
    const failed: string[] = [];

    const bars = ui.bars();
    let outcomes;
    try {
      outcomes = await batch(
        cfg,
        registry,
        manifests(cfg),
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
      if (outcome.ok) {
        done += 1;
        report(outcome.installed);
        continue;
      }
      if (single) {
        throw outcome.error;
      }
      // One bad package must not discard the ones that already succeeded, so
      // the failure is held until the state file is saved.
      ui.error(outcome.error);
      failed.push(wanted[index] ?? "");
    }

    if (done > 0) {
      await state.save(cfg);
      pathHint(cfg, state);
    }
    if (failed.length > 0) {
      throw KetchError.msg(
        `${failed.length} of ${wanted.length} packages failed: ${failed.join(", ")}`,
      );
    }
  });
}

export interface UninstallArgs {
  names: readonly string[];
  yes: boolean;
}

export async function uninstall(cfg: Config, args: UninstallArgs): Promise<void> {
  await locked(cfg, async () => {
    const state = await State.load(cfg);

    // Resolve every name up front: a typo should stop the command before it has
    // already removed the packages that did match.
    const targets = dedupe(select(state, args.names).map((pkg) => pkg.name));

    if (!(await ui.confirmOrYes(args.yes, `remove ${targets.join(", ")}?`, false))) {
      return;
    }

    let removed = 0;
    /* oxlint-disable no-await-in-loop -- removal is one at a time by design */
    for (const name of targets) {
      try {
        const pkg = await removePackage(cfg, state, name, ui.reporter);
        removed += 1;
        ui.success("removed", `${pkg.name} ${pkg.version.raw}`);
      } catch (err) {
        ui.error(err);
      }
    }
    if (removed > 0) {
      await state.save(cfg);
    }
    if (removed < targets.length) {
      throw KetchError.msg(`removed ${removed} of ${targets.length} packages`);
    }
  });
}

export interface UpgradeArgs {
  names: readonly string[];
  dryRun: boolean;
  prerelease: boolean;
  force: boolean;
  jobs: number | undefined;
  yes: boolean;
}

export async function upgrade(cfg: Config, args: UpgradeArgs): Promise<void> {
  await locked(cfg, async () => {
    const registry = await sources(cfg);
    const state = await State.load(cfg);

    const wanted = select(state, args.names);
    if (wanted.length === 0) {
      ui.out("nothing installed");
      return;
    }

    const prerelease = args.prerelease || cfg.prerelease;
    const plan: { pkg: (typeof wanted)[number]; tag: string; to: string }[] = [];
    let checked = 0;
    let unreachable = 0;

    /* oxlint-disable no-await-in-loop -- one check at a time keeps output readable */
    for (const pkg of wanted) {
      if (pkg.pinned && !args.force) {
        ui.debug(`${pkg.name} is pinned at ${pkg.version.raw}`);
        continue;
      }
      ui.step("checking", pkg.name);
      let release;
      try {
        release = await latestRelease(registry, pkg, prerelease);
      } catch (err) {
        // An unreachable source for one package must not abandon the rest.
        ui.warn(`${pkg.name}: ${err instanceof Error ? err.message : String(err)}`);
        unreachable += 1;
        continue;
      }
      checked += 1;
      // Compare versions, not tags: a retagged release is not an upgrade, and
      // neither is a source that briefly reports an older one.
      if (release.tag === pkg.tag || release.version.compare(pkg.version) <= 0) {
        continue;
      }
      plan.push({ pkg, tag: release.tag, to: release.version.raw });
    }

    if (plan.length === 0) {
      // "Up to date" is a claim about versions we actually saw. With nothing
      // checked we do not know, and exiting 0 tells a script the opposite.
      if (checked === 0 && unreachable > 0) {
        throw KetchError.msg(
          `could not check any of the ${unreachable} packages; see the warnings above`,
        );
      }
      ui.success(
        "up to date",
        unreachable > 0
          ? `${checked} packages (${unreachable} could not be checked)`
          : `${checked} packages`,
      );
      return;
    }

    ui.table(
      ["package", "from", "to"],
      plan.map((entry) => [entry.pkg.name, entry.pkg.version.raw, entry.to]),
    );

    if (args.dryRun) {
      return;
    }
    if (!(await ui.confirmOrYes(args.yes, `upgrade ${plan.length} packages?`, true))) {
      return;
    }

    const reqs: InstallRequest[] = plan.map((entry) => ({
      // The exact tag that was reported, so nothing can change between the plan
      // the user approved and what is installed. Assembled, not parsed: Rust
      // built the struct literally here, and `parse` splits at an `@` and reads
      // the last `/` as part of the id — both of which a real tag may contain.
      spec: PackageSpec.exact(entry.pkg.source.toString(), entry.tag),
      force: true,
      prerelease,
      // A package installed with --no-link stays unlinked.
      link: entry.pkg.links.length > 0,
      requireChecksum: cfg.requireChecksums,
      assetOverride: null,
      expectedSha256: null,
    }));

    let done = 0;
    const failed: string[] = [];
    const bars = ui.bars();
    let outcomes;
    try {
      outcomes = await batch(
        cfg,
        registry,
        manifests(cfg),
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
      if (outcome.ok) {
        done += 1;
        report(outcome.installed);
        continue;
      }
      ui.error(outcome.error);
      failed.push(plan[index]?.pkg.name ?? "");
    }

    if (done > 0) {
      await state.save(cfg);
      pathHint(cfg, state);
    }
    if (failed.length > 0) {
      throw KetchError.msg(`failed to upgrade ${failed.join(", ")}`);
    }
  });
}

export interface NameArgs {
  names: readonly string[];
}

/** `pin` and `unpin`: the only difference is the flag being written. */
export async function pin(cfg: Config, args: NameArgs, pinned: boolean): Promise<void> {
  await locked(cfg, async () => {
    const state = await State.load(cfg);
    for (const pkg of select(state, args.names)) {
      pkg.pinned = pinned;
      ui.success(pinned ? "pinned" : "unpinned", `${pkg.name} ${pkg.version.raw}`);
    }
    await state.save(cfg);
  });
}

/** `link` and `unlink`: re-create the links, or take them away. */
export async function link(cfg: Config, args: NameArgs, linked: boolean): Promise<void> {
  await locked(cfg, async () => {
    const state = await State.load(cfg);
    const names = select(state, args.names).map((pkg) => pkg.name);
    /* oxlint-disable no-await-in-loop -- linking is one at a time by design */
    for (const name of names) {
      if (linked) {
        await relink(cfg, state, name, ui.reporter);
        ui.success("linked", name);
      } else {
        await unlinkPackage(cfg, state, name, ui.reporter);
        ui.success("unlinked", name);
      }
    }
    await state.save(cfg);
    if (linked) {
      pathHint(cfg, state);
    }
  });
}
