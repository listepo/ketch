/**
 * Commands about ketch itself and its environment — the port of
 * `src/cmd/system.rs`.
 *
 * Everything here is about the host app or the machine it runs on rather than
 * about a client app: `doctor`, `update`, `path`, `plugin` and `self`. The
 * `path` bodies are the only ones in the CLI that write outside the ketch
 * root, which is why they are a command the user asks for rather than
 * something `install` does behind their back.
 */

import fs from "node:fs";
import os from "node:os";
import process from "node:process";
import {
  binDirOnPath,
  type Config,
  discoverPlugins,
  type DoctorCheck,
  doctorFail,
  doctorOk,
  doctorWarn,
  hostPlatform,
  KetchError,
  registry,
  selfupdate,
  shell,
  State,
  targetString,
  worstStatus,
} from "@ketch/core";
import { PROTOCOL_VERSION } from "@ketch/schemas";
// The running version lives with the clap surface in Rust too, and
// `version.test.ts` pins it to `package.json`. The import cycle back into
// `cli.ts` is only read inside function bodies, never at module scope.
import { VERSION } from "../cli.ts";
import * as ui from "../ui.ts";
import { dedupe } from "./shared.ts";

/** Refresh the local copy of the package registry. */
export async function update(cfg: Config): Promise<void> {
  // Rust announced this from inside `registry::update`; core has no `ui`, so
  // the step is said here and the module below stays free of printing.
  ui.step("updating", `registry ${cfg.registry}`);
  const count = await registry.update(cfg, { progress: ui.progress(), warn: ui.warn });
  ui.success("updated", `${count} packages from ${cfg.registry}`);
}

/** Check the environment and the install tree, optionally repairing PATH. */
export async function doctor(cfg: Config, opts: { fix: boolean }): Promise<void> {
  if (opts.fix) {
    fix(cfg);
  }

  const checks: DoctorCheck[] = [
    doctorOk("version", `ketch ${VERSION} for ${targetString(cfg.target)}`),
  ];

  // Not a platform check: every shell reads the same startup files wherever
  // it runs, so a second platform would only duplicate this.
  checks.push(shell.pathCheck(cfg));

  try {
    const host = await hostPlatform();
    checks.push(...(await host.doctor(cfg)));
  } catch (cause) {
    checks.push(
      doctorFail(
        "platform",
        reason(cause),
        "This build of ketch does not support this operating system.",
      ),
    );
  }
  checks.push(logCheck(cfg));
  checks.push(registryCheck(cfg));
  checks.push(...(await storeChecks(cfg)));

  for (const check of checks) {
    const mark =
      check.status === "ok"
        ? ui.green("ok  ")
        : check.status === "warn"
          ? ui.yellow("warn")
          : ui.red("fail");
    const name = check.status === "ok" ? ui.dim(check.name) : ui.bold(check.name);
    ui.out(`${mark} ${name}  ${check.detail}`);
    // The fix belongs with the problem, not in a summary the user has to
    // map back onto the list.
    if (check.fix !== null) {
      ui.out(`     ${ui.dim(check.fix)}`);
    }
  }

  if (worstStatus(checks) === "fail") {
    const failed = checks.filter((c) => c.status === "fail").length;
    throw KetchError.msg(`${failed} checks failed`);
  }
}

/**
 * Repair what `doctor` can repair on its own.
 *
 * Only the PATH setup qualifies today: it needs no network and no choice from
 * the user. Everything else doctor reports either is already a one-line
 * command or needs a decision ketch has no business making, and a `--fix`
 * that quietly reinstalls packages would be a worse tool than one that says
 * what to run.
 *
 * Failures are warned about rather than thrown: `doctor` exists to finish its
 * report even when part of the machine is broken.
 */
function fix(cfg: Config): void {
  if (binDirOnPath(cfg) || shell.configuredIn(cfg).length > 0) {
    return;
  }
  let shells: shell.Shell[];
  try {
    shells = shell.detect();
  } catch (cause) {
    ui.warn(reason(cause));
    return;
  }
  if (shells.length === 0) {
    ui.warn("could not tell which shell you use; run `ketch path install --shell <name>`");
    return;
  }
  for (const sh of shells) {
    try {
      report(shell.install(cfg, sh, false), false);
    } catch (cause) {
      ui.warn(`${sh}: ${reason(cause)}`);
    }
  }
}

/** Where this machine's log is, so nobody has to be told twice. */
function logCheck(cfg: Config): DoctorCheck {
  if (cfg.logLevel === "off") {
    return doctorOk("log", "off");
  }
  let size = "";
  try {
    size = ` · ${ui.bytes(fs.statSync(cfg.logFile).size)}`;
  } catch {
    // No log file yet is not a problem worth a line of its own.
  }
  return doctorOk("log", `${cfg.logFile} (${cfg.logLevel}, ${cfg.logFormat})${size}`);
}

function registryCheck(cfg: Config): DoctorCheck {
  if (!registry.exists(cfg)) {
    return doctorWarn("registry", `no local copy of ${cfg.registry}`, "Run `ketch update`.");
  }
  return doctorOk(
    "registry",
    `${registry.load(cfg, ui.warn).length} packages from ${cfg.registry}`,
  );
}

/**
 * Everything ketch itself owns: the store matches the state file, and every
 * link still points at the package that claims it.
 */
async function storeChecks(cfg: Config): Promise<DoctorCheck[]> {
  let state: State;
  try {
    state = await State.load(cfg);
  } catch (cause) {
    return [doctorFail("state", reason(cause), `Inspect or remove ${cfg.stateFile}.`)];
  }

  const checks: DoctorCheck[] = [];
  const missingPayloads: string[] = [];
  const brokenLinks: string[] = [];
  const packages = state.all();
  for (const pkg of packages) {
    if (!fs.existsSync(pkg.prefix)) {
      missingPayloads.push(pkg.name);
      // Its links cannot be sound either; one message per package is enough.
      continue;
    }
    for (const link of pkg.links) {
      // `existsSync` follows symlinks, so this catches both a deleted link and
      // one left dangling by a manual removal inside the store.
      if (!fs.existsSync(link.link)) {
        brokenLinks.push(`${pkg.name} -> ${link.link}`);
      }
    }
  }

  checks.push(
    missingPayloads.length === 0
      ? doctorOk("packages", `${packages.length} installed`)
      : doctorFail(
          "packages",
          `${missingPayloads.length} packages have no files: ${missingPayloads.join(", ")}`,
          `Run \`ketch install --force ${missingPayloads.join(" ")}\`.`,
        ),
  );

  if (brokenLinks.length > 0) {
    checks.push(
      doctorWarn(
        "links",
        `${brokenLinks.length} broken links: ${brokenLinks.join(", ")}`,
        "Run `ketch link <pkg>` to recreate them.",
      ),
    );
  }

  return checks;
}

/** Which shells have been set up, and where each one would be edited. */
export function pathStatus(cfg: Config): void {
  const check = shell.pathCheck(cfg);
  ui.out(`${ui.bold("bin")}  ${cfg.binDir}`);
  ui.out(`${ui.bold("now")}  ${binDirOnPath(cfg) ? "on PATH" : check.detail}`);

  const detected = detectedShells();
  const configured = shell.configuredIn(cfg);
  const home = homeDir();
  ui.table(
    ["shell", "state", "file"],
    shell.ALL_SHELLS.map((sh) => {
      const file = shell.configFile(sh, home);
      const state = configured.includes(file)
        ? "configured"
        : detected.includes(sh)
          ? "not set up"
          : "not in use";
      return [sh, state, file];
    }),
  );
}

/** Add the ketch block to the chosen shells' startup files. */
export async function pathInstall(
  cfg: Config,
  opts: { shell: string[]; all: boolean; dryRun: boolean; print: boolean },
): Promise<void> {
  if (opts.print) {
    ui.out(shell.manualLine(cfg));
    return;
  }
  let changed = false;
  for (const sh of chosen(opts)) {
    const change = shell.install(cfg, sh, opts.dryRun);
    changed ||= change.outcome !== "unchanged";
    report(change, opts.dryRun);
  }
  if (changed && !opts.dryRun) {
    ui.out("Open a new shell to pick it up.");
  }
}

/**
 * Take the block `ketch path install` added back out again.
 *
 * The config is unused — removing the block needs only the marker, never the
 * directory it names — but it stays in the signature so every `path` body is
 * called the same way.
 */
export async function pathUninstall(
  _cfg: Config,
  opts: { shell: string[]; all: boolean; dryRun: boolean },
): Promise<void> {
  for (const sh of chosen(opts)) {
    report(shell.uninstall(sh, opts.dryRun), opts.dryRun);
  }
}

/**
 * Which shells this invocation acts on: what was asked for, or what the
 * machine looks like.
 */
function chosen(opts: { shell: string[]; all: boolean }): shell.Shell[] {
  if (opts.all) {
    return [...shell.ALL_SHELLS];
  }
  if (opts.shell.length > 0) {
    // Rust drops only adjacent repeats (`Vec::dedup`); the shared helper drops
    // every repeat, and editing one startup file twice was never the point.
    return dedupe(opts.shell).map(asShell);
  }
  const found = shell.detect();
  if (found.length === 0) {
    // Guessing here would edit a startup file the user's shell never reads,
    // and they would have no reason to look for it.
    throw KetchError.msg(
      `could not tell which shell you use (SHELL=${process.env["SHELL"] ?? "unset"}). ` +
        "Pass --shell bash|zsh|fish, or --all, or `ketch path install --print` " +
        "for the line to add by hand.",
    );
  }
  return found;
}

/**
 * A `--shell` value as the union core speaks.
 *
 * Commander already limits the choices, so this only turns a guarantee the
 * parser holds into one the type system holds too.
 */
function asShell(name: string): shell.Shell {
  const found = shell.ALL_SHELLS.find((s) => s === name);
  if (found === undefined) {
    throw KetchError.msg(`unknown shell \`${name}\`; use bash, zsh or fish`);
  }
  return found;
}

/** The shells on this machine, or none when the home directory is unknown. */
function detectedShells(): shell.Shell[] {
  try {
    return shell.detect();
  } catch {
    return [];
  }
}

/** Where a shell would be edited, without editing it. */
function homeDir(): string {
  const dir = os.homedir();
  if (dir === "") {
    throw KetchError.msg("no home directory; set HOME");
  }
  return dir;
}

/** What a dry run would do, or what a real run did. */
const WOULD: Record<Exclude<shell.Outcome, "unchanged">, string> = {
  added: "would add",
  updated: "would update",
  removed: "would remove",
};

function report(change: shell.Change, dryRun: boolean): void {
  const detail = `${change.file} (${change.shell})`;
  if (change.outcome === "unchanged") {
    ui.step("unchanged", detail);
  } else if (dryRun) {
    ui.step(WOULD[change.outcome], detail);
  } else {
    // The outcomes are already the past-tense verbs the success line wants.
    ui.success(change.outcome, detail);
  }
}

/** Show the source plugins discovery found, and why any candidate failed. */
export async function pluginList(cfg: Config, json: boolean): Promise<void> {
  const rows: string[][] = [];
  const found: unknown[] = [];
  for (const result of await discoverPlugins(cfg)) {
    if (!result.ok) {
      // A plugin ketch cannot speak to is still worth naming: the alternative
      // is a scheme that silently does not exist.
      ui.warn(result.error.message);
      continue;
    }
    const p = result.plugin;
    rows.push([p.scheme, p.name, p.path]);
    found.push({ scheme: p.scheme, name: p.name, path: p.path, protocol: PROTOCOL_VERSION });
  }
  if (json) {
    ui.out(JSON.stringify(found, null, 2));
  } else if (rows.length === 0) {
    ui.out(`no plugins in ${cfg.pluginDir}`);
  } else {
    ui.table(["scheme", "plugin", "path"], rows);
  }
}

/** Print the directory plugins are loaded from. */
export function pluginDir(cfg: Config): void {
  ui.out(cfg.pluginDir);
}

/** Replace this binary with the latest release of ketch itself. */
export async function selfUpdate(
  cfg: Config,
  opts: { dryRun: boolean; force: boolean },
): Promise<void> {
  const result = await selfupdate.update(cfg, {
    version: VERSION,
    force: opts.force,
    dryRun: opts.dryRun,
    progress: ui.progress(),
    reporter: ui.reporter,
  });
  const move = `${result.from.raw} -> ${result.to.raw}`;
  if (result.replaced) {
    ui.success("updated", move);
  } else {
    ui.success(opts.dryRun ? "would update" : "already current", move);
  }
  if (result.notes !== null) {
    ui.out(result.notes);
  }
}

/**
 * Print the running version and where it lives.
 *
 * `cfg` is optional only because `cli.ts` calls this with no arguments; the
 * `target` and `root` lines the Rust command prints need it, so pass one to
 * get the whole answer.
 */
export function selfVersion(cfg: Config): void {
  ui.out(`ketch ${VERSION}`);
  ui.out(`target ${targetString(cfg.target)}`);
  ui.out(`root   ${cfg.root}`);
  ui.out(`binary ${selfupdate.currentExe()}`);
}

/** Remove ketch itself, and with `--purge` everything it installed. */
export async function selfUninstall(
  cfg: Config,
  opts: { purge: boolean; yes: boolean },
): Promise<void> {
  const question = opts.purge
    ? "remove ketch and everything it installed?"
    : "remove ketch itself? (installed packages are kept)";
  if (!(await ui.confirmOrYes(opts.yes, question, false))) {
    return;
  }
  for (const removed of await selfupdate.uninstallSelf(cfg, opts.purge, {
    reporter: ui.reporter,
  })) {
    ui.success("removed", removed);
  }
}

function reason(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
