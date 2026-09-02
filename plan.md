# Rust CLI testing plan

Testing a Rust CLI application requires a combination of unit tests for
internal business logic and integration tests to verify end-to-end binary
execution, argument parsing, and output formatting.

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

1. Keep pure business-logic tests in `#[cfg(test)]` modules beside the Rust
   source they exercise.
2. Add `assert_cmd`, `predicates`, and `assert_fs` as development dependencies
   for integration tests in `tests/`.
3. Cover each command's happy path, argument errors, exit status, stdout, and
   stderr by executing the real `ketch` binary.
4. Use `assert_fs` to isolate installation roots and verify filesystem state
   after install, upgrade, relink, unlink, and uninstall operations.
5. Add `trycmd` fixtures for stable, lengthy, or documentation-worthy command
   output; update snapshots only when the behavior change is intentional.
6. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
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
