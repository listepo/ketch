/**
 * Terminal output — the port of `src/ui.rs`, and the CLI's single choke point.
 *
 * Kept dependency-light on purpose: sources and platforms report progress
 * through core's `ProgressSink`, so nothing below this module needs to know
 * whether a human, a pipe, or a test is watching. This module is also the one
 * caller of core's `record`: every status line lands in the log with the same
 * words it showed on screen, and a line written any other way is invisible to
 * whoever reads the log afterwards.
 */

import process from "node:process";
import * as readline from "node:readline";
import { confirm as clackConfirm, isCancel } from "@clack/prompts";
import type { Config, InstallReporter, ProgressSink } from "@ketch/core";
import { KetchError, NullProgress, record } from "@ketch/core";
import pc from "picocolors";

// 0 quiet, 1 normal, 2 verbose — one scale, like the Rust `LEVEL` atomic.
let level = 1;
let colorOn = false;
// `createColors(false)` hands back identity functions, so before `init` runs
// (and in tests) every paint helper is a no-op rather than a surprise escape.
let colors = pc.createColors(false);

/** The bar group currently sharing the terminal, while a batch is running. */
let held: BarRenderer | null = null;

/**
 * Every status line leaves through here.
 *
 * While progress bars are on screen they own the bottom of the terminal, and
 * a bare write lands in the middle of one. The renderer knows how to print
 * above them: take the bars off the screen, write the line normally, redraw
 * them underneath it.
 */
function emit(line: string): void {
  const renderer = held;
  if (renderer !== null) {
    renderer.suspend(() => console.error(line));
  } else {
    console.error(line);
  }
}

/** Configure color and verbosity for this run. Called once from `main`. */
export function init(color: boolean | null, quiet: boolean, verbose: boolean): void {
  // NO_COLOR is a presence check, not a truthiness one: `NO_COLOR=` set to
  // the empty string still disables color, matching the Rust `var_os`.
  const enabled =
    color ??
    (Boolean(process.stderr.isTTY) &&
      process.env["NO_COLOR"] === undefined &&
      process.env["TERM"] !== "dumb");
  colorOn = enabled;
  colors = pc.createColors(enabled);
  level = quiet ? 0 : verbose ? 2 : 1;
}

/** Quiet swallows status lines; it never swallows warnings or data. */
export function setQuiet(quiet: boolean): void {
  level = quiet ? 0 : level === 0 ? 1 : level;
}

/** Verbose additionally shows the debug lines the log always gets. */
export function setVerbose(verbose: boolean): void {
  level = verbose ? 2 : level === 2 ? 1 : level;
}

export function colorEnabled(): boolean {
  return colorOn;
}

export function isQuiet(): boolean {
  return level === 0;
}

export function isVerbose(): boolean {
  return level >= 2;
}

export function bold(t: string): string {
  return colors.bold(t);
}
export function dim(t: string): string {
  return colors.dim(t);
}
export function green(t: string): string {
  return colors.green(t);
}
export function yellow(t: string): string {
  return colors.yellow(t);
}
export function blue(t: string): string {
  return colors.blue(t);
}
export function red(t: string): string {
  return colors.red(t);
}
export function cyan(t: string): string {
  return colors.cyan(t);
}

/** The right-aligned gutter every status prefix sits in: `{verb:>10}`. */
const GUTTER = 10;

/** Status line for a step that is happening now. */
export function step(verb: string, detail: string): void {
  // Logged before the level check: `--quiet` is about this terminal, and the
  // whole point of the log is to still have the run afterwards.
  record("info", `${verb} ${detail}`);
  if (isQuiet()) {
    return;
  }
  emit(`${blue(verb.padStart(GUTTER))} ${detail}`);
}

/** Something finished well. */
export function success(verb: string, detail: string): void {
  record("info", `${verb} ${detail}`);
  if (isQuiet()) {
    return;
  }
  emit(`${green(verb.padStart(GUTTER))} ${detail}`);
}

/** Something the user should know but that does not stop the run. Shown even
 * under `--quiet`: less output was asked for, not fewer warnings. */
export function warn(detail: string): void {
  record("warn", detail);
  emit(`${yellow("warning".padStart(GUTTER))} ${detail}`);
}

/** An aside: true, worth saying once, and not a problem. */
export function note(detail: string): void {
  record("info", detail);
  if (isQuiet()) {
    return;
  }
  emit(`${dim("note".padStart(GUTTER))} ${dim(detail)}`);
}

/** Only shown with `--verbose`, but always written to the log. */
export function debug(detail: string): void {
  record("debug", detail);
  if (isVerbose()) {
    emit(`${dim("debug".padStart(GUTTER))} ${dim(detail)}`);
  }
}

/** Fatal error rendering, including details and a hint when we have one. */
export function error(err: unknown): void {
  // The pipeline throws `KetchError`; anything else is a bug surfacing, and
  // still deserves a rendered line rather than a raw stack in the gutter.
  const e =
    err instanceof KetchError
      ? err
      : KetchError.msg(err instanceof Error ? err.message : String(err));
  const details = e.details();
  const hint = e.hint();
  // One record, so a failure is one entry in the log rather than three lines
  // a reader has to piece back together.
  let logged = e.message;
  for (const line of details) {
    logged += `\n${line}`;
  }
  if (hint !== null) {
    logged += `\nhint: ${hint}`;
  }
  record("error", logged);

  emit(`${red("error".padStart(GUTTER))} ${e.message}`);
  for (const line of details) {
    emit(`${" ".repeat(GUTTER)} ${dim(line)}`);
  }
  if (hint !== null) {
    emit(`${cyan("hint".padStart(GUTTER))} ${hint}`);
  }
}

let stdoutGuarded = false;

/**
 * Data output. Unlike the status helpers this goes to stdout, so `ketch list`
 * can be piped while progress still shows on the terminal.
 */
export function out(line: string): void {
  // The Rust side writes with `let _ =`; Node surfaces a closed pipe as an
  // async `error` event that would crash the process instead. Swallow it
  // once — `ketch list | head` ending early is not a failure.
  if (!stdoutGuarded) {
    stdoutGuarded = true;
    process.stdout.on("error", () => {});
  }
  console.log(line);
}

/**
 * Ask a yes/no question. Returns `default` when stdin is not a terminal, so
 * scripts never hang waiting for input that will not come.
 */
export async function confirm(question: string, defaultAnswer: boolean): Promise<boolean> {
  const answered = await ask(question, defaultAnswer);
  if (!answered) {
    // Callers return cleanly on a decline, which is indistinguishable from
    // success. Say why nothing happened — especially when the decline came
    // from a non-interactive stdin rather than from a person. Printed even
    // under `--quiet`: "it did nothing and said nothing" is not quiet, it
    // is a bug report waiting to happen.
    console.error(`${blue("cancelled".padStart(GUTTER))} ${question}`);
  }
  return answered;
}

/** The `--yes` bypass every destructive command shares: skip the question
 * entirely when the flag was given, ask otherwise. */
export async function confirmOrYes(
  yes: boolean,
  question: string,
  defaultAnswer: boolean,
): Promise<boolean> {
  return yes ? true : confirm(question, defaultAnswer);
}

/**
 * `--quiet` deliberately does not reach here. It asks for less output, not
 * for consent: silently taking the default answer to "remove this?" is not a
 * quieter version of asking, it is a different program.
 */
async function ask(question: string, defaultAnswer: boolean): Promise<boolean> {
  if (!process.stdin.isTTY) {
    return defaultAnswer;
  }
  // The interactive prompt is clack's, on stderr like every other status
  // line — but clack draws with cursor movement, which turns into garbage
  // when stderr is a pipe. A person at a terminal with stderr redirected
  // still gets the plain `[Y/n]` prompt the Rust binary shows.
  if (!process.stderr.isTTY) {
    return askPlain(question, defaultAnswer);
  }
  const answer = await clackConfirm({
    message: question,
    initialValue: defaultAnswer,
    output: process.stderr,
  });
  // Ctrl-C mid-question: the Rust binary dies to SIGINT, so nothing runs.
  // Clack catches it instead; declining is the closest thing to stopping.
  return isCancel(answer) ? false : answer;
}

/** The Rust prompt verbatim: print to stderr, read one line from stdin. */
async function askPlain(question: string, defaultAnswer: boolean): Promise<boolean> {
  const suffix = defaultAnswer ? "[Y/n]" : "[y/N]";
  process.stderr.write(`${yellow("confirm".padStart(GUTTER))} ${question} ${suffix} `);
  const rl = readline.createInterface({ input: process.stdin });
  const answer = await new Promise<string>((resolve) => {
    rl.once("line", resolve);
    // stdin closing without a line is the read error case: take the default.
    rl.once("close", () => resolve(""));
  });
  rl.close();
  switch (answer.trim().toLowerCase()) {
    case "y":
    case "yes":
      return true;
    case "n":
    case "no":
      return false;
    default:
      return defaultAnswer;
  }
}

/** Human-readable byte count. */
export function bytes(n: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"] as const;
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${units[unit]}`;
}

/**
 * How many packages to work on at once: the flag, else the configured
 * default, never zero. (`batch` itself never runs more jobs than it has
 * packages.)
 */
export function jobs(cfg: Config, flag: number | undefined): number {
  return Math.max(flag !== undefined && flag > 0 ? flag : cfg.jobs, 1);
}

/** The pipeline's reporter seam, wired to this terminal. */
export const reporter: InstallReporter = {
  step(action, detail) {
    step(action, detail);
  },
  warn(message) {
    warn(message);
  },
  debug(message) {
    debug(message);
  },
};

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

const LABEL_WIDTH = 28;
const BAR_WIDTH = 24;
const SPINNER = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"] as const;
const REDRAW_MS = 100;

/** What one bar knows about its download. Mutated by the sink, read by the
 * renderer on every frame. */
export interface BarState {
  label: string;
  total: number | null;
  current: number;
  startedAt: number;
}

/**
 * One rendered bar line, mirroring the indicatif templates:
 * `  {msg:<28} [{bar:24}] {bytes:>10}/{total_bytes} {bytes_per_sec:>11}` with
 * a known total, `  {msg:<28} {spinner} {bytes:>10}` without one.
 *
 * Exported for its tests; nothing outside this module should format a bar.
 */
export function barLine(state: BarState, frame: number, nowMs: number): string {
  const label = padChars(state.label, LABEL_WIDTH);
  const count = bytes(state.current).padStart(10);
  if (state.total === null) {
    const spinner = SPINNER[frame % SPINNER.length] ?? "";
    return `  ${label} ${spinner} ${count}`;
  }
  const ratio = state.total > 0 ? Math.min(state.current / state.total, 1) : 0;
  const filled = Math.min(Math.floor(ratio * BAR_WIDTH), BAR_WIDTH);
  // indicatif's `progress_chars("=> ")`: '=' behind, '>' at the tip, spaces
  // ahead, and the tip disappears when the bar is full.
  const fill =
    filled >= BAR_WIDTH
      ? "=".repeat(BAR_WIDTH)
      : `${"=".repeat(filled)}>${" ".repeat(BAR_WIDTH - filled - 1)}`;
  // ponytail: plain average rate since start; indicatif smooths over a
  // window. Upgrade to an exponential average if the number visibly jumps.
  const elapsed = Math.max((nowMs - state.startedAt) / 1000, 0.001);
  const rate = `${bytes(Math.floor(state.current / elapsed))}/s`.padStart(11);
  return `  ${label} [${fill}] ${count}/${bytes(state.total)} ${rate}`;
}

/**
 * The shared terminal under a set of live bars — the indicatif the port does
 * not have. Redraws in place with cursor-up + erase, on a timer that never
 * keeps the process alive.
 */
class BarRenderer {
  private readonly bars: BarState[] = [];
  /** Lines currently on screen, so a redraw knows how far up to go. */
  private drawn = 0;
  private frame = 0;
  private timer: ReturnType<typeof setInterval> | null = null;

  add(bar: BarState): void {
    this.bars.push(bar);
    if (this.timer === null) {
      this.timer = setInterval(() => {
        this.frame += 1;
        this.draw();
      }, REDRAW_MS);
      // A stuck download must not be what keeps the process running.
      this.timer.unref();
    }
    this.draw();
  }

  remove(bar: BarState): void {
    const at = this.bars.indexOf(bar);
    if (at >= 0) {
      this.bars.splice(at, 1);
    }
    this.draw();
    if (this.bars.length === 0) {
      this.stop();
    }
  }

  /** Take the bars off the screen, let `fn` print, redraw underneath it. */
  suspend(fn: () => void): void {
    this.erase();
    try {
      fn();
    } finally {
      this.draw();
    }
  }

  dispose(): void {
    this.stop();
    this.erase();
  }

  private draw(): void {
    const now = Date.now();
    // One write per frame: cursor to the top of our region, then each line
    // erased-and-rewritten, then anything left over from a taller frame
    // cleared. Interleaving writes is how bars end up inside one another.
    let buf = this.drawn > 0 ? `[${this.drawn}A\r` : "\r";
    for (const bar of this.bars) {
      buf += `[2K${barLine(bar, this.frame, now)}\n`;
    }
    buf += "[0J";
    process.stderr.write(buf);
    this.drawn = this.bars.length;
  }

  private erase(): void {
    if (this.drawn === 0) {
      return;
    }
    process.stderr.write(`[${this.drawn}A\r[0J`);
    this.drawn = 0;
  }

  private stop(): void {
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
}

/** A real terminal progress bar. */
export class BarProgress implements ProgressSink {
  private readonly state: BarState = { label: "", total: null, current: 0, startedAt: 0 };
  /** The group's renderer when this bar is one of several on screen. */
  private readonly group: BarRenderer | null;
  /** What to call the work, when the caller knows better than the download
   * does. Four bars all labelled by asset file name say very little. */
  private readonly label: string | null;
  /** The renderer a lone bar draws itself with. */
  private own: BarRenderer | null = null;

  constructor(group?: BarRenderer, label?: string) {
    this.group = group ?? null;
    this.label = label ?? null;
  }

  /** One bar in `group`, named for the work it is doing. */
  static inGroup(group: BarRenderer, label: string): BarProgress {
    const bar = new BarProgress(group, label);
    return bar;
  }

  start(total: number | null, label: string): void {
    this.state.total = total;
    this.state.current = 0;
    this.state.startedAt = Date.now();
    this.state.label = truncate(this.label ?? label, LABEL_WIDTH);
    if (this.group !== null) {
      this.group.add(this.state);
    } else {
      this.own = new BarRenderer();
      this.own.add(this.state);
    }
  }

  advance(delta: number): void {
    this.state.current += delta;
  }

  finish(message: string): void {
    if (this.group !== null) {
      this.group.remove(this.state);
    } else if (this.own !== null) {
      this.own.remove(this.state);
      this.own = null;
    }
    if (message === "") {
      return;
    }
    record("info", `fetched ${message}`);
    // In a batch the download is one step of several and `installed X`
    // follows it directly; a line per asset just pushes that off screen.
    if (!isQuiet() && this.group === null) {
      emit(`${green("fetched".padStart(GUTTER))} ${message}`);
    }
  }
}

/**
 * A terminal shared by several progress bars at once.
 *
 * Held for the length of a batch. While it lives every status line is printed
 * through the renderer rather than straight to stderr, so a bar being redrawn
 * never lands in the middle of a warning. Call `done()` when the batch ends —
 * the port of the Rust `Drop`.
 */
export class Bars {
  private renderer: BarRenderer | null;

  constructor(renderer: BarRenderer | null) {
    this.renderer = renderer;
  }

  /** One bar in this group, named for the work it is doing. */
  sink(label: string): ProgressSink {
    return this.renderer !== null ? BarProgress.inGroup(this.renderer, label) : new NullProgress();
  }

  /** Clear the bars and give the terminal back. Safe to call twice. */
  done(): void {
    // The global goes first, exactly as the Rust drop order: nothing that
    // emits afterwards may find a renderer that is being torn down.
    const group = held;
    held = null;
    const renderer = group ?? this.renderer;
    this.renderer = null;
    renderer?.dispose();
  }
}

/** Start a group of bars for concurrent work. */
export function bars(): Bars {
  if (isQuiet() || !process.stderr.isTTY) {
    return new Bars(null);
  }
  const renderer = new BarRenderer();
  held = renderer;
  return new Bars(renderer);
}

/** Pick the right sink for the current run. */
export function progress(): ProgressSink {
  if (isQuiet() || !process.stderr.isTTY) {
    return new NullProgress();
  }
  return new BarProgress();
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/** Shorten to `width`, ending with `…` when it does not fit. Counts code
 * points, as Rust counts `chars`: multi-byte input must never be split. */
export function truncate(text: string, width: number): string {
  const chars = Array.from(text);
  if (chars.length <= width) {
    return text;
  }
  return `${chars.slice(0, Math.max(width - 1, 0)).join("")}…`;
}

/** Pad with trailing spaces to `width` code points — `padEnd` counts UTF-16
 * units and drifts on anything outside the BMP. */
function padChars(text: string, width: number): string {
  const count = Array.from(text).length;
  return count >= width ? text : text + " ".repeat(width - count);
}

/** Render rows as an aligned table. Empty input produces no output. */
export function table(headers: readonly string[], rows: readonly (readonly string[])[]): void {
  if (rows.length === 0) {
    return;
  }
  const cols = headers.length;
  const widths = headers.map((h) => Array.from(h).length);
  for (const row of rows) {
    row.slice(0, cols).forEach((cell, i) => {
      widths[i] = Math.max(widths[i] ?? 0, Array.from(cell).length);
    });
  }
  const header = headers.map((h, i) => padChars(h, widths[i] ?? 0));
  out(bold(header.join("  ").trimEnd()));
  for (const row of rows) {
    const line = row.slice(0, cols).map((c, i) => padChars(c, widths[i] ?? 0));
    out(line.join("  ").trimEnd());
  }
}
