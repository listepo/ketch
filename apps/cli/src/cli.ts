/**
 * The command-line surface — the port of `src/cli.rs`.
 *
 * Kept separate from `main.ts` so the bodies in `cmd/` take their own argument
 * objects directly, with no re-packing in between. This module also owns the
 * one piece of bootstrap every command shares: the `Config` is built in a
 * `preAction` hook, once, and handed to whichever body runs.
 */

import process from "node:process";
import { Argument, Command, Option } from "@commander-js/extra-typings";
import type { Config } from "@ketch/core";
import { ensureDirs, KetchError, loadConfig, init as logInit } from "@ketch/core";
import * as lock from "./cmd/lock.ts";
import * as pkg from "./cmd/pkg.ts";
import * as query from "./cmd/query.ts";
import * as system from "./cmd/system.ts";
import { completions, COMPLETION_SHELLS } from "./completions.ts";
import * as ui from "./ui.ts";

/**
 * The running version.
 *
 * `resolveJsonModule` is off across the workspace, so this cannot be imported
 * from `package.json`; `version.test.ts` asserts the two agree, which is the
 * check the Rust build got for free from `env!("CARGO_PKG_VERSION")`.
 */
export const VERSION = "0.2.0";

const ABOUT = "Catch releases straight from GitHub.";
const LONG_ABOUT =
  "ketch installs command-line tools and macOS apps directly from GitHub releases.\n" +
  "No taps, no formulae, no build step — it downloads what the project already ships.";

/** The config built by the `preAction` hook, for the body about to run. */
let current: Config | null = null;

/**
 * The config for this run.
 *
 * Non-null for every command except `completions`, which is answered before
 * any directory is built.
 */
function cfg(): Config {
  if (current === null) {
    throw KetchError.msg("internal: command ran before the config was built");
  }
  return current;
}

/** A `-j`/`-n` value: a positive count, or a clear complaint. */
function count(raw: string, flag: string): number {
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value < 1) {
    throw KetchError.msg(`${flag} takes a positive whole number, not \`${raw}\``);
  }
  return value;
}

/** Build the whole surface. Exported so tests and `completions` can read it. */
export function build(): Command {
  // Chained rather than built up statement by statement: `@commander-js/
  // extra-typings` infers the type of `program.opts()` from the chain, and a
  // discarded return value infers nothing.
  const program = new Command()
    .name("ketch")
    .description(ABOUT)
    .addHelpText("before", `${LONG_ABOUT}\n`)
    .version(VERSION)
    .helpCommand(false)
    .showHelpAfterError()
    .option("--root <DIR>", "ketch root directory (default: ~/.ketch)")
    .addOption(
      new Option("-v, --verbose", "Show what ketch is doing, including every request").conflicts(
        "quiet",
      ),
    )
    .option("-q, --quiet", "Only print errors and requested data")
    .option("--no-color", "Never emit ANSI colour");

  // Everything below the root shares one bootstrap: colour and verbosity are
  // set from the global flags, then the config is built and the log opened.
  // `completions` is deliberately excluded — it has to work before `~/.ketch`
  // exists, which is exactly when someone is setting ketch up.
  program.hook("preAction", async (_root, action) => {
    const global = program.opts();
    ui.init(global.color === false ? false : null, global.quiet === true, global.verbose === true);
    if (action.name() === "completions") {
      return;
    }
    const config = loadConfig({
      ...(global.root === undefined ? {} : { root: global.root }),
      // Without a sink the config loader's warnings — a `root` in the file
      // that cannot take effect, most of all — are discarded, and the user is
      // left wondering why their setting did nothing.
      warn: ui.warn,
    });
    await ensureDirs(config);
    logInit(config, VERSION);
    ui.debug(
      `root ${config.root} · target ${config.target} · token ${
        config.githubToken !== null ? "yes" : "no"
      }`,
    );
    current = config;
  });

  program
    .command("install")
    .alias("i")
    .description("Install one or more packages")
    .argument("<pkg...>", "`owner/repo`, `scheme:id`, or a known alias — each may carry `@version`")
    .option("-f, --force", "Reinstall even when the requested version is already present")
    .option("--pre", "Consider prereleases when resolving `latest`")
    .option("--no-link", "Unpack and record the package without putting it on PATH")
    .option("--require-checksum", "Refuse to install unless the release publishes a checksum")
    .option("--asset <NAME>", "Use this release asset by exact file name instead of auto-selecting")
    .option(
      "-j, --jobs <N>",
      "Packages to work on at once (default: 4, or `jobs` in config.json)",
      (raw) => count(raw, "--jobs"),
    )
    .option("-y, --yes", "Answer yes to every prompt")
    .action(async (packages, opts) => {
      await pkg.install(cfg(), {
        packages,
        force: opts.force === true,
        prerelease: opts.pre === true,
        noLink: opts.link === false,
        requireChecksum: opts.requireChecksum === true,
        asset: opts.asset ?? null,
        jobs: opts.jobs,
        yes: opts.yes === true,
      });
    });

  program
    .command("uninstall")
    .alias("remove")
    .alias("rm")
    .description("Remove installed packages")
    .argument("<name...>")
    .option("-y, --yes", "Answer yes to every prompt")
    .action(async (names, opts) => {
      await pkg.uninstall(cfg(), { names, yes: opts.yes === true });
    });

  program
    .command("list")
    .alias("ls")
    .description("Show installed packages")
    .option("--json", "Emit JSON instead of a table")
    .addOption(
      new Option("--names-only", "Print only package names, one per line").conflicts("json"),
    )
    .action(async (opts) => {
      await query.list(cfg(), { json: opts.json === true, namesOnly: opts.namesOnly === true });
    });

  program
    .command("outdated")
    .description("Show installed packages that have a newer release")
    .option("--json", "Emit JSON instead of a table")
    .option("--pre", "Compare against prereleases too")
    .action(async (opts) => {
      await query.outdated(cfg(), { json: opts.json === true, prerelease: opts.pre === true });
    });

  program
    .command("info")
    .alias("show")
    .description("Show details about a package, installed or not")
    .argument("<pkg>", "An installed name, an alias, or `owner/repo`")
    .option("--json", "Emit JSON instead of formatted text")
    .option("--assets", "List the release's assets and how each one scored")
    .action(async (target, opts) => {
      await query.info(cfg(), {
        package: target,
        json: opts.json === true,
        assets: opts.assets === true,
      });
    });

  program
    .command("changelog")
    .description("Show what changed: the package's own changelog, or its release notes")
    .argument("<pkg>", "An installed name, an alias, or `owner/repo` — may carry `@version`")
    .option("--latest", "Show the newest release instead of the installed one")
    .addOption(
      new Option("--file", "Only read the changelog file the package ships")
        .conflicts("release")
        .conflicts("latest"),
    )
    .option("--release", "Only read the notes published with the release")
    .action(async (target, opts) => {
      await query.changelog(cfg(), {
        package: target,
        latest: opts.latest === true,
        file: opts.file === true,
        release: opts.release === true,
      });
    });

  program
    .command("search")
    .description("Search GitHub for installable repositories")
    .argument("<query...>")
    .option("-n, --limit <N>", "Maximum results", (raw) => count(raw, "--limit"), 15)
    .action(async (words, opts) => {
      await query.search(cfg(), { query: words, limit: opts.limit });
    });

  program
    .command("update")
    .description("Refresh the package registry (see `upgrade` for installed packages)")
    .action(async () => {
      await system.update(cfg());
    });

  program
    .command("upgrade")
    .description("Upgrade installed packages to their latest release")
    .argument("[name...]", "Packages to upgrade. Empty means every unpinned package.")
    .option("--dry-run", "Report what would change without touching anything")
    .option("--pre", "Consider prereleases")
    .option("--force", "Upgrade pinned packages too")
    .option(
      "-j, --jobs <N>",
      "Packages to work on at once (default: 4, or `jobs` in config.json)",
      (raw) => count(raw, "--jobs"),
    )
    .option("-y, --yes", "Answer yes to every prompt")
    .action(async (names, opts) => {
      await pkg.upgrade(cfg(), {
        names,
        dryRun: opts.dryRun === true,
        prerelease: opts.pre === true,
        force: opts.force === true,
        jobs: opts.jobs,
        yes: opts.yes === true,
      });
    });

  program
    .command("pin")
    .description("Hold a package at its current version")
    .argument("<name...>")
    .action(async (names) => {
      await pkg.pin(cfg(), { names }, true);
    });

  program
    .command("unpin")
    .description("Release a pin")
    .argument("<name...>")
    .action(async (names) => {
      await pkg.pin(cfg(), { names }, false);
    });

  program
    .command("link")
    .description("Re-create the links for an installed package")
    .argument("<name...>")
    .action(async (names) => {
      await pkg.link(cfg(), { names }, true);
    });

  program
    .command("unlink")
    .description("Remove the links for an installed package, keeping it installed")
    .argument("<name...>")
    .action(async (names) => {
      await pkg.link(cfg(), { names }, false);
    });

  program
    .command("lock")
    .description("Write or check `ketch.lock`, a reproducible record of what is installed")
    .option("-f, --file <FILE>", "Lockfile to write (default: ./ketch.lock)")
    .option("--check", "Report how the tree differs from the lockfile, and write nothing")
    .action(async (opts) => {
      await lock.write(cfg(), { file: opts.file ?? null, check: opts.check === true });
    });

  program
    .command("sync")
    .description("Install everything `ketch.lock` names, at the versions it names")
    .option("-f, --file <FILE>", "Lockfile to read (default: ./ketch.lock)")
    .option("--prune", "Also remove installed packages the lockfile does not name")
    .option("--dry-run", "Report what would change without installing anything")
    .option(
      "-j, --jobs <N>",
      "Packages to work on at once (default: 4, or `jobs` in config.json)",
      (raw) => count(raw, "--jobs"),
    )
    .option("-y, --yes", "Answer yes to every prompt")
    .action(async (opts) => {
      await lock.sync(cfg(), {
        file: opts.file ?? null,
        prune: opts.prune === true,
        dryRun: opts.dryRun === true,
        jobs: opts.jobs,
        yes: opts.yes === true,
      });
    });

  program
    .command("doctor")
    .description("Check the environment and the install tree")
    .option("--fix", "Repair what can be repaired without asking: currently the PATH setup")
    .action(async (opts) => {
      await system.doctor(cfg(), { fix: opts.fix === true });
    });

  buildPath(program);
  buildPlugin(program);
  buildSelf(program);

  program
    .command("completions")
    .description("Print a shell completion script")
    .addArgument(new Argument("<shell>").choices(COMPLETION_SHELLS))
    .action((shell) => {
      ui.out(completions(program, shell));
    });

  return program;
}

/** The `--shell` option, shared by `path install` and `path uninstall`. */
const shells = () =>
  new Option(
    "--shell <SHELL...>",
    "Shells to act on. Default: the ones you appear to use.",
  ).choices(["bash", "zsh", "fish"] as const);

/** `ketch path` — the shell startup file, the one thing ketch writes outside its root. */
function buildPath(program: Command): void {
  const path = program
    .command("path")
    .description("Put the ketch bin directory on your shell's PATH");

  path
    .command("install")
    .description("Add the bin directory to your shell's startup file")
    .addOption(shells())
    .addOption(new Option("--all", "Act on every shell ketch knows").conflicts("shell"))
    .option("--dry-run", "Report what would change without touching anything")
    .addOption(
      new Option("--print", "Print the line to add by hand instead of editing anything")
        .conflicts("all")
        .conflicts("dryRun"),
    )
    .action(async (opts) => {
      await system.pathInstall(cfg(), {
        shell: opts.shell ?? [],
        all: opts.all === true,
        dryRun: opts.dryRun === true,
        print: opts.print === true,
      });
    });

  path
    .command("uninstall")
    .description("Take out the block `ketch path install` added")
    .addOption(shells())
    .addOption(new Option("--all", "Act on every shell ketch knows").conflicts("shell"))
    .option("--dry-run", "Report what would change without touching anything")
    .action(async (opts) => {
      await system.pathUninstall(cfg(), {
        shell: opts.shell ?? [],
        all: opts.all === true,
        dryRun: opts.dryRun === true,
      });
    });

  path
    .command("status", { isDefault: true })
    .description("Show which shells have been set up")
    .action(() => {
      system.pathStatus(cfg());
    });
}

/** `ketch plugin` — what the source registry found on PATH and in the plugins dir. */
function buildPlugin(program: Command): void {
  const plugin = program.command("plugin").description("Manage source plugins");

  plugin
    .command("list")
    .description("Show discovered source plugins")
    .option("--json", "Emit JSON instead of a table")
    .action(async (opts) => {
      await system.pluginList(cfg(), opts.json === true);
    });

  plugin
    .command("dir")
    .description("Print the directory plugins are loaded from")
    .action(() => {
      system.pluginDir(cfg());
    });
}

/** `ketch self` — the host app, as opposed to everything it installs. */
function buildSelf(program: Command): void {
  const zelf = program.command("self").description("Manage ketch itself");

  zelf
    .command("update")
    .description("Replace this binary with the latest release")
    .option("--dry-run", "Report what would happen without replacing anything")
    .option("-f, --force", "Reinstall even when already current")
    .action(async (opts) => {
      await system.selfUpdate(cfg(), {
        dryRun: opts.dryRun === true,
        force: opts.force === true,
      });
    });

  zelf
    .command("version")
    .description("Print the running version and where it lives")
    .action(() => {
      system.selfVersion(cfg());
    });

  zelf
    .command("uninstall")
    .description("Remove ketch itself")
    .option("--purge", "Also delete the store, cache and state")
    .option("-y, --yes", "Answer yes to every prompt")
    .action(async (opts) => {
      await system.selfUninstall(cfg(), {
        purge: opts.purge === true,
        yes: opts.yes === true,
      });
    });
}

/** Parse and run. Separated from `main` so tests can drive it with fake argv. */
export async function run(argv: readonly string[] = process.argv): Promise<void> {
  await build().parseAsync(argv as string[]);
}
