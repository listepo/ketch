# ketch
https://github.com/pyrlyn/ketch
Catch releases straight from GitHub — a package manager for GitHub-released binaries and apps.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| B60 | in progress | P3 | 1 | 80% | Cursor / grok 4.7 |
| B65 | in progress | P0 | 2 | 0% | Cursor / grok 4.7 high |
| R3 | in progress | P1 | 3 | 67% | Cursor / grok 4.7 high |
| F8 | in progress | P2 | 3 | 0% | Cursor / grok 4.7 high |
| M16 | todo | P2 | 4 | 0% | |
| M17 | in progress | P2 | 4 | 90% | Cursor / grok 4.7 |
| R5 | in progress | P2 | 4 | 90% | Claude Code / opus-5.5 |
| R6 | in progress | P2 | 4 | 90% | Claude Code / opus-5.5 |
| R8 | in progress | P3 | 3 | 90% | Claude Code / sonnet-5.5 |
| F12 | in progress | P3 | 5 | 60% | Claude Code / opus-5.5 |
| F13 | in progress | P3 | 4 | 80% | Claude Code / opus-5.5 |
| F14 | in progress | P2 | 3 | 90% | Claude Code / opus-5.5 |
| F18 | in progress | P2 | 4 | 90% | Claude Code / opus-5.5 |
| D4 | todo | P3 | 2 | 0% | |
| D5 | todo | P3 | 3 | 0% | |
| D7 | todo | P3 | 2 | 0% | |
| D8 | todo | P3 | 2 | 0% | |
| D9 | todo | P3 | 2 | 0% | |
| D10 | todo | P2 | 3 | 0% | |
| D11 | todo | P3 | 5 | 0% | |
| D12 | todo | P3 | 4 | 0% | |
| D13 | todo | P3 | 3 | 0% | |
| D14 | todo | P3 | 4 | 0% | |
| D16 | todo | P3 | 5 | 0% | |
| D17 | todo | P3 | 4 | 0% | |
| D18 | todo | P3 | 3 | 0% | |
| D19 | todo | P3 | 4 | 0% | |

### Ketch audit

Actionable follow-ups from the 2026-09-20 product audit:

1. `ROADMAP.md` is wrong: signatures/trust are still marked "wanted", but `src/trust.rs` + M3 already shipped. Rewrite ROADMAP to match reality; remove shipped items from "wanted". — done: signatures → shipped M3, man pages → shipped M4, `why` → shipped M7, registry maturity → partial with collisions/staleness resolved and registry CI dropped (F2).
2. `todo.md` still lists M3/F5 as open, though they are done. Sync with `plan.md`. — done: todo.md now lists F1/F2/M3/F5 with F1 done (ketch side), F2 dropped, M3 done, F5 done (ketch side).
3. Version drift — done: `Cargo.toml`, `CHANGELOG.md` (`0.4.6`, 2026-09-20), tag `v0.4.6`, and `site/hugo.toml` (`params.version`) are all aligned. `site/sync-docs.py::sync_version()` keeps `site.Params.version` equal to `Cargo.toml` on every site build (covered by `tests/site_version.rs` and `site/test_sync_docs.py`). CI guard added: `tests/crate-version.sh` fails when the crate version, latest tag, and changelog entry disagree; wired into `just lint-shell` and the CI package job's shell-syntax step.
4. Registry has no CI — resolved as dropped (F2): `pyrlyn/ketch-registry` removed its only workflow (commit `5a9bbd6`, "no CI is wanted in this repo"), so there is no upstream to land a validating workflow in. Ketch side documents local validation plus a pre-push hook (`docs/REGISTRY.md`); name/alias collisions are fatal in `ketch registry validate` and warnings on `ketch update` by design (best-effort client).
5. "Is the registry stale?" — resolved: `ketch registry status` and the `ketch doctor` registry line already report the local copy's age and source from `registry.meta.toml` with no network call. `ketch update` remains the only refresh, by design.
6. macOS notarisation: `KETCH_NOTARIZE` exists, but secrets / a real notarized release are not set up yet (F1). Configure when ready — creator step, needs the App Store Connect key; not doable from inside the repo.
7. Docs: no Troubleshooting page (Windows locked exe, brew → self upgrade, registry collisions, notarize failures). — done: `docs/TROUBLESHOOTING.md` (wired into `site/sync-docs.py` + the pages.yml build checklist + `.gitignore`).

### Ketch audit (part 2)

8. No reference plugin in-repo as a copy-paste example. — done: `examples/ketch-source-example` (executable, `sh -n` clean, mirrors `docs/PLUGINS.md`; docs link to it).
9. Tests: trycmd is nearly empty (one `version` snapshot). — done: `tests/cases/help.trycmd` (`--help`) + `tests/cases/help-commands.trycmd` (install/upgrade/doctor/registry `--help`); `cargo test --test trycmd` green.
10. Plugin protocol: little e2e coverage with a fake `ketch-source-*` (fail paths). — done: `tests/plugin_fail.rs` (capabilities failure named by `plugin list`, future-protocol scheme refused, `releases` failure carries stderr); green.
11. Concurrent upgrade stress: few tests for "two `ketch upgrade` at once" / "binary busy mid-upgrade". — done (lock half): `tests/lock_extras.rs::a_second_upgrade_while_the_lock_is_held_reports_the_holder` proves the second run fails with exit 8 and the holder message. "Binary busy mid-upgrade" (in-use process stop offer, Windows rename-aside) is already covered in `src/process.rs` + `tests/self_update.rs` / `tests/auto_update.rs`; no new test added.
12. `extra_paths` e2e: "install → man/completion on disk → uninstall removes them". — done: `tests/lock_extras.rs::extras_are_linked_on_install_and_removed_on_uninstall` (sandbox `XDG_DATA_HOME` via new `Sandbox::ok_env`); green.
13. Evaluate multi-version side-by-side and aqua parity (global lockfile UX, built-in catalog) — done as A3 in `done.md`: all three already shipped (retention + rollback/prune, `lock`/`sync`, `builtin.toml` tiers), no gap, no issues filed.
14. Suggested order: (1) sync ROADMAP/todo + version — done above, (2) ~~CI validate on the registry~~ dropped (F2; local validate + pre-push hook instead), (3) trycmd + fake plugin + uninstall e2e + concurrent stress — done above, (4) notarize secrets — creator step, (5) multi-version issues — done as A3, nothing to file.


---

## Note 2026-09-17 — testing library candidates

Shared catalog: [`listepo/rust.md`](../../rust.md) → Code rules *Testing candidates*
and cargo *Testing candidates (evaluate — not auto-added)*. Do **not** add these
deps unless a concrete gap shows up.

Fits for ketch (1–3):

1. `vfs` — evaluate for install/store unit tests that today use `tempfile` /
   `assert_fs` host trees (path quirks, no disk).
2. `temp-env` — scoped `KETCH_*` / `PATH` overrides in unit tests.
3. `fake` — only if synthetic GitHub release/asset fixtures beat hand-written JSON.

Already covered: `assert_cmd`, `assert_fs`, `insta`, `predicates`,
`pretty_assertions`, `proptest`, `rstest`, `trycmd`. Skip `mockall` /
`tokio-test` / `testcontainers` / extra fuzzers unless a new seam needs them.

### B60. Windows self-update leaves `ketch.exe.old` behind

Observed on Windows: `ketch self update` 0.4.4 → 0.5.1 succeeded, but the old binary stayed in the bin dir as `ketch.exe.old`. Windows will not delete the file backing a running image, and at the success cleanup in `replace_binary` (`let _ = std::fs::remove_file(&backup)` after the smoke test) the process doing the deleting is that renamed old image — so the removal fails with access denied and the error is dropped. `install_self`'s aside cleanup has the same silent drop, and a stale `.old` left behind also becomes a destination that can fail the next swap's rename.

Plan: sweep the aside at the start of the self commands instead of at the end of the swap. `self update` and `self install` delete `<bin>/ketch.exe.old` before touching anything: by then any earlier updater process has exited, so the deletion works. Failure stays non-fatal. `doctor` gains a stale-`.old` note for the case where even that fails.

Execution: `aside_candidates` names both leftovers (`ketch.exe.old` from `replace_binary`, `ketch.old` from `install_self` on Windows). `sweep_stale_asides` runs at the start of `update` (not on `--dry-run`) and `install_self`, and `replace_binary` sweeps its own destination again before the rename. A failure warns and continues. The end-of-swap delete stays a single best-effort try: on Windows this process is that image, so the delete cannot succeed until exit. `stale_aside_check` warns from `doctor` when a leftover is still in the bin dir. Tests cover the two names, a sweep that deletes and one that ignores a missing file, and the doctor note.

## Plan 2026-09-27 (drafting with Ivan)

### Goals

### Tasks

### F8. Spinner and progress bar

Show a spinner while a command is running so the user sees that it started. Use a progress bar where measurable progress is available, and a spinner elsewhere. Match the behavior in rtok.

### R3. Cross-platform CI

Run verification on macOS, Windows and Linux. A ketch config is either a local file in the project or pushed to a registry; keep that model. The cross-platform check must catch OS-specific binary selection bugs like the one in B64.

Current state: `ci.yml` has separate jobs on macOS, Linux and Windows running lint, the full nextest suite and the `tui`-feature tests on all three. Formatting and commit-message checks run on macOS only, PowerShell syntax on Windows only. The packaging matrix runs on all three OSes. `Swatinem/rust-cache` is already in every job. `verify.yml` mirrors the same three-OS checks before a release.

To add:

1. Binary selection regression test: see B65 below.
2. Both config paths, local file and registry: cover the select-mode prompt when the binary name is missing and several candidates match, and assert the chosen binary is written back into the config.
3. Caching: rust-cache is already in place. Also evaluate caching the mise toolchain and the target directories on all three OSes. Do this in any case, and base the decision on the before/after build-time numbers from the cox and ketch infra-template PRs.

Findings: no workflow edit. `ci.yml` jobs `check` (`macos-latest`), `check-linux` (`ubuntu-latest`), `check-windows` (`windows-latest`) and `package` (all three) already run lint and the full nextest suite, so a binary-selection test is picked up with no extra job. `verify.yml` job `verify` uses the same three runners. `Swatinem/rust-cache@v2` is already in each of those jobs.

Do not add a second cache. `jdx/mise-action` already caches the toolchain, and `rust-cache` already caches `target/`. Numbers from [ketch#151](https://github.com/listepo/ketch/pull/151) run [36318217579](https://github.com/listepo/ketch/actions/runs/36318217579), the current-workflow run [36318223420](https://github.com/listepo/ketch/actions/runs/36318223420), the following warm main run [36320755597](https://github.com/listepo/ketch/actions/runs/36320755597), and [cox#57](https://github.com/listepo/cox/pull/57) run [36318140763](https://github.com/listepo/cox/actions/runs/36318140763):

- Mise cache hits on the ketch infra-template jobs: 35 MB macOS, 46 MB Linux, 62 MB Windows. Cox shows the same macOS and Linux hits and has no Windows job.
- macOS `check`: 8m9s on a target-cache miss, then 1m47s after restoring 778 MB.
- Linux `check-linux`: 894 MB restored in 12s, job 1m12s.
- Windows `check-windows`: 815 MB restored, and the restore itself was 2m52s of a 5m56s job. Another target cache would pay that download again.

Item 2 stays with B64. Item 1 is B65, which closes with B64's `tests/bin_choice.rs`.

### B65. Binary selection regression test

Add a fixture with two similarly named binaries (for example `rtok` and `rtok-hook`) and assert the intended one is chosen on every OS: macOS, Windows and Linux. This is the test that would have caught the Windows alphabetical-sort bug, where `rtok hook` was selected instead of the intended binary.

Note for the owner: B64's branch `b64-bin-name` already adds `tests/bin_choice.rs` with `rtok`, `rtok-hook` and `other-tool` fixtures, not gated by OS. Reuse or extend it once B64 merges instead of writing a second fixture.

### Priorities

Set in the task table above (creator, 2026-09-27).

## Plan 2026-09-30 (dictated by Ivan)

Eleven tasks Ivan dictated on 2026-09-30. Docs only: nothing below is implemented yet. Ivan's numbering maps to IDs as follows: 1 → B66, 2 → B67, 3 → B68, 4 → F9, 5 → B69, 6 → M10, 7 → M11, 8 → M12, 9 → F10, 10 → F11, 11 → R4. Priorities in the task table are suggestions; the creator confirms them.

Code read for this plan (`main` at `3920dc6`, v0.8.0): `src/self_update.rs` (`uninstall_plan`, `uninstall_self`, `remove_root_at`), `src/install.rs` (`install`, `uninstall`, `remove_store_dir`), `src/platform/unix.rs` and `macos.rs` (`move_into_store`), `src/shell.rs` (`install_user`, `uninstall_user`, `user_path_configured`), `src/cmd/pkg.rs` (`install`, `uninstall`), `src/error.rs`, `src/ui.rs`, `src/extra.rs` (`write_ketch_docs`, `render_manpage`), `src/cli.rs`, `Cargo.toml`, `install.ps1`, `dist-workspace.toml`.

### Suggested order

1. **Lifecycle (one code path).** B67 → B69 → B68 → B66 → F9. B67, B68 and B66 all touch the uninstall/update path (`install::uninstall`, `remove_store_dir`, `move_into_store`, `self_update::uninstall_self`), so they should land in that order, as separate PRs, on one owner. B69 depends on B67: "not found" is only honest once "uninstalled" means no state record, no `store/<name>/`, no links. F9 comes after B68 because its "yes" path runs the update.
2. **Output.** F10 → F11. Both go through the same line helpers in `src/ui.rs` (`step_line`, `success_line`, `warn_line`, `note_line`, `error_lines`), and F11's icon sits in the column F10 colours. The messages added by B69 and F9 go through those helpers too, so they pick up colour and icons without extra work.
3. **Generated from the CLI definition.** M10, M11, M12 all read `Cli::command()` (clap derive), so a CLI change regenerates them. They can land in parallel with 1 and 2, but land after F9/B69 if those add flags so the snapshots are blessed once. M12 depends on M11 (same completer for dynamic values) and on B66 (M12 adds a registry value, `Command Processor\AutoRun`, which self uninstall must remove).
4. **R4 (fuzz)** is independent and can start at any time. Its first step (a library target) touches `src/main.rs` and `Cargo.toml`, so coordinate with whoever is mid-change there.

Dependency summary: B69 ← B67; B68 shares the stale-sibling sweep with B67; F9 ← B68; B66 ← M12 (re-check after M12 adds AutoRun); F11 ← F10; M12 ← M11; R4 independent.

### Dependencies and tooling

- **clap** 4 (derive) is already the CLI. **clap_complete** 4 is already a dependency: `ketch completions <shell> [--install]` and `self install` (`extra::write_ketch_docs`) generate bash, zsh, fish, elvish and PowerShell scripts. M11 and the PowerShell half of M12 extend that; dynamic values need clap_complete's `unstable-dynamic` feature (`CompleteEnv`) or a small hidden `ketch __complete` command. M11 (done) chose `ketch __complete [--root DIR] <installed|registry> [PREFIX]` in `src/complete.rs`: `unstable-dynamic` is outside clap_complete's semver promise. M12 calls the same command.
- **clap_mangen** (new dependency, same clap 4 major; rtok uses 0.3 for `rtok man`) for M10. Today's `extra::render_manpage` hand-writes one `ketch.1`. `mandoc -Tlint` (ships with macOS, `apt install mandoc` on Linux) checks the roff in CI.
- **bash** ≥ 4 with bash-completion 2 for lazy-loaded completions. macOS ships bash 3.2, so document `brew install bash bash-completion@2`.
- **PowerShell**: `Register-ArgumentCompleter -Native` is what clap_complete already emits. `pwsh` 7 exists on the `windows-latest` runner; Windows PowerShell 5.1 uses a different profile directory. **doskey** is built into cmd.exe. cmd has no programmable completion, so doskey gives macros only (see M12).
- **Colour**: no colour crate; `src/ui.rs` writes ANSI itself and honours `--no-color`, `NO_COLOR` and `CLICOLOR_FORCE`. **Emoji width**: `unicode-width` is already a transitive dependency (through indicatif). F11 needs it as a direct dependency to pad columns correctly.
- **Fuzzing**: `cargo-fuzz` (`cargo install cargo-fuzz`, or `"cargo:cargo-fuzz"` in `mise.toml`) plus a **nightly** toolchain (`rustup toolchain install nightly`). `mise.toml` pins stable 1.98.1 only, and that stays the build toolchain. libFuzzer runs on macOS and Linux; Windows is not a target for R4. Use the same layout as rtok's in-progress `test/cargo-fuzz` branch (a standalone `fuzz/` workspace excluded from the root one).




Plan:


## Plan 2026-09-30 — desktop app on `ketch-core`

Seven tasks from `docs/research-desktop.md`. Creator's decision (2026-09-30): a native UI on each platform, macOS first; Windows and Linux follow later (`roadmap.md`). So the core is exported through UniFFI to a SwiftUI app. R5–R8 make the core usable outside a terminal and help the CLI and the TUI on their own. Priorities are suggestions; the creator confirms them.

Order: R5 → R6 and R7 (in either order; R7 after B64) → R8 → R9 → F12 → F13.

### R5. Workspace split: `ketch-core` library crate

Turn the repository into a Cargo workspace. The modules move into `crates/ketch-core` (with a `src/lib.rs`); the `ketch` binary keeps `main.rs`, `cli.rs`, `cmd/`, `complete.rs` and the terminal renderer. The public surface is what `cmd/` calls today. No behaviour change: every existing test passes unchanged, `unsafe_code = "forbid"` and MSRV 1.86 stay on both crates, `release.yml` still matches `dist generate`, and the release asset names do not change.

Overlaps with R4 step 1 (a `src/lib.rs` for the fuzz targets): whichever lands first does it, and the other builds on it.

Plan:
1. Timing: this moves most of `src/`, so start it when the in-progress tasks touching `src/` (B64, B65, M9, F8, R3) have merged, and do the move as pure `git mv` commits so open branches rebase across the renames.
2. Root `Cargo.toml` becomes the `ketch` package plus `[workspace] members = [".", "crates/ketch-core"]` with `[workspace.package]` (edition, `rust-version`, licence, repository) and `[workspace.lints]` (`unsafe_code = "forbid"`) inherited by both. The root stays the `ketch` package so `scripts/release.sh`, `tests/crate-version.sh`, `release-plz.toml` and `dist-workspace.toml` (`members = ["cargo:."]`) keep reading the version where they do now; `ketch-core` gets `version.workspace = true` and `publish = false`, and dist is told to skip it (`dist = false` in its package metadata).
3. `crates/ketch-core/src/lib.rs` declares every module except `cli`, `cmd`, `complete` and the terminal half of `ui`; `builtin.toml`, `sigstore-trusted-root.json`, `migrations/` and `src/snapshots` move with the modules that embed them (fix `include_str!`/`embed_migrations!` paths).
4. Visibility: start with `pub mod` for what `cmd/` uses, `pub(crate)` for the rest; no re-architecture in this task. `ui.rs` stays in the core for now (R6 moves the printing out) so this task is a move only.
5. `main.rs`, `cli.rs`, `cmd/`, `complete.rs` stay in the root package and `use ketch_core::…`. Dependencies split: clap, clap_complete, crossterm/ratatui (`tui` feature) go with the binary; the rest with the core. The `tui` feature is forwarded if the core still needs it.
6. Tests: `tests/` stays with the binary (it drives the real binary). Unit tests move with their modules. `trycmd`/`insta` snapshot paths are checked, not re-recorded.
7. Tooling: `Justfile` (`--workspace` where needed), `ci.yml`, `.cargo/config.toml` `paths` override from `just setup`, `sync-docs.py` if it lists `src/` paths; AGENTS.md "Layout" table updated; `rust.md` and `toolchain.md` unchanged (no new crates).

Check: `just check` clean; `cargo nextest run --workspace` passes with no snapshot changes; `dist build` for the host produces `ketch-<target>.tar.gz` with the same layout; `just dist-generate` leaves `release.yml` unchanged; `scripts/release.sh --dry-run` prints the right next version.

Status: PR https://github.com/pyrlyn/ketch/pull/187 merged 2026-10-01. The version moved to `[workspace.package]`; `release.sh` and `crate-version.sh` read it there, and v0.9.0 was released after the merge. Remaining: the doc examples that became doctests are marked `ignore` and should be rewritten (18 `ignore` fences under `crates/` at the last check).

### R6. A reporter instead of the global `ui::` sink

The pipeline prints through `ui::` directly (`install.rs`, `self_update.rs`, `registry.rs`, `listing.rs`), and `ui.rs` keeps global state for the TUI. Replace that with a `Reporter` passed into the core (or typed events on a channel, generalising `tui::Event`): progress, status, warning, log. The CLI implements it with today's `ui.rs`, the TUI with its events. `ui.rs` stays the only place that prints, and `log::record` is still reached through it.

Done when the core has no `ui::` calls, CLI output is byte-for-byte the same (the existing `trycmd`/`insta` snapshots pass unchanged), and a test reporter can assert the events of an install.

Plan:
1. Inventory (as of f60d85e): `ui::` calls outside `cmd/`, `ui.rs` and `main.rs` — `install.rs` 35, `self_update.rs` 27, `registry.rs` 8, `listing.rs` 7, `http.rs` 5, `hooks.rs` 4, `process.rs` 4, `trust.rs` 4, `log.rs` 3, `source/mod.rs` 3, and 1–2 each in `changelog`, `config`, `manifest`, `resolve`, `state`, `stats`, `extract`, `platform/unix`, `platform/windows`, `source/{github,local,plugin}`. Globals in `ui.rs`: `COLOR`, `LEVEL`, `BARS` (indicatif `MultiProgress`), `TUI`, `TUI_INPUT_PAUSE`; `log.rs` has `SINK`.
2. Define in the core `pub enum Event` (typed, not rendered strings): `Step { package, stage }` (resolve, download, verify, extract, link, hooks), `Progress { id, done, total }`, `Status`, `Warn`, `Note` (verbose), each carrying structured fields; and `pub trait Reporter: Send + Sync { fn event(&self, e: Event); }`. Core entry points take `&dyn Reporter` (via a small `Ctx { cfg, reporter, … }` so signatures do not grow per task).
3. Convert module by module, smallest first, `install.rs` and `self_update.rs` last; each step is its own commit with snapshots unchanged.
4. The binary's `ui.rs` implements `Reporter` by rendering exactly today's lines and bars; the TUI controller implements it by mapping `Event` to its own events (drop the string-stripping path in `ui::line`). `log::record` stays called from `ui.rs` only; a non-CLI host gets a `LogReporter` adapter in the core that records events to the log file, so the GUI's operations are logged too.
5. Colour and verbosity become renderer settings, not core globals; `log::SINK` is initialised by the host.
6. AGENTS.md "Conventions": "All output goes through `ui::`" becomes "The core reports through `Reporter`; only `ui.rs` prints".

Check: `grep 'ui::' crates/ketch-core/src` is empty; snapshots unchanged; a new unit test installs a fixture package with a recording reporter and asserts the event sequence; `--tui` still works (manual run against a `KETCH_ROOT` scratch tree).

Status: PR https://github.com/pyrlyn/ketch/pull/189 merged 2026-10-01. `ui.rs` and `tui/` left the core; the core reports through `Ctx { cfg, report }`, with `LogReporter` and a test `Recorder`. Remaining: colour and verbosity are still global inside `ui.rs` (plan step 5); the TUI still receives events through `ui::Terminal`; the `--tui` check ran on a zero-size pty, so it is weak and needs a manual run on a real terminal.

### R8. Core calls from a long-running host

A GUI keeps running between operations and must not freeze. Core operations must be callable from a worker thread, and the process lock must be tryable: `state::Lock` gets a non-blocking acquire, so a frontend can report "another ketch is running" instead of waiting. Check that nothing in the core relies on process-global state that a second operation in the same process would see stale (config, the `ui` init, cached listings).

Done when two operations in one process run one after the other in a test, and a held lock gives a typed "busy" error.

Finding from the survey: `Lock::acquire_path` already fails fast with the holder's pid, but it treats a lock file holding *its own pid* as re-entrant (`owned: false`). In a GUI process two concurrent operations would both pass. That is the main fix here.

Plan:
1. Lock: keep the lock file for cross-process exclusion, and add an in-process guard (a `static` `Mutex<bool>`/`AtomicBool` "held by this process") so a second acquire in the same process fails with the same busy error instead of adopting the lock. Keep the existing re-entrancy only where the CLI relies on it (find the callers first; if none, remove it and say why in the commit).
2. Typed error: `Error::Busy { pid: Option<u32> }` (today it is a message), so a frontend can show "ketch is busy (pid N)" and retry, and the CLI prints the same text as today.
3. Cancellation: a `Cancel` token (`Arc<AtomicBool>`) in the context, checked between packages and between download chunks; a cancelled operation cleans up its temp dir and returns `Error::Cancelled`. The CLI wires it to Ctrl-C where it already handles SIGINT (the TUI's exit 130 path).
4. Process globals: `Config::load` reads env and files on every call — the host rebuilds `Config` per operation, so config edits in `config.toml` are seen; `log::SINK` initialised once per process; the `listing.rs` cache is file-based, fine. `tokio` runtime for `push.rs` is created per call, confirm it is not nested inside a host runtime.
5. `Send`: core entry points are callable from any thread (no `Rc`, no thread-locals in the pipeline).

Check: unit tests — two locks in one process → second is `Busy`; lock released on drop and on error; a cancelled fixture install leaves no partial store folder and no state entry; an end-to-end test runs the CLI while a lock is held and asserts the busy message.

Status: PR https://github.com/pyrlyn/ketch/pull/184 merged 2026-10-01. Pid re-entrancy had no callers and was removed. Remaining: SIGINT wiring for the CLI (no signal handler exists; tokens are never cancelled there), and `self upgrade` / registry downloads still pass a never-cancelled token.

### F12. Native macOS app (SwiftUI) on `ketch-ffi`

A SwiftUI app in the repository (e.g. `desktop/macos/`), consuming R9's XCFramework and Swift package: installed and registry packages (list, search), install, upgrade, uninstall, changelog, doctor, with progress from the reporter callback and dialogs for the decider. It manages the same ketch root as the CLI and respects the same lock (R8).

Decided (creator, 2026-09-30): the app shares the ketch root (`~/.ketch`, or `KETCH_ROOT`) with the CLI — one state, one lock, one store. Also decided: a menu-bar extra is in scope (status and pending upgrades at a glance, quick actions), and the app follows ketch's triple licence (GPL-3.0-only, royalty-free, commercial). Minimum macOS version: 26 (the deployment target of the app and of R9's XCFramework).

Design (creator, 2026-10-01): the UI is frosted (matte) or glossy glass with a 3D effect. On macOS 26 that means the native Liquid Glass material (SwiftUI `.glassEffect`, `GlassEffectContainer`), with depth from layering, shadows and specular highlights — system materials, not a hand-drawn imitation, so it follows accessibility settings (Reduce Transparency, Increase Contrast).

Plan:
1. Project: `desktop/macos/` with the app target described in a text spec (XcodeGen or Tuist — pick at start with a sourced comparison, add to `toolchain.md`) so the project is reviewable in diffs; bundle id under the creator's team, deployment target macOS 26, Swift 6 strict concurrency, depends on the local `KetchCore` package from R9. `AGENTS.md` layout table gets the new paths.
2. Architecture: one `@Observable` `KetchStore` on the main actor owning the state; every core call runs in `Task.detached` and reports back through the `Reporter` callback hopped to the main actor. A `KetchCoreProtocol` wraps the FFI object so view models are testable with a fake.
3. Main window (`NavigationSplitView`): Installed (name, version, source, update badge; upgrade, uninstall, reveal in Finder), Discover (registry search, install), package detail (description, versions, changelog rendered from sanitized Markdown via `AttributedString(markdown:)`, release link), Activity (current operation with per-package progress and a log, Cancel button), Doctor (findings with fix actions the core offers).
4. Decisions: the binary choice becomes a sheet listing candidates; confirmations for uninstall/upgrade-all are app dialogs (the CLI's `cmd/` confirmations, re-done in the frontend per R7).
5. Menu bar: `MenuBarExtra` with the number of pending upgrades, "Upgrade all", the running operation's progress, "Open ketch", "Quit"; an update check on launch and on a timer while running (interval in Settings). Optional "Open at login" via `SMAppService.mainApp`.
6. Busy and shared root: a `Busy` error shows "ketch is running in another process (pid N)" with Retry; the app re-reads state when the window becomes key, so installs made from the CLI show up. The root shown in Settings (`KETCH_ROOT` honoured when set in the app's environment).
7. Settings: update-check interval, include prereleases, GitHub token presence (read-only note pointing to the CLI's config — the app does not store tokens), open `config.toml`.
8. About screen names the licence (GPL-3.0-only / royalty-free / commercial) and links the repository.
9. Tests: Swift Testing for `KetchStore` with the fake core (install flow, busy, cancel, decider sheet); one UI smoke test that launches the app against a scratch root.

Check: the app builds and runs on macOS 26 on both architectures; manual pass against a scratch `KETCH_ROOT`: install a fixture package, see it in the CLI's `ketch list`, uninstall from the CLI and see it disappear in the app; a held CLI lock shows the busy state; `swift test` and the UI smoke test pass in CI.

Status: part 1 (app shell on a fake core) merged as PR https://github.com/pyrlyn/ketch/pull/185 (2026-10-01). XcodeGen 2.46.0 chosen over Tuist and plain SwiftPM (sources in `docs/research-desktop.md`). `Theme.swift` now reads F14's generated tokens and the screens were rebuilt on the Liquid glass design (F16 and F17, `done.md`). R9 (PR https://github.com/pyrlyn/ketch/pull/208) is merged, so the core is available. Remaining: a `LiveKetchCore` adapter over the generated `KetchCore` (`Ketch/Core/CoreFactory.swift` still returns `FakeKetchCore`; the adapter maps R9's structured `Changelog`), and the manual checks against the real core.

### F13. macOS app release pipeline

A separate workflow for the app: `xcodebuild` archive, Developer ID signing with the existing certificate, notarisation and stapling (a `.app`/`.dmg` can be stapled, unlike the bare CLI binary; needs F1's App Store Connect key), and an update mechanism chosen in the task (a sourced comparison first). cargo-dist keeps releasing the CLI; the CLI's asset names do not change.

Constraint found in the survey: `install.sh`, `install.ps1` and the GitHub source (`src/source/github.rs`) resolve the host through `/releases/latest`. An app release in this repository that GitHub marks as latest would send every CLI installer and `ketch self upgrade` to a release without CLI assets.

Plan:
1. Decided (creator, 2026-10-01): app releases live in this repository (monorepo), tagged `desktop-v*` and created with `make_latest: false`. A script in `tests/` asserts the app workflow never creates a release eligible for latest, and that `install.sh`/`install.ps1`/the GitHub source still resolve only CLI releases.
2. Versioning: the app has its own version (`desktop-vX.Y.Z`), separate from the CLI's; release-plz and `scripts/release.sh` ignore it. Changelog section for the app via git-cliff with a path filter on `desktop/` and `crates/ketch-ffi/`.
3. Workflow `.github/workflows/desktop-release.yml`, `workflow_dispatch` only: macOS runner with Xcode 26; `just xcframework`; `xcodebuild archive` + `-exportArchive` with a Developer ID export options plist; signing with the existing `MACOS_CERTIFICATE`/`MACOS_CERTIFICATE_PWD` (hardened runtime on); a `.dmg`; `xcrun notarytool submit --wait` with the App Store Connect secrets from F1; `xcrun stapler staple` on the `.dmg`; `spctl --assess` as the smoke test; SHA256 checksum next to the asset.
4. Updates: compare Sparkle 2 and alternatives with primary sources (versions, dates, licence, EdDSA signing) and pick one; its signing key becomes a repository secret; the appcast is published with the release.
5. Homebrew: optionally a second cask (`ketch-app`) generated by a script like `scripts/cask.sh` and pushed to the tap — only after the first signed release works.
6. Docs: AGENTS.md "Releasing" gets an app subsection (secrets, tag scheme, the latest-release constraint).

Check: a dry run on a branch produces a signed, notarised, stapled `.dmg` that opens on a clean macOS 26 machine without a Gatekeeper prompt; `install.sh` still resolves the CLI release afterwards; the update feed moves an older build to the new one.

Status: PR https://github.com/pyrlyn/ketch/pull/190 merged 2026-10-01. Sparkle 2.10.0 with EdDSA; the feed is `releases/download/desktop-appcast/appcast.xml`, independent of `/releases/latest`. Two real bugs fixed on the way: `cliff.toml`'s unanchored `tag_pattern` and `select_release` ranking `desktop-v*` above CLI tags under `--pre`. Remaining: waiting on the creator for the secrets `APPSTORE_CONNECT_KEY`, `APPSTORE_CONNECT_KEY_ID`, `APPSTORE_CONNECT_ISSUER_ID`, `SPARKLE_ED_PRIVATE_KEY` and the matching `SUPublicEDKey` committed; the XCFramework step in `desktop-release.yml` is still a skipped hook (`XCFRAMEWORK: 'false'`) although R9 has merged; no real signed release has run.

### F14. Design system for the macOS app: `DESIGN.md` and tokens

The creator asked for a new, polished design with tokens (2026-10-01), following the F12 glass requirement: frosted or glossy glass with a 3D effect, macOS 26 Liquid Glass.

Plan:
1. Research the `DESIGN.md` format (a design spec agents read: tokens in front matter plus prose) with primary sources; follow it if it is a maintained spec, else a documented equivalent.
2. `desktop/macos/DESIGN.md`: principles, glass and depth rules, colour (light and dark, accent, semantic status colours for installed / update / error / busy), typography (SF Pro scale), spacing, radii, elevation levels (shadow and highlight per layer for the 3D effect), motion, icons (SF Symbols), components (sidebar, package row, card, progress, sheet, menu-bar extra, buttons), accessibility fallbacks (Reduce Transparency, Increase Contrast, Reduce Motion).
3. Tokens as one source: `desktop/macos/design/tokens.json` in the W3C Design Tokens format, generated into Swift (`Tokens.swift`, header says it is generated) by a maintained generator (e.g. Style Dictionary), run by a `just` recipe, with a drift check.
4. `desktop/macos/design/preview.html`: a static page rendering the tokens and key components in light and dark, for review.
5. Hand-off to F12: the app uses the generated tokens instead of literals.

Check: the generator reproduces `Tokens.swift` byte-for-byte; the preview renders both themes; text colours meet WCAG AA contrast on their glass backgrounds (checked by the script or documented per pair).

Status: PR https://github.com/pyrlyn/ketch/pull/188 merged 2026-10-01. Google Labs DESIGN.md spec (alpha, `@google/design.md` 0.4.0) with generated front matter; tokens in W3C DTCG 2025.10; Style Dictionary 5.5.5 with custom formats for Swift (four appearances), CSS and the front matter; `just design-check` covers drift, WCAG AA and lint. Research in `docs/research-design-system.md`. `Theme.swift` reads `Tokens.swift` since F16. Remaining: an Accessibility Inspector pass on the real Liquid Glass material.

### F18. Figma design for macOS, Windows and Linux

The creator asked (2026-10-01) to update the Figma file "ketch for macOS — Liquid glass"
(`v7OJLmQEyCFbJ63uSYpJ9g`) for the new requirements: the app is coming to Windows and Linux too
(R10, R11), the macOS app as built in F17 differs from the first mock-ups, and the core now
exposes decisions the screens do not show yet (binary choice, stopping running processes, the
ambiguous `bin` glob, the busy lock, pin and rollback).

Done when the file has a shared layer (common screen specs, shared semantic tokens, truly
cross-platform components) and per-platform pages; the macOS screens match the F17 app; the new
feature screens exist for macOS in light and dark; Windows 11 (WinUI 3 / Fluent) and Linux
(GTK 4 + libadwaita, or what R11 recommends) variants exist for Installed, Discover, Updates,
Package detail, Settings, the binary-choice dialog and the tray or its equivalent, in light and
dark; prototype links cover the new frames; and the Figma mapping doc names the pages,
collections, modes and the code syntax of every variable.

Execution plan:
1. Read the F17 SwiftUI views, `tokens.json`, R10 and R11 (if pushed), the `ketch-ffi` surface
   and the F17 screenshots.
2. Figma, shared layer: a `Platform` variable collection (modes macOS / Windows / Linux) for
   material, radius, spacing and type-size differences beside `Color` (Light / Dark), after
   checking the plan's mode limit; a Shared page with wireframe-level screen specs and the
   cross-platform components; per-platform pages.
3. Sync the macOS screens with F17 (bottom progress bar, Discover hero and shelf, Doctor fix
   text, monogram icons, native toolbar controls, busy banner, Settings tabs, orange update
   badges, smoke-tinted terminal).
4. New macOS screens, light and dark: binary choice, stop running processes, ambiguous `bin`
   glob, busy lock, activity detail, pin / unpin and rollback, update notifications; anything
   the core does not expose yet is marked "needs core" in the frame description.
5. Windows and Linux variants in each platform's idiom, light and dark, on the shared tokens.
6. Prototype links; screenshot-verify each step.
7. Update the Figma mapping doc (`desktop/macos/design/figma.md`, or a platform-neutral place
   if the structure warrants it), open a PR, wait for CI.

Status: the Figma work is done — shared pages, macOS synced with F17 plus ten new frames,
Windows (Fluent) and Linux (libadwaita, per R11) pages in light and dark, prototype flows on
every page. `figma.md` stays in `desktop/macos/design/` until D5 moves it. Left: the creator's
review of the file and the merge of the PR.

### M16. One module owns config file I/O

`AGENTS.md`: one module owns all config loading, validation and editing, and the rest of the code does not import `toml` or `toml_edit`. Today `registry.rs`, `manifest.rs`, `extra.rs`, `push.rs`, `wizard.rs` and `model.rs` use them directly, besides `config.rs` and `lockfile.rs`.

Done when TOML parsing, rendering and editing for the files ketch owns go through one module, and no other module imports `toml` or `toml_edit`. Behaviour does not change. Before starting, confirm with the creator whether `ketch.toml` manifests and `ketch.lock` belong to that module or keep their own, with only the TOML calls moved.

### D4. Contract fixtures for every app's fake core

Three apps each test against a fake core; if each fake invents its own records and event streams, they will drift from the real one and from each other. Research: section 2, "Written once".

Done when a set of language-neutral JSON scenarios (records, event streams with progress and `Abandoned`, `Busy`, `Cancelled`, decisions) is generated from the Rust types by a test that fails on drift, and the macOS app's fake core reads them; the Windows and Linux fakes read the same files when they exist.

### D5. Design tokens for XAML and GTK

`tokens.json` feeds only Swift today. The Windows and Linux apps should share ketch's brand (accent, status colours, spacing, radii, type scale) without imitating the glass. Style Dictionary has no XAML or GTK format, so it takes two custom ones. Research: section 2.

Done when the token source lives in `desktop/design/`, `just design-tokens` also writes a XAML `ResourceDictionary` (Light, Dark, HighContrast theme dictionaries) and a GTK stylesheet setting libadwaita's CSS variables, glass and elevation tokens stay macOS-only, generated files say so in their first lines, and a drift check covers all outputs. Brand tokens on every platform, with native surfaces, were decided by the creator (2026-10-01, open decision 6).

### D7. macOS: update notifications

The macOS app checks for updates on a timer (F12) but tells nobody unless the window or menu-bar panel is open. Builds on F12's live core; F12's remaining work (the `LiveKetchCore` adapter and the manual checks) stays in F12. Research: section 1.

Done when new upgrades since the last notice post one `UNUserNotificationCenter` notification, authorisation is asked only when the user turns notifications on in Settings, clicking it opens Updates, and a unit test covers which upgrades count as new.

### D8. macOS: `ketch://` links

A link on a web page or in the registry could open a package in the app. Whatever a link carries is untrusted input, so it is validated by the core. A link only opens a package page and never starts an install (creator, 2026-10-01, open decision 10). Research: section 1, "Deep links".

Done when `CFBundleURLTypes` registers `ketch`, `onOpenURL` opens the package page a valid link names, through the core's validation, any other action is refused, and tests cover malformed and hostile links.

### D9. macOS: VoiceOver pass

The macOS app's accessibility was only checked on the fake core. F14 keeps its own Accessibility Inspector pass on the real glass; this task covers labels. Localisation is not needed for now (creator, 2026-10-01, open decision 7), so no String Catalog. Research: section 1.

Done when every icon-only control has an accessibility label, and a VoiceOver walk through the nine screens finds no unlabeled control.

### D10. Windows: C# binding for `ketch-ffi`

The Windows app is C# + WinUI 3 over `ketch-ffi` (creator, 2026-10-01). `uniffi-bindgen-cs` last released for UniFFI 0.31.0; `ketch-ffi` is on 0.32.2. Decided by the creator (2026-10-01, open decision 4): first try the generator built from PR #176 (UniFFI 0.32.0), pinned to commit `0fc022aa1d73fb1dda91a778b63f2824d7dca58b`; if it fails against 0.32.2, use the C ABI from D15 through `LibraryImport`. Downgrading `ketch-ffi` to UniFFI 0.31.2 is not an option. Research: section 4.

Done when the chosen route generates C# for `ketch-ffi`, the generator is pinned (a commit or a version, no system install), a .NET 10 test calls `installed`, `doctor` and a cancelled install against a scratch root on Windows CI, and the route taken and why is in the research page.

### D11. Windows: WinUI 3 app shell on a fake core

The Windows app's screens can be built before the binding is settled, the way F12 started on macOS. Research: sections 3b and 3c.

Done when a WinUI 3 app (Windows App SDK, .NET 10, built with `dotnet build`, no Visual Studio required) has the common screens in a `NavigationView` with a `TitleBar` over Mica, `ContentDialog` and `InfoBar` for decisions and busy, light, dark and contrast themes, all on a fake core reading D4's fixtures, with a Windows CI job that builds and runs its tests.

### D12. Windows: the app on the real core

Swap the Windows app's fake core for the binding. Depends on D10, D11, D1 and D2.

Done when the app runs every screen on `ketch-ffi` through D10's binding, work runs off the UI thread with events marshalled to the `DispatcherQueue`, cancel and `Busy` behave as in the contract, and a manual pass against a scratch root is recorded.

### D13. Windows: tray icon, notifications, start at login, links

The Windows counterparts of the macOS menu-bar extra, notifications, login item and URL scheme. WinUI has no tray control, so the icon is Win32's notification area. Research: sections 1 and 3c.

Done when a notification-area icon opens the tray panel, `AppNotificationManager` posts update notices unpackaged, start at login and the `ketch` protocol are registered through `ActivationRegistrationManager` (with D8's validation rules), one instance runs at a time via `AppInstance`, and each can be switched off in Settings.

### D14. Windows: release pipeline

How the Windows app reaches users and updates itself, separate from the CLI's release. R10's open decisions on distribution and the Windows App SDK licence come first.

Done when CI builds an unpackaged, signed (Artifact Signing or the creator's choice) release on `desktop-windows-v*` tags without touching `/releases/latest`, updates arrive through the chosen route (Velopack or winget), and the creator's answers to R10's decisions are recorded.

### D16. Linux: Vala + GTK 4 app shell on a fake core

The Linux app's screens in Vala with GTK 4 and libadwaita, following the GNOME HIG, built before the C ABI is ready. GTK 4 + libadwaita over KDE and Blueprint for the markup were decided by the creator (2026-10-01, open decisions 1 and 2). Research: sections 3b, 3c and 5.

Done when a Meson project builds a libadwaita app with the common screens in an `AdwNavigationSplitView` that adapts to narrow windows, Blueprint files (pinned as a Meson subproject) for the UI, `AdwAlertDialog`, `AdwBanner` and toasts for decisions, busy and finished work, dark and high-contrast styles, all on a fake core reading D4's fixtures, with a Linux CI job that builds and runs its tests.

### D17. Linux: the app on the real core

Swap the Linux app's fake core for `ketch-capi`. Depends on D15 and D16.

Done when every screen runs on the C ABI, calls run off the main loop with events returned through the `GLib.MainContext`, cancel and `Busy` behave as in the contract, and a manual pass against a scratch root is recorded.

### D18. Linux: notifications, background and autostart

GNOME has no tray in its HIG; an app that checks in the background asks the Background portal and notifies through `GNotification`. Research: sections 1 and 3c.

Done when update notices go through `GNotification` (the Notification portal under Flatpak), background running and start at login are requested through the Background portal (libportal) with an XDG autostart entry outside a sandbox, `ketch://` links follow D8's rules, and there is no tray: the creator deferred a StatusNotifierItem tray (2026-10-01, open decision 8).

### D19. Linux: packaging and release

How the Linux app reaches users. R10 left Flatpak against distribution packages open, and Flatpak needs home access for PATH work.

Done when the creator has chosen the format, CI builds it on `desktop-linux-v*` tags without touching `/releases/latest`, the app ships AppStream metadata and a `.desktop` file that validate, and updates come from the chosen package manager.

### M17. `ketch import`: a package from winget, Homebrew or a Linux repository

Requested by the creator (2026-10-02). `ketch import winget|brew|linux <name>` looks a package up by name in another package manager, converts its definition to a ketch manifest, writes it as a user manifest (`~/.ketch/manifests/<name>.toml`) and installs it the normal way. Re-running it is idempotent: an unchanged conversion with the release already installed prints that everything is up to date and touches nothing; a changed conversion or a newer upstream version rewrites the manifest and installs. The manifest holds only what ketch needs (`name`, `source`, `kind` for an app, `bin`, `[asset.target]` pins); nothing else from the source is copied.

Hard rule: only packages whose artifacts are GitHub Release assets convert. Anything else writes nothing and exits non-zero with `<name> can't be converted: it is not distributed through GitHub Releases, and that is not supported yet.`

Done when the three converters, the command, docs (`docs/COMMANDS.md`, `docs/MANIFESTS.md`) and offline tests (recorded fixtures, a temp root, never the real config) are in, and `just check` is green.

Research (sources checked 2026-10-02):

- Homebrew: `formulae.brew.sh/api/formula/<name>.json` and `/api/cask/<token>.json` (https://formulae.brew.sh/docs/api/). A cask's `url` is the macOS arm64 download; `variations` keyed by macOS release (`sequoia`, …) are the Intel ones, `arm64_<release>` the arm64 ones; `sha256` may be `no_check`; `artifacts` lists `app`, `binary`, `pkg`, `installer` and others (response of `/api/cask.json`, 7778 casks). A formula's `urls.stable.url` is the source it builds from; its binaries are bottles on `ghcr.io` (`bottle.stable.files`), never GitHub Releases. Of 817 core formulae whose stable URL is under `releases/download/`, none is a prebuilt archive naming an OS and an architecture (`/api/formula.json`) — so a formula converts only in that case and is otherwise "source only".
- winget: the community source is the `microsoft/winget-pkgs` repository, `manifests/<first letter>/<Id with dots as folders>/<version>/<Id>.installer.yaml` (https://github.com/microsoft/winget-pkgs/tree/master/doc/manifest/schema/1.12.0). The community source has no public REST endpoint (the REST protocol is for private sources and the msstore source), so versions are listed with the GitHub contents API and the manifest read raw. `InstallerType`, `NestedInstallerType`, `NestedInstallerFiles` and `Commands` may sit at the root or per installer; one architecture may have several installers (per `Scope` or `InstallerLocale`), often with the same URL (`Git.Git` 2.55.0.5).
- YAML: `serde_yaml` is deprecated (crates.io, `0.9.34+deprecated`), `serde_yaml_ng` and `serde_norway` last released 2024; `serde-saphyr` 1.3.0 (2026-09-16, MIT/Apache-2.0, pure Rust) is maintained, so it reads the winget manifests. Its `rust-version` is 1.89, above ketch's declared MSRV 1.86, which nothing in CI checks — flagged for the creator.
- Linux: there is no cross-distribution source that names an artifact URL. Repology (https://repology.org/api) maps names across distributions but carries no download URLs (**unverified**: the API page could not be reached on 2026-10-02). Debian, Fedora and the official Arch repositories build from source, so their recipes point at source archives. Flathub is keyed by reverse-DNS app IDs, not package names. The AUR is name-based with a JSON API (https://wiki.archlinux.org/title/Aurweb_RPC_interface, `rpc/v5/info`, `rpc/v5/search?by=provides`), and its `-bin` packages repackage upstream's prebuilt artifacts, with per-architecture `source_<arch>` URLs and `sha256sums_<arch>` in a machine-readable `.SRCINFO` (https://wiki.archlinux.org/title/.SRCINFO). So `linux` means Arch Linux: the official repositories (https://wiki.archlinux.org/title/Official_repositories_web_interface, `.SRCINFO` from `gitlab.archlinux.org/archlinux/packaging/packages/<pkgbase>`) and the AUR, the official one first.

Execution plan:
1. This card.
2. `serde-saphyr` for the winget YAML (one commit, `toolchain.md` and `rust.md` rows).
3. Core `crates/ketch-core/src/import/`: the shared rules (a GitHub release URL, the tag, per-target asset globs, the rejection error) and one converter per source on plain data, with recorded fixtures under `crates/ketch-core/src/import/fixtures/`; the fetching behind a small trait so tests never reach the network; base URLs overridable by `KETCH_IMPORT_BREW`, `KETCH_IMPORT_WINGET_API`, `KETCH_IMPORT_WINGET_RAW`, `KETCH_IMPORT_ARCH`, `KETCH_IMPORT_ARCH_GITLAB`, `KETCH_IMPORT_AUR`.
4. Writing the manifest through `manifest.rs` (the module that owns user-manifest files), rendering through `wizard::render`, and the idempotency decision in the core.
5. `ketch import` in `cli.rs` and `cmd/import.rs`; end-to-end tests in `tests/import.rs` against a local mock of the sources and of the GitHub API.
6. Docs: `docs/COMMANDS.md`, `docs/MANIFESTS.md`, help snapshots, man pages.

Rules decided here:
- GitHub Releases means every artifact URL is `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>` (or `releases/latest/download/<file>`), all of one repository and one tag. A GitHub homepage or an `archive/` source tarball does not count.
- Mixed installers: if any artifact ketch would use, on any architecture, is hosted elsewhere, nothing converts — one manifest has one source, and a partial conversion would install on one machine and silently fall back to guessing on another.
- An artifact that is a source archive (an archive naming neither an OS nor an architecture, for a Homebrew formula) is not a release artifact.
- Installer formats ketch cannot place (winget `msi`, `msix`, `exe`, `inno`, `nullsoft`, `wix`, `burn`; cask `pkg`, `installer` and the other non-`app`, non-`binary` artifacts; Linux `.deb`/`.rpm`) are refused with their own message, after the GitHub check.
- A file at the manifest path that does not start with the ``# Written by `ketch import`` header is the user's and is never replaced; the command refuses instead.
- The host must be one of the converted targets, or nothing is written: a manifest this machine cannot install from would leave a package `ketch install` fails on.
- Idempotency: the rendered file is compared byte for byte with the one on disk, the installed version with the source's tag. Same file and installed ≥ source → "Everything is up to date", nothing touched; a newer tag → rewrite and upgrade; same version, changed file → reinstall (`force`); installed newer than the source → the installed release stays and only the manifest is redone. The install pins the source's asset (`asset_override`) and checksum (`expected_sha256`); a missing checksum warns and installs, checked the usual way.
- Bin paths and asset patterns have the version replaced by `*`, so the manifest keeps matching after the next release.

Status (2026-10-02): steps 1–6 in, on `feat/import-foreign-packages` (draft PR #215). Not done: `docs/ru` and `docs/uk` do not exist on `main` (only on the unmerged `ci/sync-docs-i18n`), so the doc changes are English only; the `serde-saphyr` MSRV question is open for the creator; the binaries inside a cask's `.app` (`binary` under `$APPDIR`) are not linked.
