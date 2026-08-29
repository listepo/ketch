/**
 * The progress-reporting seam between long work and whoever is watching.
 *
 * Downloads and other slow operations report through `ProgressSink`, so
 * nothing below the UI needs to know whether a human, a pipe, or a test is
 * watching. The terminal implementation lives with the UI; this module owns
 * only the shape and the sink that discards everything.
 */

/** Where a long-running operation reports its progress. */
export interface ProgressSink {
  start(total: number | null, label: string): void;
  advance(delta: number): void;
  finish(message: string): void;
}

/** Discards everything. Used by tests, `--quiet`, and non-terminal output. */
export class NullProgress implements ProgressSink {
  start(_total: number | null, _label: string): void {}
  advance(_delta: number): void {}
  finish(_message: string): void {}
}
