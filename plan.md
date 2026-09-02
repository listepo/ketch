# Rust CLI testing plan

Testing a Rust CLI application requires a combination of unit tests for
internal business logic and integration tests to verify end-to-end binary
execution, argument parsing, and output formatting.

## Implementation status

- [x] Added `assert_cmd`, `predicates`, `assert_fs`, `trycmd`, `rstest`,
  `insta`, and `pretty_assertions` as locked development dependencies, with
  portable binary assertions and a literate version snapshot.
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
- **rstest** provides parameterized tests and fixtures for compact, explicit
  test matrices such as archive formats, target tokens, and malformed input.
- **insta** stores reviewed snapshots for stable structured values and output;
  redact volatile paths, timestamps, and IDs instead of making assertions vague.
- **pretty_assertions** gives useful colored diffs for non-trivial equality
  checks; import its macros where the standard assertion would hide the cause.

## Execution plan

1. [x] Keep pure business-logic tests in `#[cfg(test)]` modules beside the Rust
   source they exercise.
2. [x] Add `assert_cmd`, `predicates`, `assert_fs`, `trycmd`, `rstest`, `insta`,
   and `pretty_assertions` as development dependencies for unit and integration
   tests.
3. [x] Cover each command's happy path, argument errors, exit status, stdout, and
   stderr by executing the real `ketch` binary.
4. [x] Use `assert_fs` to isolate installation roots and verify filesystem state
   after install, upgrade, relink, unlink, and uninstall operations.
5. [x] Add `trycmd` fixtures for stable, lengthy, or documentation-worthy command
   output; update snapshots only when the behavior change is intentional.
6. [x] Use `rstest` for true case matrices, `insta` for stable reviewed snapshots,
   and `pretty_assertions` for readable equality diffs.
7. [x] Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and
   `cargo test` before committing. Run the end-to-end suite separately with
   `cargo test --test install` when changing the install pipeline.

## Definition of done

- Unit tests cover changed business logic.
- Integration tests execute the compiled binary rather than internal helpers.
- Temporary files are isolated and cleaned up automatically.
- Output assertions identify the intended stream and exit code.
- Snapshot fixtures are reviewed as both tests and user-facing documentation.
- Parameterized cases are explicit and deterministic; snapshots redact only
  values that are inherently volatile.

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

### Dual-mode terminal UI contract

ketch supports two human-facing terminal experiences; the ratatui screen must
extend, not replace, the classic terminal UI.

- **Classic terminal UI (default):** line-oriented status, warnings, final
  summaries, and `indicatif` progress bars when stderr is interactive. It uses
  no raw mode or alternate screen, remains readable in terminal scrollback,
  and is the fallback for pipes, CI, `--quiet`, machine-readable output, and a
  terminal that cannot start the TUI.
- **Full-screen TUI (opt-in):** `--tui` with the `tui` Cargo feature renders
  the queue, activity, aggregate status, and keyboard help in the alternate
  screen. `q`/`Esc` return to the classic terminal UI while work continues;
  Ctrl-C restores the terminal before exiting with the standard interrupt
  status.
- **Parity requirement:** every install/upgrade/sync/registry-update state,
  warning, partial failure, and final result must remain understandable in
  classic mode. The TUI may aggregate or enhance that information, but cannot
  become its sole output path.
- **Testing requirement:** test both modes for each terminal-facing change:
  classic output streams and exit codes through spawned-binary assertions, and
  TUI reducer/rendering plus non-TTY fallback without escape sequences.

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

# Roadmap delivery plan

This is the execution plan for [`ROADMAP.md`](ROADMAP.md). Its ordering is a
delivery sequence rather than a calendar commitment: each milestone ends at a
usable, releasable boundary and can be paused without leaving a partial
platform or trust policy exposed to users.

## Shared delivery rules

- Preserve the existing `Platform` boundary: commands, sources, extraction,
  manifests, state, and the TUI remain OS-agnostic. Platform-specific behaviour
  belongs in `src/platform/`.
- Keep the default CLI line-oriented. New interactive behaviour must remain
  opt-in and cannot change output, exit status, logs, or JSON contracts for
  scripts and CI.
- Treat release metadata, archives, registry entries, signature sidecars, and
  shell paths as untrusted input. Reuse existing validation and ownership
  checks; do not weaken them to make a new platform or feature fit.
- Every schema or persisted-state change needs a backward-read test, generated
  documentation/schema updates where applicable, and an explicit migration or
  compatibility rule.
- Ship each milestone only after `cargo fmt --check`,
  `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`,
  the relevant feature/platform checks, and a packaged-binary smoke test.

## Milestone 0 — Establish cross-platform contracts

**Goal:** make the existing macOS assumptions explicit before adding a second
backend.

1. Inventory `src/platform/macos.rs`, `src/shell.rs`, extraction, and tests for
   Unix-only APIs, executable-bit assumptions, symlink semantics, and
   case-sensitive-path assumptions.
2. Move only genuinely shared asset-token scoring, destination ownership, and
   executable-discovery helpers into `src/platform/mod.rs`; leave macOS policy
   in the macOS backend.
3. Add table-driven unit tests for asset classification and placement
   preflight that can run on any host without invoking the active backend.
4. Add CI jobs that at least compile and unit-test the supported target matrix;
   run real install end-to-end tests on each platform as it becomes available.

**Exit criteria:** macOS behaviour and persisted link records are unchanged;
the `Platform` trait is sufficient for Linux without command-level `cfg`s.

## Milestone 1 — Linux support

**Goal:** support native Linux CLI releases without `.app` or macOS trust
behaviour leaking into the experience.

1. Add `src/platform/linux.rs` and select it from `platform::host()` under
   `target_os = "linux"`.
2. Implement `score_asset` for `linux`, `gnu`, `musl`, `x86_64`, and `aarch64`.
   Reject sidecars and foreign platform assets using the shared guards; define
   and test the ordering for native versus emulated architecture and static
   versus dynamically linked assets when libc is uncertain.
3. Implement `place`/`unplace` with the same ownership preflight and
   replacement guarantees as macOS: only create bin-directory symlinks, never
   overwrite a user-owned destination, and tolerate already-missing links.
4. Return `TrustVerdict::NotApplicable` until a signature verifier is added;
   do not strip or emulate macOS quarantine semantics.
5. Implement Linux `doctor` checks for writable store/bin directories and PATH
   reachability. Reuse shell setup only after its Linux file-selection logic is
   verified.
6. Add Linux integration fixtures for install, upgrade, relink, unlink,
   uninstall, checksum failure, and user-owned destination protection. Build a
   Linux release archive and smoke-test it in CI.

**Exit criteria:** `ketch install`, `upgrade`, `link`, `unlink`, `uninstall`,
and `doctor` work for Linux tar/zip release assets without macOS-specific
output or filesystem writes.

## Milestone 2 — Windows support

**Goal:** support native Windows release assets while retaining safe ownership
and uninstall behaviour.

1. Complete the Milestone 0 portability inventory before coding; replace or
   isolate Unix-only filesystem, permissions, shell, and process code so the
   Windows target compiles cleanly.
2. Add `src/platform/windows.rs` and a `target_os = "windows"` branch in
   `platform::host()`.
3. Implement Windows asset scoring for `.exe`, `.zip`, architecture tokens,
   and common target triples. Explicitly reject installers and package formats
   (`.msi`, `.nupkg`, etc.) that ketch does not install.
4. Make placement copy executables by default, record every copied destination
   in `LinkRecord`, and verify identity before replacement/removal. Cover
   case-insensitive name collisions and `.exe` name normalization.
5. Define Windows PATH integration separately from POSIX shell edits; support
   it only after an idempotent, reversible user-environment implementation and
   tests exist.
6. Add Windows CI, integration fixtures, release packaging, and a native
   packaged-binary smoke test.

**Exit criteria:** native Windows installation is safe, reversible, and does
not expose symlink-only or POSIX-shell assumptions to users.

## Milestone 3 — Provenance and signature verification

**Goal:** distinguish a matching checksum from an authenticated publisher.

1. Design a manifest `trust`/signature policy that declares verifier type,
   expected identity or pinned key, required sidecar pattern, and failure mode.
   Validate it in `Manifest::validate` and regenerate documentation/schema
   outputs before adding a verifier.
2. Extend installed state to record the verified method, identity, and
   sidecar/attestation digest. Preserve old state files by making new fields
   optional/defaulted and test both old and new records.
3. Add sigstore/cosign verification first: locate a matching `.sigstore` or
   `.intoto.jsonl` sidecar, bind it to the selected asset digest, verify the
   transparency-log/provenance data, and compare its identity to the manifest.
   Fail closed when a policy is declared but verification cannot complete.
4. Add minisign/signify next, requiring a manifest-pinned public key and a
   matching `.minisig` sidecar; do not use ambient key material.
5. Add GPG detached signatures only with a declared trust root or fingerprint.
   Never treat a locally available keyring as publisher authorization by
   default.
6. Surface verifier results in `info`, logs, and the TUI/line reporter without
   printing untrusted signature text. Add offline fixtures for valid, missing,
   mismatched, expired, and identity-confused signatures.

**Exit criteria:** a package can require publisher provenance; an authenticated
identity is persisted and auditable; checksum-only installs retain their
current explicitly weaker status.

## Milestone 4 — Man pages and shell completions

**Goal:** make existing manifest `extra_paths` useful without writing outside
approved locations unexpectedly.

1. Classify each validated `extra_paths` entry as a man page or completion by
   explicit manifest metadata or tightly documented path rules; reject
   ambiguous paths rather than guessing.
2. Resolve entries only beneath the extracted payload and record every exposed
   destination in state so uninstall/relink use the same ownership proof as
   binaries.
3. Add platform-specific destination resolvers: standard user man roots and
   shell completion directories, with configuration/doctor reporting before a
   write occurs.
4. Extend `path`/`doctor` guidance for `MANPATH` only when a package actually
   exposes man pages. Keep arbitrary startup-file edits opt-in and reversible.
5. Add integration tests for linking, relinking, upgrading, unlinking, and
   uninstalling man/completion files, including user-owned destination and
   traversal attempts.

**Exit criteria:** a manifest can expose documented auxiliary files, all links
are reversible, and uninstall never deletes a replacement it does not own.

## Milestone 5 — Registry maturity

**Goal:** move registry validation left while keeping update explicit.

1. Create a registry-side CI workflow or reusable validation command that
   parses every `ketch.toml`, runs manifest validation, checks alias collisions,
   and performs offline fixture installs for changed entries.
2. Promote name/alias collisions from local-update warnings to registry CI
   failures, while retaining warnings for already-published bad registries so
   clients remain best-effort.
3. Record successful registry update metadata (source revision/ETag and
   timestamp) under the ketch root, separately from package manifests.
4. Add `ketch registry status` (or equivalent `doctor` section) that reports
   age, source, and refresh advice without contacting the network. Keep
   `ketch update` the only refresh action.
5. Document registry author and maintainer workflows, including the exact
   validation command and compatibility policy.

**Exit criteria:** invalid/colliding registry entries are rejected before
merge, and users can identify stale local registry data without hidden network
traffic.

## Milestone 6 — Version rollback

**Goal:** safely switch a package back to a retained prior version.

**Prerequisite correction:** the roadmap says every version stays in the
store, but the current upgrade commit removes the replaced prefix. Establish a
retention model first; do not add a command that promises versions which have
already been deleted.

1. Define retained-version state: version, prefix, asset digest, links, and
   trust result, plus a retention policy and migration from current single
   installed-package records.
2. Change upgrade cleanup so an eligible older prefix is retained only after
   the new version is placed and state is atomically saved. Add pruning as an
   explicit command/policy, never an implicit data loss.
3. Add `ketch rollback <pkg> [--to <version>]`, defaulting to the immediately
   prior retained version. Preflight all destinations before retiring current
   links; preserve user replacements and state atomicity on failure.
4. Show available versions and retention details in `info`/`list` as needed.
5. Add end-to-end cases for successful rollback, missing retained version,
   occupied destination, pinned package interaction, and uninstall after a
   rollback.

**Exit criteria:** rollback never redownloads, never discards the currently
working version before the target is placeable, and makes its retained-history
policy visible.

## Milestone 7 — Resolution explanation (`ketch why`)

**Goal:** make selection decisions inspectable without changing them.

1. Extract a structured, side-effect-free resolution trace from manifest
   lookup, release selection, and asset scoring. It must carry rejected
   candidates and reasons without exposing secrets or untrusted control text.
2. Add `ketch why <pkg> [--json]`, displaying manifest tier/origin, source and
   version selection, scored/rejected assets, checksum/trust policy, and the
   final candidate.
3. Keep normal install resolution on the same functions so explanation cannot
   drift from behaviour; do not make an additional network call beyond what
   an equivalent dry resolution requires.
4. Add deterministic unit fixtures and binary-level JSON/text snapshots for
   aliases, user manifests, registry/built-in precedence, prereleases, pinned
   assets, and no-compatible-asset failures.

**Exit criteria:** users can explain a package decision end to end, and every
reported explanation is derived from the production resolver.

## Release sequencing

1. Land Milestone 0, then Linux as the first user-facing platform release.
2. Treat Windows, provenance, auxiliary paths, registry maturity, rollback,
   and `why` as independent feature releases after their own gates pass.
3. Do not add building from source, dependency resolution, root execution, or
   arbitrary install roots; these remain deliberately out of scope.
