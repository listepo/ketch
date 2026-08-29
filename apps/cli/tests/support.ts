/**
 * Scaffolding for the end-to-end tests: a throwaway ketch root, fixture
 * archives that stand in for real release assets, and a source plugin that
 * serves them.
 *
 * Everything here is offline on purpose. The plugin protocol already lets a
 * source hand ketch an asset it fetched itself (`docs/PLUGINS.md`), so a
 * twenty-line shell script is enough to play the part of a release host — no
 * network, no fixtures checked into the tree, and no test that depends on
 * somebody else's tag still existing.
 *
 * Port of `tests/support/mod.rs`, plus the fixture builders `tests/install.rs`
 * kept to itself — the suite may be split across files here, so they are
 * shared. Two deliberate departures from the Rust harness: archives are packed
 * by the system `tar` and `zip`, which ship with macOS — the only platform the
 * suite runs on, like the platform layer it exercises — and the CLI under test
 * is `apps/cli/src/main.ts` run by whichever runtime is running the suite;
 * `process.execPath` is what keeps that working under Node and Bun alike.
 */

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";

/** The CLI entry the sandbox spawns. */
export const CLI_MAIN = path.resolve(import.meta.dirname, "..", "src", "main.ts");

/**
 * A compiled binary to exercise instead of the sources, when one is named.
 *
 * The released artifact is not the same program as `main.ts` handed to a
 * runtime: it is bundled, it resolves modules differently, and it is what
 * users actually run. Pointing the whole suite at it — rather than smoking it
 * with `--version` — is the only way a bundling failure is found before a tag
 * goes out instead of after.
 */
const COMPILED_BINARY = process.env["KETCH_E2E_BINARY"];

/** What one ketch run left behind. `status` is null when a signal killed it. */
export interface RunResult {
  readonly status: number | null;
  readonly stdout: string;
  readonly stderr: string;
}

/**
 * A ketch root, applications directory and plugin, all inside one temp dir.
 * Vitest has no `Drop`, so test files call `dispose` from `afterEach`.
 */
export class Sandbox {
  private readonly tmp: string;

  constructor() {
    this.tmp = fs.mkdtempSync(path.join(os.tmpdir(), "ketch-e2e-"));
    for (const dir of [this.root(), this.apps(), this.pluginDir(), this.assetsDir(), this.home()]) {
      fs.mkdirSync(dir, { recursive: true });
    }
    this.writePlugin();
  }

  /** Remove the whole tree. */
  dispose(): void {
    fs.rmSync(this.tmp, { recursive: true, force: true });
  }

  root(): string {
    return path.join(this.tmp, "root");
  }

  apps(): string {
    return path.join(this.tmp, "Applications");
  }

  bin(): string {
    return path.join(this.root(), "bin");
  }

  store(): string {
    return path.join(this.root(), "store");
  }

  /**
   * A home directory of its own, so a test that edits shell startup files
   * cannot reach the one belonging to whoever is running the suite.
   */
  home(): string {
    return path.join(this.tmp, "home");
  }

  /** Where the run's log lands. */
  log(): string {
    try {
      return fs.readFileSync(path.join(this.root(), "logs", "ketch.log"), "utf8");
    } catch {
      return "";
    }
  }

  /** Write `config.json` for this root, for settings with no flag. */
  configure(config: object): void {
    fs.writeFileSync(path.join(this.root(), "config.json"), JSON.stringify(config));
  }

  private pluginDir(): string {
    return path.join(this.root(), "plugins");
  }

  /** Where fixture assets and the JSON the plugin serves both live. */
  private assetsDir(): string {
    return path.join(this.tmp, "assets");
  }

  /**
   * Run ketch against this sandbox. The environment is set per-invocation
   * rather than process-wide, so tests stay safe to run in parallel.
   */
  run(args: readonly string[]): RunResult {
    // The bin dir on PATH is the configuration ketch is installed into;
    // `doctor` is right to fail without it.
    return this.runWith(args, this.pathWithBin());
  }

  /**
   * Run ketch with the sandbox bin dir left off PATH, which is what an
   * install that has not been wired into a shell yet actually looks like.
   */
  runOffPath(args: readonly string[]): RunResult {
    return this.runWith(args, process.env["PATH"] ?? "");
  }

  private runWith(args: readonly string[], pathVar: string): RunResult {
    const env: Record<string, string> = {};
    for (const [key, value] of Object.entries(process.env)) {
      if (value !== undefined) {
        env[key] = value;
      }
    }
    // A token in the ambient environment (CI always has one) must not reach a
    // test: nothing here is allowed to touch the network.
    for (const key of [
      "ZDOTDIR",
      "XDG_CONFIG_HOME",
      "KETCH_GITHUB_TOKEN",
      "GITHUB_TOKEN",
      "GH_TOKEN",
    ]) {
      delete env[key];
    }
    env["KETCH_ROOT"] = this.root();
    env["KETCH_APPS_DIR"] = this.apps();
    env["NO_COLOR"] = "1";
    env["PATH"] = pathVar;
    // Shell setup writes into `$HOME`. Pointing it at the sandbox is what
    // keeps the suite from editing a real `.zshrc`.
    env["HOME"] = this.home();
    env["SHELL"] = "/bin/zsh";
    const out =
      COMPILED_BINARY === undefined
        ? spawnSync(process.execPath, [CLI_MAIN, ...args], { env, encoding: "utf8" })
        : spawnSync(COMPILED_BINARY, args, { env, encoding: "utf8" });
    if (out.error !== undefined) {
      throw new Error(`could not run ketch: ${out.error.message}`);
    }
    return { status: out.status, stdout: out.stdout, stderr: out.stderr };
  }

  /** `PATH` with the sandbox bin dir in front of the inherited one. */
  private pathWithBin(): string {
    return [this.bin(), ...(process.env["PATH"] ?? "").split(path.delimiter)].join(path.delimiter);
  }

  /** Run ketch and fail the test with its full output if it did not succeed. */
  ok(args: readonly string[]): string {
    const out = this.run(args);
    if (out.status !== 0) {
      throw new Error(
        `\`ketch ${args.join(" ")}\` failed with ${String(out.status)}\n` +
          `--- stdout ---\n${out.stdout}\n--- stderr ---\n${out.stderr}`,
      );
    }
    return out.stdout;
  }

  /** Run ketch expecting failure, returning stderr. */
  fail(args: readonly string[]): string {
    const out = this.run(args);
    if (out.status === 0) {
      throw new Error(
        `\`ketch ${args.join(" ")}\` was expected to fail but succeeded\n` +
          `--- stdout ---\n${out.stdout}`,
      );
    }
    return out.stderr;
  }

  /** Publish the releases the test plugin will serve for `id`. */
  publish(id: string, releases: readonly Release[]): void {
    fs.writeFileSync(path.join(this.assetsDir(), `${id}.releases.json`), JSON.stringify(releases));
  }

  /** Build a release asset on disk and describe it for the plugin. */
  asset(name: string, archive: Archive): Asset {
    const assetPath = path.join(this.assetsDir(), name);
    writeArchive(archive, assetPath, this.tmp);
    return new Asset(name, assetPath, sha256File(assetPath));
  }

  /**
   * Describe an asset `asset()` already built, without repacking it. The Rust
   * harness rebuilt fixtures byte-for-byte because its archive writer was
   * deterministic; the system `tar` is not, so a test whose claim depends on
   * serving the same bytes a lockfile recorded reuses the file instead.
   */
  publishedAsset(name: string): Asset {
    const assetPath = path.join(this.assetsDir(), name);
    return new Asset(name, assetPath, sha256File(assetPath));
  }

  private writePlugin(): void {
    const caps = '{"protocol":1,"scheme":"test","download":true,"search":false}';
    const script =
      "#!/bin/sh\n" +
      "set -eu\n" +
      `DB=${shellQuote(this.assetsDir())}\n` +
      'case "$1" in\n' +
      `capabilities) printf '%s' '${caps}' ;;\n` +
      "describe) printf 'null' ;;\n" +
      'releases) cat "$DB/$2.releases.json" ;;\n' +
      "search) printf '[]' ;;\n" +
      'download) cp "$2" "$3" ;;\n' +
      '*) echo "unsupported subcommand: $1" >&2; exit 1 ;;\n' +
      "esac\n";
    const pluginPath = path.join(this.pluginDir(), "ketch-source-test");
    fs.writeFileSync(pluginPath, script);
    fs.chmodSync(pluginPath, 0o755);
  }
}

/** One release the plugin will report, in the wire shape of `docs/PLUGINS.md`. */
export class Release {
  private notes: string | null = null;

  private readonly version: string;
  private readonly assets: readonly Asset[];

  constructor(version: string, assets: readonly Asset[]) {
    this.version = version;
    this.assets = assets;
  }

  /** Notes published alongside the release, the way a forge serves them. */
  withNotes(notes: string): Release {
    this.notes = notes;
    return this;
  }

  toJSON(): object {
    return {
      version: this.version,
      tag: `v${this.version}`,
      prerelease: false,
      draft: false,
      ...(this.notes === null ? {} : { notes: this.notes }),
      assets: this.assets,
    };
  }
}

/** A fixture asset: a real file on disk, plus the digest the plugin publishes. */
export class Asset {
  readonly name: string;
  readonly filePath: string;
  private readonly digest: string | null;

  constructor(name: string, filePath: string, digest: string | null) {
    this.name = name;
    this.filePath = filePath;
    this.digest = digest;
  }

  /**
   * Publish a digest that does not match the bytes, so the install is
   * rejected the way a tampered download would be.
   */
  withWrongDigest(): Asset {
    return new Asset(this.name, this.filePath, "0".repeat(64));
  }

  toJSON(): object {
    return {
      name: this.name,
      // The plugin downloads, so `url` is just the path it copies from.
      url: this.filePath,
      ...(this.digest === null ? {} : { digest: { algo: "sha256", hex: this.digest } }),
    };
  }
}

/** One file inside a fixture archive. */
export interface Entry {
  readonly path: string;
  readonly body: string;
  readonly mode: number;
}

export const Entry = {
  file(entryPath: string, body: string): Entry {
    return { path: entryPath, body, mode: 0o644 };
  },

  /**
   * An executable that prints `says` when run, so a test can prove the thing
   * on PATH is the thing that was installed.
   */
  program(entryPath: string, says: string): Entry {
    return { path: entryPath, body: `#!/bin/sh\necho '${says}'\n`, mode: 0o755 };
  },
};

/** A fixture archive, in one of the formats ketch sniffs for. */
export interface Archive {
  readonly format: "tar.gz" | "zip";
  readonly entries: readonly Entry[];
}

export const Archive = {
  tarGz(entries: readonly Entry[]): Archive {
    return { format: "tar.gz", entries };
  },

  zip(entries: readonly Entry[]): Archive {
    return { format: "zip", entries };
  },
};

/**
 * Pack an archive with the system `tar`/`zip`. Staging real files first is
 * what carries each entry's mode into the archive.
 */
function writeArchive(archive: Archive, dest: string, scratch: string): void {
  const staging = fs.mkdtempSync(path.join(scratch, "stage-"));
  try {
    for (const entry of archive.entries) {
      const entryPath = path.join(staging, entry.path);
      fs.mkdirSync(path.dirname(entryPath), { recursive: true });
      fs.writeFileSync(entryPath, entry.body);
      fs.chmodSync(entryPath, entry.mode);
    }
    // Naming the top-level members keeps them recorded without a `./` prefix.
    const members = [...new Set(archive.entries.map((entry) => firstComponent(entry.path)))];
    const packed =
      archive.format === "tar.gz"
        ? spawnSync("tar", ["-czf", dest, "-C", staging, ...members], { encoding: "utf8" })
        : spawnSync("zip", ["-q", "-r", "-y", dest, ...members], {
            cwd: staging,
            encoding: "utf8",
          });
    if (packed.status !== 0) {
      throw new Error(`packing ${dest} failed: ${packed.stderr}`);
    }
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}

function firstComponent(entryPath: string): string {
  const [first] = entryPath.split("/");
  return first ?? entryPath;
}

function sha256File(filePath: string): string {
  return createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");
}

/**
 * Single-quote a path for the plugin script. Temp dirs contain no quotes, but
 * the script is generated rather than hand-written, so it should not assume so.
 */
function shellQuote(quoted: string): string {
  return `'${quoted.replaceAll("'", "'\\''")}'`;
}

/**
 * The architecture token naming an asset this machine runs natively — the
 * Rust target spelling (`aarch64`), which is what asset selection scores.
 */
export function hostArch(): string {
  return process.arch === "arm64" ? "aarch64" : "x86_64";
}

/** Split into lines the way Rust's `str::lines` does: no trailing empty one. */
export function lines(text: string): string[] {
  const all = text.split("\n");
  if (all.at(-1) === "") {
    all.pop();
  }
  return all;
}

// ---------------------------------------------------------------------------
// Shared fixtures, ported from the head of `tests/install.rs`
// ---------------------------------------------------------------------------

/**
 * The same changelog whatever version ships it, so a test can prove the
 * section that gets printed is the one for the version installed.
 */
export const CHANGELOG = `# Changelog

## [2.0.0] - 2024-06-01

- the second one

## [1.0.0] - 2024-05-01

- the first one
`;

/**
 * A command-line tool, shaped like a real release tarball: a version-stamped
 * wrapper directory with the binary under `bin/`.
 */
export function toolArchive(version: string): Archive {
  return Archive.tarGz([
    Entry.program(`testtool-${version}/bin/testtool`, `testtool ${version}`),
    Entry.file(`testtool-${version}/README.md`, "# testtool\n"),
    Entry.file(`testtool-${version}/CHANGELOG.md`, CHANGELOG),
  ]);
}

/** A macOS app, shaped like a real release zip: the bundle alone at the root. */
export function appArchive(version: string): Archive {
  return Archive.zip([
    Entry.file(
      "TestApp.app/Contents/Info.plist",
      '<?xml version="1.0"?><plist version="1.0"><dict></dict></plist>',
    ),
    Entry.program("TestApp.app/Contents/MacOS/TestApp", `app ${version}`),
  ]);
}

/**
 * Publish `testtool` at one version, with a decoy for every other platform so
 * asset selection is doing real work rather than picking the only candidate.
 */
export function publishTool(sandbox: Sandbox, version: string): void {
  const arch = hostArch();
  const native = sandbox.asset(
    `testtool-${version}-${arch}-apple-darwin.tar.gz`,
    toolArchive(version),
  );
  const linux = sandbox.asset(
    `testtool-${version}-${arch}-unknown-linux-gnu.tar.gz`,
    toolArchive("linux-decoy"),
  );
  sandbox.publish("testtool", [
    new Release(version, [linux, native]).withNotes(`published notes for ${version}`),
  ]);
}

/** One tool per name, so a batch has several distinct packages to install. */
export function publishNamed(sandbox: Sandbox, name: string, version: string): void {
  const asset = sandbox.asset(
    `${name}-${version}-${hostArch()}-apple-darwin.tar.gz`,
    Archive.tarGz([Entry.program(`${name}-${version}/bin/${name}`, `${name} ${version}`)]),
  );
  sandbox.publish(name, [new Release(version, [asset])]);
}

/** Run an installed program and return its trimmed stdout. */
export function runProgram(programPath: string): string {
  const out = spawnSync(programPath, { encoding: "utf8" });
  if (out.status !== 0) {
    throw new Error(`${programPath} did not run`);
  }
  return out.stdout.trim();
}
