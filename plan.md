# Rust CLI testing plan

Testing a Rust CLI application requires a combination of unit tests for
internal business logic and integration tests to verify end-to-end binary
execution, argument parsing, and output formatting.

## Implementation status

- [x] Added `assert_cmd`, `predicates`, `assert_fs`, and `trycmd` as locked
  development dependencies, with portable binary assertions and a literate
  version snapshot.
- [x] Preserved colocated unit tests and the offline, macOS install pipeline
  suite; the new binary tests cover help, invalid arguments, output streams,
  and an isolated root.
- [x] Added a `Justfile` for the documented lightweight task-runner choice.
- [x] Implemented opt-in `ratatui`/`crossterm` TUI support behind `--features
  tui`, including non-TTY/CI fallback and reducer/rendering tests.

## Test tools

- **assert_cmd** executes your compiled CLI binary and runs assertions against
  exit codes, stdout, and stderr.
- **predicates** composes boolean assertions for output matching (for example,
  string containment and regular expressions).
- **assert_fs** automates the setup, tear-down, and verification of temporary
  files and directories.
- **trycmd** orchestrates snapshot testing using plain text or Markdown files
  to check CLI commands against expected output. If the CLI generates lengthy
  or complex text outputs, line-by-line assertions are inefficient; `trycmd`
  keeps those expectations readable while making the fixtures documentation.

## Execution plan

1. [x] Keep pure business-logic tests in `#[cfg(test)]` modules beside the Rust
   source they exercise.
2. [x] Add `assert_cmd`, `predicates`, and `assert_fs` as development dependencies
   for integration tests in `tests/`.
3. [x] Cover each command's happy path, argument errors, exit status, stdout, and
   stderr by executing the real `ketch` binary.
4. [x] Use `assert_fs` to isolate installation roots and verify filesystem state
   after install, upgrade, relink, unlink, and uninstall operations.
5. [x] Add `trycmd` fixtures for stable, lengthy, or documentation-worthy command
   output; update snapshots only when the behavior change is intentional.
6. [x] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
   `cargo test` before committing. Run the end-to-end suite separately with
   `cargo test --test install` when changing the install pipeline.

## Definition of done

- Unit tests cover changed business logic.
- Integration tests execute the compiled binary rather than internal helpers.
- Temporary files are isolated and cleaned up automatically.
- Output assertions identify the intended stream and exit code.
- Snapshot fixtures are reviewed as both tests and user-facing documentation.

## Cargo cache maintenance

Install the optional `cargo-cache` developer tool with:

```bash
cargo install cargo-cache --locked
```

Use the project aliases from `.cargo/config.toml` to inspect and clean local
Cargo state without affecting the application:

```bash
cargo cache-info
cargo cache-dry-run
cargo cache-autoclean
```

Always inspect the dry run before cleanup. Cache removal only trades disk space
for future downloads; it must not be used as a substitute for fixing build or
test failures.

## Task runner selection

Choose **Just** if you want a fast, lightweight, and simple command alias tool
that feels like `make` without the baggage, or if the repository manages
multiple languages alongside Rust. References: [Rust Project Primer](https://rustprojectprimer.com/tools/tasks.html),
[Just vs cargo-make](https://www.libhunt.com/compare/cargo-make-vs-just), and
[Just and Cargo build scripts](https://just.systems/man/en/whats-the-relationship-between-just-and-cargo-build-scripts.html).

Choose **cargo-make** if you need complex CI/CD build pipelines,
cross-platform conditional flows, automated crate installations, or built-in
scripting extensions such as duckscript tailored specifically for Rust.
References: [cargo-make announcement](https://users.rust-lang.org/t/announcing-cargo-make-task-runner-and-build-tool-for-rust/11629),
[Rust Project Primer](https://rustprojectprimer.com/tools/tasks.html), and
[cargo-make documentation](https://sagiegurari.github.io/cargo-make/).

For ketch, prefer **Just** for a future task runner because the repository
combines Rust with shell and site tooling and currently needs only simple
aliases. Revisit cargo-make if CI grows into conditional, multi-stage Rust
automation.

## Next feature: ratatui TUI support

Add an optional interactive terminal UI for long-running installs, upgrades,
syncs, and registry updates. Use [ratatui](https://ratatui.rs/) for rendering
and [crossterm](https://docs.rs/crossterm/latest/crossterm/) for terminal input,
raw mode, alternate-screen handling, and resize events. Keep the current
line-oriented `ui` output as the default so scripts, pipes, CI, `--quiet`, and
JSON output remain stable and do not require a terminal.

### Architecture

- Add a `tui` feature with `ratatui` and `crossterm` as optional dependencies;
  do not increase the default binary surface until the feature is enabled.
- Keep `src/install.rs`, sources, and platform code terminal-agnostic. Extend
  their existing progress/reporter seam with typed events rather than writing
  directly to a `ratatui::Frame`.
- Put the event model, reducer, and ratatui renderer in a dedicated
  `src/tui/` module. The reducer owns state; rendering is a pure projection of
  that state, which makes it testable with ratatui's `TestBackend`.
- Enter the TUI only when explicitly requested (for example, `--tui`) or when
  the eventual default policy confirms an interactive TTY. Fall back to the
  line UI when stderr is not a terminal, and never activate it for `--quiet`,
  machine-readable output, or CI.
- Guard terminal setup and teardown with an RAII type. Always restore raw mode,
  the cursor, and the alternate screen on success, error, panic, and Ctrl-C.

### Screen design

Use a compact three-region layout that remains useful at narrow widths:

```
┌ ketch · install · 2/4 packages ───────────────────────────────────────────┐
│ Queue (left)             │ Activity (main)                                  │
│ ✓ ripgrep 14.1            │ Downloading  ████████░░  80%  12.4 MiB / 15 MiB │
│ ⟳ fd 10.2                 │ Verifying checksum                             │
│ · bat pending             │ Extracting archive                             │
│ · jq pending              │                                                 │
├──────────────────────────┴─────────────────────────────────────────────────┤
│ 2 succeeded · 0 failed · 1m 04s                         q quit  ? help      │
└────────────────────────────────────────────────────────────────────────────┘
```

- **Header:** command, active package count, and overall progress.
- **Queue pane:** every requested package with pending, active, succeeded, or
  failed status; keep the selected package visible while the list scrolls.
- **Activity pane:** current stage, byte progress, checksum/trust warnings,
  and a bounded scrollback of recent events.
- **Footer:** success/failure totals, elapsed time, and key hints (`q` quit,
  `?` help, `Esc` return to the line UI if supported).
- Use color and symbols as enhancements only; status text must remain
  understandable with `NO_COLOR` and accessible terminal themes.

### Delivery steps

1. [x] Define typed progress events and a reducer independent of ratatui.
2. [x] Add the optional feature and a terminal-session guard that cleans up on all
   exit paths.
3. [x] Implement the queue/activity/footer renderer and keyboard handling with
   deterministic redraws and resize support.
4. [x] Wire `--tui` through the CLI while preserving existing output behavior for
   every non-TUI invocation.
5. [x] Unit-test reducer transitions and rendering with `TestBackend`; retain
   `assert_cmd`/`trycmd` coverage for the line UI and add a smoke test proving
   `--tui` refuses or falls back cleanly in a non-TTY subprocess.
6. [x] Guard terminal cleanup after success, a failed package, `q`/`Esc`, and
   panic with RAII plus a panic hook; run the full Rust gates and the packaged
   binary smoke test.

### Acceptance criteria

- TUI builds only with the opt-in feature and does not change default CLI
  output, exit codes, logs, or JSON contracts.
- All install stages and partial-batch failures are visible without flooding
  the terminal or losing the final result.
- Non-TTY and redirected invocations never emit escape sequences.
- Terminal state is restored on every exit path, including Ctrl-C and panic.
- Rendering and reducer tests run without a real terminal or network.
