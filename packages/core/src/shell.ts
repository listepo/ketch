/**
 * Putting the ketch bin directory on PATH, in the user's own shell.
 *
 * Separate from `platform/` because what has to be edited is a shell's
 * startup file, not an operating system's: bash, zsh and fish read the same
 * files wherever they run, so a Linux or Windows backend inherits all of this
 * unchanged.
 *
 * This is the only code in ketch that writes outside the ketch root, and it
 * runs only when the user asks for it — `ketch path install`, or
 * `ketch doctor --fix`. Everything it adds sits between two markers so it can
 * be found again, rewritten in place, and taken back out without guessing.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import type { Config } from "./config.ts";
import { binDirOnPath } from "./config.ts";
import { KetchError } from "./errors.ts";
import type { DoctorCheck } from "./platform/platform.ts";
import { doctorFail, doctorOk, doctorWarn } from "./platform/platform.ts";

/** Opens the block ketch owns. Must begin a line and end one. */
export const BEGIN = "# >>> ketch >>>";
/** Closes the block ketch owns. */
export const END = "# <<< ketch <<<";

/**
 * A shell whose PATH ketch knows how to set up.
 *
 * Anything else is handled by printing the line to add by hand: a shell whose
 * quoting rules are not implemented here would be edited wrongly, and a
 * broken startup file costs the user more than a manual paste.
 *
 * A `Shell` value is already the name `$SHELL` ends with and the one the user
 * types, so unlike the Rust `Shell::name()` method no separate accessor is
 * needed for it.
 */
export type Shell = "bash" | "zsh" | "fish";

/** Every shell ketch can configure. */
export const ALL_SHELLS: readonly Shell[] = ["bash", "zsh", "fish"];

/**
 * The shell a program path refers to, or `null` for one ketch cannot set up.
 *
 * Both the directory and the leading `-` that marks a login shell are
 * ignored, because `$SHELL` and `argv[0]` disagree about both.
 */
export function fromProgram(program: string): Shell | null {
  const segments = program.split("/");
  const base = segments[segments.length - 1] ?? program;
  const trimmed = base.replace(/^-+/, "");
  switch (trimmed) {
    case "bash":
      return "bash";
    case "zsh":
      return "zsh";
    case "fish":
      return "fish";
    default:
      return null;
  }
}

/**
 * Files this shell may already be configured in, most preferred first.
 *
 * bash is the awkward one: a terminal on macOS starts a login shell, which
 * reads `.bash_profile` and never `.bashrc`, while most Linux terminals do
 * the reverse. Ordering by what the host actually starts is what stops
 * ketch writing into a file nothing reads.
 */
export function candidates(shell: Shell, homeDir: string): string[] {
  const bash =
    process.platform === "darwin" ? [".bash_profile", ".bashrc"] : [".bashrc", ".bash_profile"];
  switch (shell) {
    case "bash":
      return bash.map((f) => path.join(homeDir, f));
    case "zsh":
      return [path.join(zdotdir(homeDir), ".zshrc")];
    case "fish":
      return [path.join(configHome(homeDir), "fish", "config.fish")];
  }
}

/**
 * The file to edit: the first candidate that already exists, else the one
 * this host would create.
 */
export function configFile(shell: Shell, homeDir: string): string {
  const options = candidates(shell, homeDir);
  // Unreachable while `candidates` returns a non-empty list, and a sane
  // answer rather than a crash if it ever stops.
  return options.find((p) => isFile(p)) ?? options[0] ?? path.join(homeDir, ".profile");
}

/** The one line that does the work. */
export function exportLine(shell: Shell, binDir: string): string {
  switch (shell) {
    case "bash":
    case "zsh":
      // A single-quoted literal joined to "$PATH" keeps every character of
      // the directory: a path holding a space, a `$` or a quote still
      // expands to exactly itself.
      return `export PATH=${quotePosix(binDir)}:"$PATH"`;
    case "fish":
      // Deliberately not `fish_add_path`: that writes a universal variable,
      // which outlives the file ketch is editing and would survive
      // `ketch path uninstall`.
      return `set -gx PATH ${quoteFish(binDir)} $PATH`;
  }
}

/** The whole block ketch owns, markers included. */
export function block(shell: Shell, binDir: string): string {
  return `${BEGIN}\n${exportLine(shell, binDir)}\n${END}\n`;
}

/**
 * What `install` or `uninstall` did to one shell's config file.
 *
 * - `added` — the block was written for the first time.
 * - `updated` — an existing ketch block named a different directory and was
 *   rewritten.
 * - `removed` — the block was taken out again.
 * - `unchanged` — nothing to do: already correct, or the user had set it up
 *   by hand.
 */
export type Outcome = "added" | "updated" | "removed" | "unchanged";

/** One shell's config file, and what happened to it. */
export interface Change {
  readonly shell: Shell;
  readonly file: string;
  readonly outcome: Outcome;
}

/** The login shell, from `$SHELL`. */
export function current(): Shell | null {
  const shell = process.env["SHELL"];
  return shell === undefined ? null : fromProgram(shell);
}

/**
 * Shells worth configuring on this machine: the login shell, plus any whose
 * config file the user already keeps.
 *
 * A shell that is neither is left alone. Creating a startup file for a shell
 * nobody runs is litter, and `--shell` says so explicitly when it is wanted.
 */
export function detect(): Shell[] {
  const homeDir = home();
  const currentShell = current();
  return ALL_SHELLS.filter(
    (s) => s === currentShell || candidates(s, homeDir).some((p) => isFile(p)),
  );
}

/**
 * Add the block to one shell's config file.
 *
 * `dryRun` computes the outcome and writes nothing.
 */
export function install(cfg: Config, shell: Shell, dryRun: boolean): Change {
  const binDir = binDirStr(cfg);
  const file = configFile(shell, home());
  const text = readShellFile(file);
  const hadBlock = blockSpan(text) !== null;

  // A line the user wrote themselves already does the job. A second copy
  // would be both redundant and impossible to tell from theirs later.
  if (!hadBlock && mentions(text, binDir)) {
    return { shell, file, outcome: "unchanged" };
  }

  const spliced = splice(text, block(shell, binDir));
  let outcome: Outcome;
  if (spliced === null) {
    outcome = "unchanged";
  } else {
    if (!dryRun) {
      writeShellFile(file, spliced);
    }
    outcome = hadBlock ? "updated" : "added";
  }
  return { shell, file, outcome };
}

/**
 * Take the block back out of one shell's config file, leaving everything the
 * user wrote exactly as it was.
 */
export function uninstall(shell: Shell, dryRun: boolean): Change {
  const file = configFile(shell, home());
  const text = readShellFile(file);
  const next = unsplice(text);
  let outcome: Outcome;
  if (next === null) {
    outcome = "unchanged";
  } else {
    if (!dryRun) {
      writeShellFile(file, next);
    }
    outcome = "removed";
  }
  return { shell, file, outcome };
}

/**
 * Config files that already put the bin dir on PATH, whether ketch wrote them
 * or the user did.
 *
 * An unreadable file is not configured as far as anyone can tell, so it is
 * skipped rather than reported: this feeds a diagnostic, not a decision.
 */
export function configuredIn(cfg: Config): string[] {
  let homeDir: string;
  let binDir: string;
  try {
    homeDir = home();
    binDir = binDirStr(cfg);
  } catch {
    return [];
  }
  return ALL_SHELLS.flatMap((s) => candidates(s, homeDir)).filter((p) => {
    try {
      return mentions(fs.readFileSync(p, "utf8"), binDir);
    } catch {
      return false;
    }
  });
}

/**
 * The PATH line of `ketch doctor`.
 *
 * Three states, not two. A bin dir that is written into `.zshrc` but missing
 * from this process's environment is not broken — the shell that started
 * ketch simply predates the edit — and calling that a failure sends the user
 * round the same loop forever.
 */
export function pathCheck(cfg: Config): DoctorCheck {
  const bin = cfg.binDir;
  if (binDirOnPath(cfg)) {
    return doctorOk("PATH", `${bin} is on PATH`);
  }
  const configured = configuredIn(cfg);
  if (configured.length === 0) {
    return doctorFail(
      "PATH",
      `${bin} is not on PATH`,
      "Run `ketch path install`, or `ketch doctor --fix`.",
    );
  }
  return doctorWarn(
    "PATH",
    `${bin} is set up in ${configured.join(", ")} but not in this shell`,
    "Open a new shell.",
  );
}

/** The line to add by hand, for a shell ketch does not know. */
export function manualLine(cfg: Config): string {
  return exportLine("bash", binDirStr(cfg));
}

function home(): string {
  const dir = os.homedir();
  if (dir === "") {
    throw KetchError.msg("no home directory; set HOME");
  }
  return dir;
}

/**
 * zsh reads its files from `$ZDOTDIR` when that is set, and only falls back
 * to the home directory when it is not.
 */
function zdotdir(homeDir: string): string {
  const value = process.env["ZDOTDIR"];
  return value !== undefined && path.isAbsolute(value) ? value : homeDir;
}

function configHome(homeDir: string): string {
  const value = process.env["XDG_CONFIG_HOME"];
  return value !== undefined && path.isAbsolute(value) ? value : path.join(homeDir, ".config");
}

/**
 * The bin dir as something a shell file can hold.
 *
 * A newline is the one failure no amount of quoting fixes, so it is refused
 * rather than written and hoped for. (Rust also guards against a `bin_dir`
 * that is not valid UTF-8; a JS string is already Unicode text rather than
 * raw path bytes, so that case cannot arise here.)
 */
function binDirStr(cfg: Config): string {
  if (cfg.binDir.includes("\n")) {
    throw KetchError.msg(`${cfg.binDir} contains a newline; no shell can express that on one line`);
  }
  return cfg.binDir;
}

/**
 * Single-quote for the POSIX family, where the only character that cannot
 * appear inside single quotes is the single quote itself.
 */
export function quotePosix(text: string): string {
  return `'${text.replaceAll("'", "'\\''")}'`;
}

/**
 * Single-quote for fish, which unlike POSIX honours backslash escapes inside
 * single quotes — so a literal backslash has to be doubled.
 */
export function quoteFish(text: string): string {
  return `'${text.replaceAll("\\", "\\\\").replaceAll("'", "\\'")}'`;
}

/**
 * True when some line the shell will actually run names this directory.
 *
 * Comments are skipped so that a file still carrying a commented-out attempt,
 * or ketch's own markers, does not read as configured.
 */
export function mentions(text: string, binDir: string): boolean {
  return splitLines(text)
    .filter((line) => !line.trimStart().startsWith("#"))
    .some((line) => line.includes(binDir));
}

/**
 * The `[start, end)` range of the ketch block within `text`, markers and
 * trailing newline included.
 */
export function blockSpan(text: string): [number, number] | null {
  const start = markerAtLineStart(text, BEGIN, 0);
  if (start === null) {
    return null;
  }
  const endLine = markerAtLineStart(text, END, start + BEGIN.length);
  if (endLine === null) {
    return null;
  }
  let end = endLine + END.length;
  if (text.slice(end).startsWith("\n")) {
    end += 1;
  }
  return [start, end];
}

/**
 * Offset of `marker` where it occupies a whole line, at or after `from`.
 *
 * Whole-line matching is what keeps a marker quoted inside somebody's own
 * script from being mistaken for the block ketch owns.
 */
function markerAtLineStart(text: string, marker: string, from: number): number | null {
  let index = text.indexOf(marker, from);
  while (index !== -1) {
    const startsLine = index === 0 || text[index - 1] === "\n";
    const after = index + marker.length;
    const endsLine = after >= text.length || text[after] === "\n";
    if (startsLine && endsLine) {
      return index;
    }
    index = text.indexOf(marker, index + 1);
  }
  return null;
}

/**
 * Put `newBlock` into `text`, replacing any block already there. `null` means
 * the file already says exactly this.
 */
export function splice(text: string, newBlock: string): string | null {
  const span = blockSpan(text);
  if (span !== null) {
    const [start, end] = span;
    const next = text.slice(0, start) + newBlock + text.slice(end);
    return next !== text ? next : null;
  }
  let next = text;
  if (next !== "") {
    if (!next.endsWith("\n")) {
      next += "\n";
    }
    next += "\n";
  }
  next += newBlock;
  return next;
}

/** Take the block out. `null` means there was none. */
export function unsplice(text: string): string | null {
  const span = blockSpan(text);
  if (span === null) {
    return null;
  }
  const [start, end] = span;
  let head = text.slice(0, start);
  // The blank line that was inserted ahead of the block goes back out with
  // it, so installing and uninstalling repeatedly cannot grow the file.
  if (head.endsWith("\n")) {
    const shortened = head.slice(0, -1);
    if (shortened.endsWith("\n")) {
      head = shortened;
    }
  }
  return head + text.slice(end);
}

/** A missing config file reads as empty: it is about to be created. */
function readShellFile(file: string): string {
  try {
    return fs.readFileSync(file, "utf8");
  } catch (cause) {
    if (errnoCode(cause) === "ENOENT") {
      return "";
    }
    throw KetchError.io(file, asError(cause));
  }
}

/** Replace the file's contents, atomically and in place. */
function writeShellFile(file: string, text: string): void {
  // A startup file is very often a symlink into a dotfiles repository.
  // Renaming over the link would replace it with a regular file and quietly
  // detach the user from their own dotfiles, so the write follows it first.
  let target: string;
  try {
    target = fs.realpathSync(file);
  } catch {
    target = file;
  }

  const parent = path.dirname(target);
  if (parent === target) {
    throw KetchError.msg(`${target} has no parent directory`);
  }
  try {
    fs.mkdirSync(parent, { recursive: true });
  } catch (cause) {
    throw KetchError.io(parent, asError(cause));
  }

  const name = path.basename(target) || "profile";
  const tmp = path.join(parent, `.${name}.ketch-tmp`);

  try {
    fs.writeFileSync(tmp, text, "utf8");
  } catch (cause) {
    throw KetchError.io(tmp, asError(cause));
  }

  // A startup file the user made private must not come back world-readable.
  try {
    fs.chmodSync(tmp, fs.statSync(target).mode);
  } catch {
    // No existing file to match permissions with, or the chmod failed;
    // either way the tmp file keeps the mode it was created with.
  }

  try {
    fs.renameSync(tmp, target);
  } catch (cause) {
    try {
      fs.rmSync(tmp, { force: true });
    } catch {
      // Best effort; the rename error below is what matters.
    }
    throw KetchError.io(target, asError(cause));
  }
}

function isFile(p: string): boolean {
  try {
    return fs.statSync(p).isFile();
  } catch {
    return false;
  }
}

/**
 * `String.split("\n")` with Rust's `str::lines()` semantics: a `\r` right
 * before the newline is stripped, and a single trailing newline is treated as
 * the final line's terminator rather than introducing an extra empty line.
 */
function splitLines(text: string): string[] {
  if (text === "") {
    return [];
  }
  const body = text.endsWith("\n") ? text.slice(0, -1) : text;
  return body.split("\n").map((line) => (line.endsWith("\r") ? line.slice(0, -1) : line));
}

function asError(cause: unknown): Error {
  return cause instanceof Error ? cause : new Error(String(cause));
}

function errnoCode(cause: unknown): string | undefined {
  return typeof cause === "object" && cause !== null && "code" in cause
    ? String((cause as { code: unknown }).code)
    : undefined;
}
