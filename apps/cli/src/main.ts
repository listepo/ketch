#!/usr/bin/env node
/**
 * ketch — catch releases straight from GitHub.
 *
 * `main` does one thing: run the parsed command and converge every failure
 * path here, so a single place decides how errors are shown and what the
 * process exits with.
 */

import process from "node:process";
import { KetchError, path as logPath } from "@ketch/core";
import { run } from "./cli.ts";
import * as ui from "./ui.ts";

try {
  await run();
} catch (err) {
  ui.error(err);
  // What npm and cargo do, and for the same reason: the terminal shows the
  // failure, the log shows the run that led to it.
  const file = logPath();
  if (file !== null) {
    ui.note(`the full log of this run is in ${file}`);
  }
  process.exit(err instanceof KetchError ? err.exitCode() : 1);
}
