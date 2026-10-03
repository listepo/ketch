### M8. Lifecycle hooks in a manifest

A manifest's `[hooks]` table names one shell line for each of `before_install`, `after_install`, `before_update`, `after_update`, `before_uninstall` and `after_uninstall`. `src/hooks.rs` runs them with `sh -c` (`cmd /C` on Windows), `KETCH_HOOK`, `KETCH_PACKAGE`, `KETCH_VERSION`, `KETCH_PREVIOUS_VERSION`, `KETCH_PREFIX`, `KETCH_BIN_DIR` and `KETCH_ROOT` in the environment, and the store prefix as the working directory whenever it exists. `install::commit` runs the before hook ahead of placement and the after hook once `state` records the package; `install::uninstall` does the same around unplace and removal. A reinstall of the same version is an install, not an update. A failing `before_*` hook stops the operation with its stderr as the detail; a failing `after_*` hook is a warning. Rollback runs the two update hooks; prune runs none. A hook is killed with its process tree after ten minutes via `plugin::run_with_deadline`, the runner source plugins already used.

Hooks are a trust boundary: they run only from a user-tier manifest (`~/.ketch/manifests`). A registry or built-in manifest carrying hooks is refused at install before anything is placed, and skipped with a warning on uninstall. `Manifest::validate` refuses a blank hook. Schema and semantics are in `docs/MANIFESTS.md`.

Tests: `src/hooks.rs` (environment, working directory, stderr detail, origin gate), `src/model.rs` (parsing, blank hook, misspelt key), `tests/install.rs` (order across install, upgrade and uninstall; a failing before hook installs nothing).

### B61. `self update` swap fails on a transient Windows lock

`replace_binary` retries a short antivirus lock. `io_retry` pauses 100, 150, 200, then 250 ms and gives up. It wraps the rename of the running binary aside, the copy of the new binary into place, and the restore rename/remove. Only `PermissionDenied` and Windows os errors 5 and 32 are retried; any other error returns on the first attempt. The success-path removal of the `.old` image is not retried: that file is the running image until this process exits, and B60 sweeps it on the next self command. Noted in `docs/TROUBLESHOOTING.md`.

Tests: a `PermissionDenied` that clears on the third call, a `NotFound` that returns with no pause, a lock that never clears and stops after the four pauses, and raw errors 5 and 32 counting as transient. Verified on macOS: `cargo clippy --all-targets --locked -- -D warnings` clean, `cargo nextest run --locked --all-targets` 588 passed. The Windows-only probe cases run in CI.

### B63. `self update` hangs forever when a spawned process never starts

`process::run_bounded` runs the child off the caller's wait, because an antivirus filter can hold process creation itself. After 30s a process listing is stopped and warned about by program and pid. After 60s `ketch --version` is stopped, the error says so, and restore puts the previous binary back. A spawn that never returns has no pid; the message says the wait was ended, and the spawn thread stops the child if creation later succeeds.

Tests: the stopped-probe wording, a command that outlives a short budget and is no longer alive, and on Windows `tree.com` (exit 0) / `where.exe` (exit 1). Verified on macOS with the same clippy and nextest run as B61. The `cfg(windows)` cases run in CI.

### F4. Self upgrade, in-use processes, auto-update

`ketch self update` is now `ketch self upgrade` (same verb as client packages). `self update` remains a clap alias so the Homebrew cask and existing scripts keep working. `ketch upgrade` and `ketch self upgrade` list other processes running from a file about to be replaced, ask whether to stop them, and on yes TERM then KILL (taskkill /F on Windows); a decline leaves them running and replacement continues as before. The current ketch pid is never offered. `auto_update` in `config.toml` / `KETCH_AUTO_UPDATE` defaults to `true`: `install` and `upgrade` refresh the registry and print that auto-update is enabled; a failed fetch is a warning. `false` leaves previous behaviour. Offline e2e sets `KETCH_AUTO_UPDATE=false`.

Tests: occupant listing and `--yes` stop in `src/process.rs`; CLI e2e in `tests/auto_update.rs` and `tests/self_update.rs` (alias + dry-run verbs).

### M4. Man pages and completions

`extra_paths` is classified as a man page or a completion from explicit `{ path, kind }` metadata or the path rules in `docs/MANIFESTS.md`. Ambiguous or untyped entries are refused at validate rather than guessed. Entries resolve only under the extracted payload; destinations go in `LinkRecord` with `role` man/completion so uninstall and relink use the same ownership proof as binaries. Old state without `role` still loads as binaries.

Platform methods name the user man root (`$XDG_DATA_HOME/man` or `~/.local/share/man`) and per-shell completion directories. `ketch doctor` reports those paths before anything is written. `ketch self install` / `self update` generate ketch's own man page and completions into the store prefix and link them; `ketch completions <shell> --install` does the same for one shell.

Tests: classification and plan in `src/extra.rs`, place/unplace extras in `src/platform/macos.rs`, doctor dests in `src/platform/mod.rs`, backward-read in `src/state.rs`.

### M7. `ketch why`

Resolution is inspectable without changing it. `evaluate_assets`, `select_release`, and `list_opts` are the functions install already uses: `ketch why <pkg> [--json]` runs one `Source::resolve` like install, records rejected assets and releases, and never fetches checksum sidecars or runs trust inspection.

The trace names the manifest tier and origin, the source, the selected version, scored and rejected assets, checksum and trust policy, and the final candidate. Secrets, URLs, headers, and bidi overrides stay out. Failure still prints the trace, then exits non-zero.

Tests: unit fixtures in `src/resolve.rs` and `src/source/mod.rs`; binary JSON and text snapshots in `tests/why.rs` for aliases, user-over-registry, registry-over-builtin, prereleases, pinned assets, and no-compatible-asset.

### M5. Registry maturity

This-repo half of registry maturity. `ketch registry validate` already failed on parse errors and name/alias collisions; it now also offline-installs `--changed` entries against `--fixture` (a file named after the package, or a folder of one file) in a throwaway root, so a `github:` source never hits the network. Clients (`ketch update`, lookup) still warn and skip a published bad registry.

`ketch update` writes `registry.meta.toml` under the ketch root (repo, GitHub tarball revision SHA, optional ETag, `fetched_at`), separate from the package folders. `ketch registry status` and the doctor registry line report age and source from that file with no network call. `ketch update` remains the only refresh.

Author/maintainer workflow, the exact command (`ketch registry validate .`, plus `--fixture` / `--changed`), and the fail-closed-CI / best-effort-client compatibility policy are in `docs/REGISTRY.md`.

F2 is not closed: the ketch-registry repository still needs a GitHub Actions workflow that runs `ketch registry validate` (and fixture installs for changed entries) on every pull request. Other tasks besides F2 remain in `plan.md`.

### M6. Rollback

Upgrade used to delete the previous prefix, so a rollback command would have promised versions that were already gone. State now keeps a retained-version record (version, prefix, asset digest, links, trust) plus a visible `retention.keep` policy (default 1). Old `state.json` files still load: missing fields mean keep-1 and no retained prefixes.

The previous prefix is kept only after the new version is placed; `ketch prune` is the only command that deletes retained prefixes. `ketch rollback <pkg> [--to <version>]` relinks a retained prefix without redownloading, preflights destinations, refuses pinned packages, and leaves the working version in place on failure. `list`/`info` show retained versions and the policy. Uninstall removes current and retained prefixes.

Tests: `a_v1_state_without_retention_fields_still_loads`, `retain_replaced_keeps_an_eligible_prefix_and_skips_the_same_one`, `select_retained_defaults_to_the_previous_and_names_a_miss`; e2e success, missing retained, occupied destination, pinned, uninstall after rollback.

### B50. Batch install documented as writing in asked order

Batch install was documented as writing packages into the store in request order; concurrent batches place them one at a time in completion order instead. README now says store writes follow whichever download finishes first, while exit status and printed results stay in request order.

### B52. "Nothing is written outside the tree except…" omits files

The README's outside-the-tree list omitted `./ketch.lock`, `./ketch.toml`, and in-place `ketch self update` replacing the running binary. README now names all three.

### B53. `brew uninstall --cask ketch` removal story

`brew uninstall --cask ketch` removes only Homebrew's bootstrap binary and leaves `~/.ketch` and every installed package in place. README now says so in the removal section.

### B55. Installer documented as only `ketch self install`

The curl and PowerShell installers also run `ketch path install` and can write a bootstrap copy at `--install-dir`. README now describes all three steps.

### B56. `--name` help narrower than the flag

`--name` clap help said it applied only when installing with `--path`, but the flag accepts any single-package install and overrides the installed name.

Widened the doc comment on `InstallArgs::name` in `src/cli.rs` to say it sets the installed name for a single package, with the file-basename default still noted for `--path`.


### B57. `link`, `unlink`, `self version` and `path status` undocumented

`ketch link`, `ketch unlink`, `ketch self version`, and `ketch path status` (the default for bare `ketch path`) existed but were undocumented. README's command list and PATH section now cover them.

### B59. Config table omits keys

The config table omitted `self_repo` / `KETCH_SELF_REPO`, the env-only `KETCH_GITHUB_API` override, and the 16-job cap on `jobs` / `--jobs`. README now lists `self_repo`, notes the API override and cap in prose, and marks `jobs` as capped at 16.

### B35. `tui` feature never compiled by CI and never shipped

The `tui` feature is optional and not in release tarballs, but nothing in CI compiled or tested it — so breakage there went unnoticed.

Added a `Test (tui)` step (`cargo test --all-targets --locked --features tui`) to the `check`, `check-linux`, and `check-windows` jobs in `.github/workflows/ci.yml`. `scripts/package.sh` is unchanged; AGENTS.md does not require shipping TUI in releases.

### B34. Crate-wide `allow(dead_code)` on non-macOS

`#![cfg_attr(not(any(target_os = "macos", target_os = "linux", target_os = "windows")), allow(dead_code))]` in `src/main.rs` suppressed dead-code warnings for the whole crate on unsupported hosts and was meant to paper over macOS-only code compiled on other targets. Removed the crate-wide attribute; macOS-only items already carry `#[cfg_attr(not(target_os = "macos"), allow(dead_code))]` in `src/platform/mod.rs` and `src/platform/scoring.rs`.

### B38. Plugin stderr dropped on timeout, oversize and non-UTF-8

Only the non-zero-exit path carried stderr into the error; a plugin killed at the deadline, one that wrote more than 8 MiB, and one that wrote non-UTF-8 all reported without it.

Added `stderr` to `Error::Plugin` and surfaced it through `Error::details()` like `Error::Command`. `run_plugin` now passes captured stderr into `plugin_fail` on timeout, oversize stdout, and non-UTF-8 output. Tests: `stderr_is_included_when_a_plugin_times_out`, `plugin_fail_includes_stderr_in_details`.

# ketch — completed tasks

### B31. TUI rows keyed by two names for one package

`prepare` emitted `PackageSpec::label()` (an alias, or a path) while `commit` emitted `manifest.name`, so a TUI row spun at Installing forever. Both prepare and commit now key the row with `PackageSpec::label()`, carried on `Prepared`. Install `completed` uses the same key. Test: `alias_or_path_install_does_not_stick_on_installing`.


### B42. `rstest` and `insta` declared but unused

`rstest` and `insta` were listed in `[dev-dependencies]` but not used as real test infrastructure: no `rstest` imports, and `insta` snapshot calls had no snapshot files. Removed both crates from `Cargo.toml`, replaced snapshot assertions in `src/resolve.rs` and `src/source/mod.rs` with explicit `assert_eq` checks, and dropped the snapshot helpers from `tests/why.rs`.

### B47. `release.sh` scrapes `cargo metadata` by regex

The regex depended on `"name"` immediately preceding `"version"` in `cargo metadata` output. When it failed, the script aborted on a leftover release branch with only "nothing was pushed" as guidance.

`package_version()` now reads the version via `cargo pkgid --offline`. `abort_release()` cleans up an uncommitted release branch automatically and, after a commit, explains how to push or delete the local (and remote) branch. Added `tests/release-sh-version.sh` and wired it into CI shell checks.

### B14. `--tui` confirmation hangs the terminal

The session entered raw mode before the command was dispatched, and in raw mode Enter is a carriage return while `ui::ask` reads a line — so `ketch upgrade --tui` hung at its confirmation with no way out. Reachable only from a source build with `--features tui`.

`confirm`/`prompt`/`prompt_required` now leave the alternate screen and `disable_raw_mode()` around the line read (via `with_tui_input_paused` → `Controller::pause_for_input`), then re-enter. Event polling is skipped while paused so a TUI `send` cannot steal Enter. Tests: `confirm_pauses_an_active_tui_session_before_reading`, `send_does_not_poll_input_while_paused_for_a_prompt`.

### B18. `registry push` skips registry extra checks

`push` stopped at `Manifest::deserialize` + `Manifest::validate`, while `registry::read_package` also refused a `local:` source and a `name` that disagreed with the folder — so a manifest could open a pull request that registry CI then rejected.

Extracted `registry::validate_registry_entry` (name/folder agreement and no `local:` source) and call it from both `read_package` and `push::load`. Added unit tests in `push.rs` and integration tests in `tests/registry_push.rs` for both refusal cases.

Shipped work moved out of `plan.md`. `CHANGELOG.md` is the release record.

### B46. `just check` is not the CI gate

`just check` ran `cargo test --locked` where CI runs `--all-targets`, and it skipped `scripts/package.sh` and the cask `brew style` gate while `AGENTS.md` called it "the whole CI gate".

`just test` and `just check` now use `cargo test --all-targets --locked`. Added `just lint-shell`, `just package`, and `just lint-cask` (macOS only) to `just check`, matching CI's `package` job on this host. `AGENTS.md` describes the local gate accurately.

### B48. `registry push` documented as validating like the registry

`docs/REGISTRY.md` still said push validated the file "exactly as the registry will" without spelling out the per-package checks B18 added, and it described fetching the registry before validation.

The Contributing section now states that `registry push` runs the same per-package checks as CI and `registry validate` (folder name, no `local:` source, `Manifest::validate`), does not scan for name collisions, validates before fetching the registry copy, and drops the old validate-only wording.

### B49. User manifest documented as contributable as-is

`docs/REGISTRY.md` claimed the same per-package checks apply to `~/.ketch/manifests/*.toml`, so a manifest that works locally can be contributed as-is. User manifests only pass [`Manifest::validate`](MANIFESTS.md); registry entries also require folder-name agreement and refuse `local:` sources.

The "What ketch checks" section now states that registry-only rules do not apply to user manifests, and that a manifest that installs locally is not necessarily contributable as-is.

### S1. `ketch self install` / `update` / `uninstall`

ketch is one of its own packages, installed from its own release and verified against a published checksum rather than trusted on first use.

### S2. Homebrew cask

Generated by `scripts/cask.sh` into `pyrlyn/homebrew-tap` on every release. Homebrew keeps only the bootstrap binary.

### S3. Complete removal

`ketch self uninstall` takes the packages, the root, the `PATH` block in every shell startup file and the Homebrew cask, after printing the list and asking once.

### S4. Publish-then-tag releases

A tag now exists only for a release that finished, so no installed copy can see a version whose binaries are missing.

### S5. `ketch config create`

A questionnaire that asks what each field of a `ketch.toml` should say and writes the file, so a manifest starts from answers rather than a copied example.

### S6. `ketch registry push`

Turns a project's own `ketch.toml` into a registry pull request, through a fork when it has to — and reviews before it sends: the registry's current copy is fetched first, an update shows its diff and asks, and `--yes` answers in advance for scripts. The old top-level spelling, which never asked, is gone.

### S7. Registry push hardening

Oversized contents-API files error instead of reading as empty; `find_pull` filters on the asked base; registry tarball fetch uses `api_base()` / `KETCH_GITHUB_API`.

### S8. `doctor --json`, leftover-cask/orphan/stale-lock checks, `outdated -j`

The JSON flag was already on the CLI and printed text; it now emits a report object. Doctor names a Homebrew cask left after uninstall, store prefixes with no state entry, and a `.lock` a crashed run left behind. `outdated` checks packages concurrently, like install. The `tap` job fetches `ketch-*.tar.gz` from the published release so a cask failure can be re-run without rebuilding.

### S9. `ketch registry validate`

Fail-closed check of a registry tree: every `ketch.toml` through `Manifest::validate`, plus name/alias collisions. The client still warns and skips; this is what `pyrlyn/ketch-registry` CI should run. `--json` for machines.

### S10. Cask install/uninstall smoke

The `tap` job `brew install --cask`s the generated file and uninstalls it before pushing to the tap.

### S11. `install.sh` root is `--root`

`--install-dir` no longer names the store; it is an optional bootstrap PATH location. Default root stays `~/.ketch`.

### S12. Local filesystem installs

`local:` / `ketch install --path` installs an archive, bare binary, symlink, or macOS `.app` from disk; `list`/`info` (text + JSON) surface `local_kind` and path; `outdated` skips them.

### M0. Cross-platform contracts

macOS assumptions made explicit before a second backend: inventory of Unix-only APIs; `src/platform/scoring.rs`; `src/platform/unix.rs`; `extract/macos` gated; CI `check-linux`; `self_update::replace_binary` and `local::copy_tree` use unix helpers behind `cfg(unix)`. `score_macos_asset` in scoring; table-driven `is_ours` / `destination_available` / `clear_destination`; non-macOS tests for `host()`, `doctor`, `install`, and `list`.

### M1. Linux

Native Linux CLI: `src/platform/linux.rs`, `tests/install_linux.rs`. Bin-dir symlinks, no `.app` or macOS trust behaviour. Trust is `NotApplicable` until a verifier exists.

### M2. Windows

`src/platform/windows.rs`, `tests/install_windows.rs`. Copies into the bin dir and records `CopiedFile`. `ketch path install` on Windows writes the user PATH. `install.sh` fetches the host tarball; `release.yml` publishes Windows tarballs; CI runs the Windows suite on Windows.

### B5. Empty `KETCH_GITHUB_TOKEN` voids fallback

`KETCH_GITHUB_TOKEN=` returned `Some("")`, which short-circuited the `.or_else` chain to `GITHUB_TOKEN` and `GH_TOKEN` before the trailing filter. Each env var is now filtered with `.filter(|t| !t.trim().is_empty())` before the next fallback, matching `KETCH_APPS_DIR` and `KETCH_REGISTRY`. Unit test `an_empty_ketch_github_token_falls_back_to_the_next_token_variable` verifies empty `KETCH_GITHUB_TOKEN` still picks up `GITHUB_TOKEN`.

### B16. Empty boolean environment variable fails every command

`KETCH_LINK_APPS=`, `KETCH_PRERELEASE=`, `KETCH_ALLOW_EMULATION=`, `KETCH_STRIP_QUARANTINE=` and `KETCH_REQUIRE_CHECKSUMS=` each exited with "must be a boolean, not ``".

`env_bool` now filters empty and whitespace-only values with `.filter(|v| !v.trim().is_empty())`, matching `parsed`, `KETCH_REGISTRY`, and the other settings. Unit test `an_empty_boolean_environment_variable_is_treated_as_unset` covers all five keys with empty, space, and tab values.

### B10. Registry swap deletes working copy before rename

`swap_in` moved the working copy aside before installing the fresh tree, restored it when the second rename failed, and only then removed the aside — so a failed swap no longer leaves `<root>/registry` missing and concurrent installs still see `registry::exists() == true`. Covered by `a_failed_swap_puts_the_old_registry_back` on Unix.

### B11. Payload symlink entry point never discovered

`discover_executables` kept only regular files, so `bin/tool -> ../libexec/realtool` linked `realtool` under its internal name and left `tool` off `PATH`.

`payload_executable_entry` now accepts a symlink whose canonical target is a regular file inside the payload and returns the symlink's own path; `bin/` preference still keys on that entry path. The old lexical `ParentDir` check rejected valid relative targets such as `../libexec/realtool`. Covered by `discover_executables_keeps_a_bin_symlink_under_its_own_path` on Unix (and Windows when symlinks are available).

### B13. User copied `.app` deleted on stale `CopiedApp`

For a copied bundle any directory at the recorded path still counts as ours, so `unlink`/`uninstall` removes a user's own copy.

`still_placed` for `CopiedApp` now requires `record.target.exists()` in addition to the link being a directory, so stale records cannot authorize `unplace` or `clear_destination` once the store copy is gone. Covered by the directory case in `unplace_leaves_a_file_the_user_put_where_a_link_was` (macOS), `a_recorded_link_is_ours_only_while_the_disk_still_agrees` (unix `is_ours`), and `unplace_refuses_to_delete_a_replaced_copied_app_without_a_store_target` (Windows).

### B12. `registry validate` passes a symlinked `ketch.toml`

The guard that stopped `check_tree` reading through a symlink also dropped the folder from discovery, so a tree with one valid package and one symlinked `ketch.toml` could still report validated and exit 0.

`candidate_package_dirs` now lists folders whose `ketch.toml` exists even when it is a link; `is_package_file` rejects non-regular files. `check_tree` records a `ValidationError` without reading through the link; `load_dir` warns and skips. Covered by `check_tree_never_reads_through_a_symlinked_package_file` and `load_dir_warns_when_ketch_toml_is_a_symlink` on Unix.

### B6. `--verbose` never reaches the log

`--verbose` detail never reached the log, which `README.md` and the module comment in `log.rs` promise it does. `ui::debug` records at `Level::Debug`, but the sink wrote only records at or below `cfg.log_level`, and `--verbose` raised only the terminal level.

`log::init` now takes the verbose flag and `file_level` raises the sink to `Debug` when `--verbose` is set and logging is not off. Unit test `verbose_writes_debug_detail_to_the_log_file` verifies debug detail is written at the default info level when verbose is on.

### B19. `local:` symlink to archive or `.app` never installs

`classify` returned `Symlink` for any symlink, so `install --path link.tar.gz` forged a `bin` entry named after the link and then failed.

`classify` now canonicalizes a non-dangling symlink and classifies by the resolved target (archive, `.app`, binary, or plain-directory error) instead of returning `LocalKind::Symlink`. Unit tests `classifies_a_symlink_to_an_archive`, `classifies_a_symlink_to_an_app_bundle`, and `classifies_a_symlink_to_a_binary` verify the fix.

### B2. `install local:<fifo>` hang

`source/local.rs` classifies by opening the path, so a FIFO with no writer blocks in `open(2)` before anything can time out, and a character device never reaches EOF. A user manifest or `--path` can still pass `local:`.

`classify` and `download` now inspect paths with `symlink_metadata` and refuse anything that is not a regular file, a symlink, or a directory before `read_head` or `fs::copy` can open them (`ensure_is_regular_file`, `ensure_local_payload`). Unit test `refuses_a_named_pipe_before_open_can_block` verifies a FIFO is rejected without hanging.

### B15. Questionnaire accepts answers that fail validation

`cmd/config.rs` validated nothing while asking, and `Manifest::validate` ran once at the end, so every other answer was thrown away.

Added prompt-time validators in `wizard.rs` (`validate_package_name`, `validate_alias_list`, `validate_extra_path_list`, `validate_bin_entry`, `prompt_until_valid`) that reuse the same `usable_file_name` / `contained_path` predicates as `Manifest::validate`. `cmd/config.rs` now re-asks on failure for package name, `provides`, `extra_paths`, and `bin` entries; `Manifest::validate` remains the final backstop. Unit test `invalid_answers_are_reasked_before_accepting` verifies an invalid name is rejected and the question is asked again.

### A1. Audit fixes already landed

`unplace`/`is_ours` no longer delete a file the user put where a link used to be; `install.sh` no longer links the installed binary to itself when `--install-dir` respells `<root>/bin` (nor resolves a relative `--root` inside its own temp dir); a Windows-built zip whose members have no execute bit installs; `registry validate` refuses a symlinked `ketch.toml` and a `local:` entry and sees two folders that land on one name; a curated manifest is found from a differently-cased `owner/repo`; nested manifest tables reject unknown keys; a lockfile entry with an unrecognised `target` is refused; a locked asset is pinned so its hash stays checkable; an empty `KETCH_ROOT`/`KETCH_APPS_DIR` is treated as unset; client-app text is filtered on its way to the terminal.

### B39. `ketch history --limit 0` says nothing was recorded

`--limit` is a `u32`, `LIMIT 0` yields no rows, and the empty-result branch printed "no history recorded yet" (or "for {pkg}") for both an empty database and a limit that asked for nothing.

`history` in `src/cmd/query.rs` now returns quietly when `--limit 0` yields no rows, so that case is not read as an empty database. JSON mode still emits `[]`. Integration test `history_with_a_zero_limit_does_not_claim_nothing_was_recorded` covers text and JSON output after an install.


### B44. `tap` job can mix version and checksums from different releases

`VERSION` came from `needs.version` (HEAD) while the tarballs were fetched by `TAG` from the published release, so a cask could advertise one version with checksums from another.

Added `scripts/tap-release-version.sh` to strip the `v` prefix from the published release tag and fail when it disagrees with the workflow version. The `tap` job now resolves the version right after downloading assets, passes it through `steps.release.outputs.version` to cask generation and the tap push, and CI exercises the guard.
### B33. Lockfile with empty `asset` fails `sync`

`LockedPackage::validate` checks name, tag, source and hash but not `asset`, so `asset = ""` is a valid file; `choose_asset` then fails during `sync` with an opaque "release has no asset named ``" error.

`Lockfile::validate` now refuses a blank `asset` the same way it refuses a blank `tag`, so `Lockfile::load` (and therefore `sync`) fails early with a clear message. Unit test `a_blank_asset_is_refused` covers it.

### B20. `self update --dry-run` says would update when it would not

`replaced: false` covered both "already current" and "dry run", and the verb was chosen from `dry_run` alone.

Added `would_update` to `SelfUpdate`, fixed the already-current guard to treat `v`-prefixed tags as the same version (`matches_request`), and chose the verb from `would_update` instead of `dry_run`. Integration tests in `tests/self_update.rs` mock the GitHub API and assert the dry-run message.


### B22. Log keeps bidi and zero-width characters

`log::escape` drops `is_control()` characters but not the invisible formatting ones, though the file is meant to be `cat`ed.

`log::escape` now also drops bidi and zero-width formatting characters via `changelog::is_invisible`, which is now `pub(crate)` for reuse. Unit test `a_record_drops_bidi_and_zero_width_formatting` verifies the strip.
### B23. Release notes and `info --json` prose unfiltered

`ketch self update` prints the release body with `ui::out`, and `ketch info --json` serialises manifest prose as it stands; serde escapes C0 but not bidi overrides.

`ui::printable` is now `pub(crate)` so command code reuses the same filter as status lines and tables. `ketch self update` passes release notes through it before `ui::out`. `ketch info --json` runs manifest, source, and asset prose through `json_prose` / `ui::printable` before serialisation. Unit test `json_prose_strips_bidi_overrides` and integration tests `info_json_keeps_bidi_out_of_the_json_it_prints` and `release_notes_are_filtered_before_they_reach_stdout` verify the strip.





### B8. `changelog <pkg>@<version> --file` not installed

`elsewhere` treated any exact version as "not local", and `state.find` was handed the raw `pkg@version` string, which matches no key. The shipped `CHANGELOG.md` on disk was refused with "not installed".

Fixed by looking up the installed package via `spec.alias` (and source ref fallbacks), comparing the requested version against the installed tag before treating the payload as local, and keeping `--file` on the installed copy when versions match. Regression test: `changelog_file_with_an_explicit_version_reads_the_installed_payload`.

### B17. Failed checksum fetch reported as missing

`GitHubSource::checksums` swallows a failed sidecar request with a `--verbose`-only debug line, and the install then says "published no checksum; trusting <hash> on first use" — which is false. Fix: remember that a checksum file existed and could not be read, and say that.

`verify_checksum` no longer swallows `checksums()` fetch errors. When a sidecar exists but cannot be read, the failure is carried through `Prepared`/`Installed` as `checksum_unavailable`, and `report` warns that a checksum file could not be read instead of claiming none was published. When `require_checksum` is set, the fetch error fails the install. Added `install::tests::a_failed_checksum_fetch_is_not_reported_as_missing`; `github::tests::a_checksum_fetch_failure_is_not_reported_as_a_missing_file` already covered the source layer.

### B1. Plugin child hang

A plugin that orphans a child holding its stdout hangs ketch forever. `source/plugin.rs` kills only the direct child and then joins its reader threads inside `thread::scope`; EOF on the inherited pipe never comes while a grandchild holds it, so the deadline its own documentation promises bounds nothing. Discovery probes every plugin on every source-loading command, so one such plugin hangs `install`, `search` and `info` too. Fix: run the child in its own process group and `killpg` it on the deadline, and bound the reader side so `output()` returns within the deadline whatever the grandchildren do.

Plugin subprocesses now start in their own process group (Unix `process_group(0)`, Windows `CREATE_NEW_PROCESS_GROUP`). On the deadline, and after the child exits, the whole tree is stopped (`kill -s KILL -- -<pid>` / `taskkill /T /F`). Reader threads are joined with the remaining deadline, so `output()` returns even if a grandchild still held a pipe. Regression: `a_plugin_that_orphans_a_child_holding_stdout_is_stopped_within_the_deadline`.

### B4. Process lock can be stolen

The process lock can be stolen from a live holder. B reads a dead pid, forks `ps` to check, and by the time it renames, A has reclaimed the lock — `rename` is not compare-and-swap. A failed pid write is also swallowed, leaving an empty lock everyone treats as stale. Fix: after the rename, re-read the moved file and claim it only if it still holds the value judged stale; otherwise rename it back and report `Locked`.

After `rename`, `take` re-reads the moved file and keeps the claim only if it still holds that stale value; otherwise it renames the file back and returns `Locked`. A failed pid write removes the empty lock and surfaces the IO error. Tests: `take_does_not_steal_a_lock_that_changed_during_the_stale_check`, `a_failed_pid_write_does_not_leave_an_empty_lock`.

### B7. `ketch info` fails when source is unavailable

A missing or too-new plugin made `info` exit 1 with `no source is registered for scheme …`, though the comment above the manifest fallback promises that an installed package always has an answer, and `outdated` only warns.

For an installed package, `info` now treats the source as optional: it still prints name, version, prefix and binaries, and warns that url, latest, stars, license, archived (and assets when asked) cannot be shown. An uninstalled package still fails with `UnknownScheme`. Regression: `info_still_reports_an_installed_package_when_the_source_is_unavailable` covers a deleted plugin and a protocol-too-new plugin.


### B9. `outdated --json` cannot report failed checks

The text output prints "N could not be checked"; `--json` printed `[]` at exit 0 for the same run, so a machine consumer could not tell "everything is current" from "the network was down".

`outdated --json` now emits an object `{status, outdated, failed, unreachable}`. `unreachable` is the count of sources that could not be checked. Failed checks still exit non-zero. Tests: `outdated_json_marks_a_total_failure`, `outdated_json_reports_unreachable_count_when_a_source_cannot_be_checked`, `outdated_json_reports_partial_failures_and_still_exits_non_zero`.

### B43. `force` dispatch republishes HEAD, not the repaired tag

The `version` job derived `TAG`/`VERSION` from `cargo metadata` at the checked-out commit and never looked at the dispatch inputs, while publish did `gh release upload "$TAG" dist/* --clobber`. After main moved, a force re-run could clobber a different release.

`workflow_dispatch` now takes a `tag` input. Force requires it, uses it as `TAG`, and fails if `GITHUB_SHA` is not that tag's commit or if `Cargo.toml` at that commit disagrees. It no longer guesses the tag from HEAD.
### B45. Breaking-change reminder skipped by `!` before the colon

`case "${subject%%:*}" in *!*) exit 0` accepted `feat(api!): …`, which commitlint does not treat as a breaking marker.

The hook now skips the CLI-surface reminder only when the header ends with `!` (`feat!:` / `feat(scope)!:`), matching commitlint. Added `tests/commit-msg-breaking.sh` and wired it into `just lint-commits`.


### B27. `.dmg`/`.pkg` detected by extension ahead of content

Both sat first in the macOS extractor list and accepted the file name alone, which `extract/mod.rs` says never happens.

`DmgExtractor` and `PkgExtractor` now detect only by content (`koly` trailer and `xar!` magic). Removed the extension fallback and `has_extension`. Added tests that plain files named `.dmg`/`.pkg` are not claimed, and that content without matching extensions is still claimed.
### B28. `is_rejected` matches `sources` inside `resources`
### B29. tar directory members lose mode and mtime

`EntryType::Directory` went through `walk_inside`/`create_dir` and never `entry.unpack`, so a `private/` member at 0700 landed 0755.

Directory members now use `ensure_parent` plus `entry.unpack`, matching regular files, so permissions from the tar header are applied. The `tar` crate does not restore directory mtimes on unpack (it returns before setting them); mode is covered by unit test `tar_directory_members_keep_mode`.

`NON_BINARY_TOKENS` was matched with `contains`, so `tool-1.0-macos-arm64-resources.tar.gz` was refused as source code.

`is_rejected` now uses the existing `token_at` helper for `NON_BINARY_TOKENS`, matching name/path parts instead of substrings. Replaced the `"-src-"` entry with `"src"` so source builds still reject with whole-token matching. Unit test `is_rejected_matches_non_binary_tokens_as_whole_parts` verifies `*-resources.tar.gz` is accepted while `*-sources.tar.gz` and `*-src-*` builds are rejected.

### B21. Table cell with newline or tab breaks the row

A registry `ketch.toml` with a multi-line `description` prints its second line unindented under `ketch search`'s table, and a literal tab shifts every later column. `table` has to fold its cells onto one line.

`table_lines` now folds each cell onto one line after `printable`: `fold_line` runs `split_whitespace().join(" ")` so newlines and tabs cannot break a row or shift later columns. Regression: `a_table_row_stays_on_one_line_when_a_cell_has_newlines_or_tabs`.
### B24. `install.sh --version` misses `v`-prefixed tags

`release.yml` creates the tag as `v$version` and `install.sh` pasted `--version` straight into the download URL. The cask already builds `v#{version}` itself.

After version resolution, `install.sh` now prefixes a bare semver with `v` before building release download URLs (a value that already starts with `v` is left alone). The stub-release harness only serves `v9.9.9` assets, and `version_flags_accept_bare_and_v_prefixed_tags` checks both `9.9.9` and `v9.9.9`.

### B3. `self uninstall` leaves bootstrap link

`self uninstall` left `install.sh`'s bootstrap link dangling on `PATH`. An explicit `--install-dir` followed by `self uninstall --yes` removed the root and left `<install-dir>/ketch -> <root>/bin/ketch` pointing at nothing.

`ketch self install --link-dir <dir>` records that path as a `LinkRecord` (including when the package is already installed) so `install::uninstall` takes it back. `install.sh` passes `--link-dir` and no longer writes outside the root itself. Tests: `uninstall_takes_back_a_bootstrap_link_dir`, `self_uninstall_removes_a_bootstrap_link_outside_the_root`.

### B25. Exact version older than newest 30 releases not found

The `v`-prefix retry covers the everyday spelling, but the fallback still lists one page: a tag spelled another way on a busy repository ends in `no release found`.

Exact tag lookup still tries `/releases/tags/{tag}` and the `v`-prefixed spelling. When both miss, `resolve_from_listing` walks GitHub `/releases` with `per_page` and `page` until a page is short, `pick` matches, or `MAX_RELEASE_PAGES`. `list_releases` still returns page 1 only. Regression: `an_exact_tag_not_on_the_first_list_page_is_still_resolved`.


### B30. `find_mount_point` takes the first mount

`.find` contradicted its own doc comment; a DMG with a helper volume mounted first gave `copy_volume` the wrong volume.

`find_mount_point` now keeps the last mounted directory from `hdiutil attach` output. Regression: `prefers_the_last_mount_when_hdiutil_lists_multiple_volumes`.

### B26. Foreign operating systems still scored

A release shipping only `freebsd`/`netbsd`/`plan9` assets had one accepted (as an emulated `x86_64` build) and linked instead of failing.

`FOREIGN_OS_TOKENS` in `src/platform/mod.rs` lists unsupported OS name tokens (`freebsd`, `netbsd`, `openbsd`, `plan9`, `dragonfly`). `names_foreign_os` in `src/platform/scoring.rs` rejects them in every platform scorer before architecture matching, so they are never scored as emulated x86_64. Regression: `foreign_operating_systems_are_never_selected`.

### B37. Plugin `digest.algo` ignored

The wire type carries the algorithm and `docs/PLUGINS.md` never restricted it, but `install::verify_checksum` compared the hex against its own sha256 unconditionally.

`verify_checksum` now uses an inline plugin digest only when `algo` is `sha256`; other algorithms fall through to `checksums()`. `docs/PLUGINS.md` documents the restriction. Regression: `ignores_a_non_sha256_plugin_digest`, `uses_a_sha256_plugin_digest_without_calling_checksums`.

### B32. `self update` replaces whatever binary is running

With no `ketch` entry in `state.json` it copied the release over `current_exe()` with no check that the path is inside the root.

In-place self-update now refuses when the running binary is outside `cfg.root` (`is_inside_root`), with a message to run `ketch self install` first. Unit tests cover inside, outside, and prefix-trap paths.

### B36. Ambient `KETCH_*` variables leak into e2e tests

The sandbox helper removed only the three GitHub token variables, while `Config::load` reads eleven other `KETCH_*` settings from the environment before the sandbox's own `config.toml`. `ketch_with_path` now strips every ambient `KETCH_*` variable, then sets only `KETCH_ROOT` and `KETCH_APPS_DIR`.


### B54. ROADMAP promised every version stays in the store

ROADMAP said every version stays in the store and rollback was a simple relink; upgrades actually delete the previous prefix until M6 ships retention.

The rollback bullet now states retention and `ketch rollback` are M6 (not shipped), that upgrades currently remove the previous store prefix, and that pruning would enforce a retention policy rather than keeping every version indefinitely.


### B51. `src/shell.rs` called the only writer outside the root

`src/shell.rs` is called the only writer outside the root; `/Applications`, the binary and the cask are too. Rewrote the Layout paragraph in `AGENTS.md` to list bootstrap binary placement, `/Applications`, platform links, and shell PATH edits separately.

### B58. PLUGINS.md flag order disagrees with the client

PLUGINS.md listed `releases <id> [--prerelease] [--limit N]` while the client sends `--limit N` before `--prerelease` when invoking plugin `releases`. Updated the subcommand signature in `docs/PLUGINS.md` to `releases <id> [--limit N] [--prerelease]`.

### B60. LOCKFILE.md omits unknown-`target` check

LOCKFILE.md's "Refused" table omitted the unknown-`target` check that `Lockfile::validate` performs.

Added a row to the table in `docs/LOCKFILE.md`: a `target` ketch does not recognise is refused because it silently turns the entry into a cross-target one, so the recorded asset and hash stop applying and a hash that drifted under the tag reads as clean.
### B41. Terminal filter and nested tables have no failure-path test

`ui.rs` covered `table_lines` only: `step`, `success`, `warn`, `error`, `note`, and `debug` all filter client text through `ui::printable`, but nothing failed if one of them stopped calling it. `Registry`'s `deny_unknown_fields` had no test showing that a `[[package]]` file with a stray top-level key is refused.

Line builders (`step_line`, `success_line`, `warn_line`, `note_line`, `debug_line`, `error_lines`) mirror what the status helpers emit so tests can read the filtered text without touching stderr. Regression tests: `step_detail_cannot_redraw_the_terminal`, `success_detail_cannot_redraw_the_terminal`, `a_warning_cannot_redraw_the_terminal`, `a_note_cannot_redraw_the_terminal`, `debug_output_cannot_redraw_the_terminal`, `an_error_headline_cannot_redraw_the_terminal`, `error_details_cannot_redraw_the_terminal`, `an_error_hint_cannot_redraw_the_terminal`. `a_registry_file_with_a_stray_top_level_key_is_refused` checks `parse_registry` rejects an unknown top-level key beside `[[package]]`.
### B40. Three tests weaker than their names

`check_tree_treats_name_collisions_as_errors` passed against the old name-only `collisions()` too. The four Mach-O asserts in `recognises_program_headers` passed on 32-bit-only detection. `registry_validate_rejects_a_bin_name_that_would_escape` accepted either `.zshrc` or any `file name` mention.

`check_tree_treats_name_collisions_as_errors` now uses the `foo`/`foo.git` path-keyed collision and asserts the error is reported against the registry root. `recognises_program_headers` loops the four on-disk Mach-O prefixes, requires 64-bit variants to exceed 32-bit-only matching, covers universal LE, and rejects a near-miss prefix. `registry_validate_rejects_a_bin_name_that_would_escape` requires both `binary name` and `not usable as a file name` in stdout.

### A2. 2026-09-20 product audit — docs sync, version guard, troubleshooting, reference plugin, e2e gaps

ROADMAP rewritten to match reality (signatures → shipped M3, man pages → shipped M4, `why` → shipped M7, registry maturity → partial with registry CI dropped as F2); `todo.md` synced with `plan.md`. Version drift closed (`Cargo.toml` = CHANGELOG 0.4.6 = tag `v0.4.6` = `site/hugo.toml`) plus a CI guard: `tests/crate-version.sh` fails when crate version, latest tag, and changelog entry disagree (wired into `just lint-shell` and CI). Registry CI resolved as dropped with local validation + pre-push hook in `docs/REGISTRY.md`; staleness needs no new code (`registry status` / doctor already report age from `registry.meta.toml`). Added `docs/TROUBLESHOOTING.md` (site-wired), `examples/ketch-source-example`, trycmd help snapshots (`tests/cases/help*.trycmd`), `tests/plugin_fail.rs`, `tests/lock_extras.rs` (lock exit 8 + extras link/unlink, new `Sandbox::ok_env`).

Verified: `cargo fmt --check` clean, `cargo clippy --all-targets -D warnings` clean, full `cargo nextest run` 571 passed, shell checks green. Left open: notarisation secrets (creator step), multi-version/aqua evaluation (see A3).

### A3. Multi-version side-by-side and aqua parity — evaluated, not built

Audit item 13, verdict after reading the tree; no code changed. Side-by-side versions are already answered by retention: upgrades keep the previous prefix (`retained` in state, default `keep = 1`), `ketch rollback` relinks without redownloading, `ketch prune` drops old ones (see M6 in this file). Two versions on PATH at once is deliberately not a thing: one bin dir, one link per name. Global lockfile UX is already shipped (`ketch lock` / `--check` / `sync`, see `docs/LOCKFILE.md`); built-in catalog is already shipped (`src/builtin.toml` tiers in `src/manifest.rs`). No gap to fill, no issues filed.

### F1. Notarisation — done (ketch side)

Done on the ketch side: `release.yml` has a `Notarise` step for the two signed targets, gated on `KETCH_NOTARIZE == 'true'`. Once on, it fails on a missing `APPSTORE_CONNECT_KEY` (the `.p8`, base64), `APPSTORE_CONNECT_KEY_ID` or `APPSTORE_CONNECT_ISSUER_ID`. It zips the packed binary with `ditto`, runs `xcrun notarytool submit --wait`, and fails unless the status is `Accepted`. The smoke test requires `spctl` to report `source=Notarized Developer ID`. `AGENTS.md` Releasing documents the switch and the three secrets. `tests/release-yml-notarize.sh` is wired into `just lint-shell` and CI.

Not done (creator step, needs the App Store Connect key): add the secrets, set `KETCH_NOTARIZE=true`, and run a release.

### F2. Registry CI in ketch-registry — dropped

`ketch registry validate` exists in this repo (tree checks, name/alias collisions, optional `--fixture` / `--changed` offline-install). There is deliberately no registry-side workflow: ketch-registry removed its only workflow (commit `5a9bbd6`, "no CI is wanted in this repo"). The documented substitute is local validation plus a pre-push hook (`docs/REGISTRY.md`). Collisions are fatal in `validate` and warnings on `update` by design.

### M3. Provenance and signatures — done

`trust` table on Manifest (verifier sigstore|minisign|gpg, mode require|warn, sidecar templates, issuer + repository/identity, public_key, fingerprint), checked in `Manifest::validate`; docs/MANIFESTS.md. `src/trust.rs`: sigstore offline against an embedded trusted root plus Rekor SET check, minisign-verify with the pinned key, pgp with an inline key pinned by fingerprint (never a keyring); fail closed unless `mode = "warn"`. Results in `info` (text + JSON), the install report and the log. Fixtures in tests/fixtures/trust, unit tests in trust.rs, e2e in tests/trust.rs.

### F5. Config reset and shared file backup — done (ketch side)

`ketch config reset` writes `config.toml` with compiled defaults after confirming, backing the existing file up beside itself as `config.toml.bak-<unix-seconds>` via the `file-backup` crate. `ConfigCommand::Reset { yes }`, `Config::default_toml()`, `reset()` in `src/cmd/config.rs`, e2e in `tests/config_reset.rs`, docs in README Configuration and `docs/COMMANDS.md`. Unit tests `default_toml_parses_back_to_compiled_defaults` and `a_reset_file_loads_back_to_the_effective_defaults` in `src/config.rs` (plus the `ENV_GUARD`/`CleanEnv` fix for the flaky token-fallback test).

Not done (needs rtok owner): `rtok-agent-sdk::backup` still has its own copy and does not re-export the shared crate.

### R1. Release the way rtok does — done

The creator asked for ketch's release to work like `pyrlyn/rtok`'s. cargo-dist 0.32.0 (`dist-workspace.toml`, pinned in `mise.toml` with git-cliff 2.13.1) generates `release.yml` with `dispatch-releases`: all five targets build, are signed (macOS) and smoke-tested, and only then does the `host` job create the tag and the release, with the `CHANGELOG.md` section as notes. `scripts/dist-generate.sh` patches what dist has no setting for: the `MACOS_CERTIFICATE`/`MACOS_CERTIFICATE_PWD` secret names, the Notarise and Smoke test steps from `.github/build-check.yml`, and the aggregate `SHA256SUMS` plus a download-size table. `.github/build-setup.yml` adds the cache and the codesign identity step. The cask moved to dist's publish job `.github/workflows/tap.yml`; `scripts/cask.sh` now reaches the binary inside the tarball's `ketch-<target>/` directory. Entry points: `bump.yml` (verify, then `scripts/release.sh <level>`), release-plz (its merge, gated on the `chore: release vX.Y.Z` title and a `release-plz-` branch, runs verify and `scripts/release.sh patch --no-bump`), and `just release`. `verify.yml` is the shared gate. git-cliff writes the changelog from `cliff.toml` for both release-plz and `scripts/release.sh`. CI packages with `dist build` and fails on a stale `release.yml`. Removed: `scripts/package.sh`, `tests/release-sh-version.sh`, `tests/release-yml-notarize.sh`; added `tests/release-sh.sh` and `tests/release-workflows.sh`. Asset names are unchanged (`ketch-<target>.tar.gz`, `SHA256SUMS`); `install.sh`, `install.ps1`, `self upgrade` and a store install all find the binary in the new layout. Deliberate differences from rtok: no dist installers or updater, a cask rather than a formula, `.tar.gz` on Windows, the Rust toolchain not pinned, and the version commit subject `chore: release vX.Y.Z` because commitlint requires conventional commits. Not verified: a real release run, which needs GitHub.

### F6. Deterministic process-listing tests

`process::tests::lists_a_child_running_from_a_copied_file` and `yes_stops_the_child_holding_the_file` fail under a loaded `just check` after ~14 s (`panicked at src/process.rs:337`, "no occupant") and pass alone. Done means both tests pass or fail on what they assert, not on scheduling, and neither assertion is weaker than today.

Cause: the sleeper is a copy of `/bin/sleep`. macOS launch constraints SIGKILL a copied platform binary a few milliseconds after exec (`rc=137`, reproduced outside any sandbox), so the test only passes when `lsof` happens to catch the child before it dies. Under load each `lsof -t` scan takes ~280 ms, the first one lands after the kill, and 50 iterations × (scan + 20 ms) is the observed ~14 s. The stop test is vacuous for the same reason: its child is already dead before `offer_to_stop` runs.

Plan (done):

1. `src/process.rs` tests: on macOS spawn a copy of the test binary itself (not a platform binary, so launch constraints do not apply; `std::fs::copy` clones on APFS, so size is free), running a helper test that sleeps only when an env var is set. Linux keeps `/bin/sleep`, Windows keeps `PING.EXE`.
2. `wait_for`: fail at once with the exit status if the child has already exited, and bound the retry by a wall-clock deadline instead of an iteration count. `std::process::Command::spawn` returns after exec, so the first scan is expected to hit.
3. `yes_stops_the_child_holding_the_file`: assert the child is running and is the listed pid before stopping it, and that `wait()` reports it ended by a signal (Unix) / not successfully (Windows) — rather than the always-true "reaped pid is not alive".
4. Verify: `cargo nextest run`, `cargo clippy --all-targets`, `cargo fmt --check`; the two tests repeated under a parallel CPU load; confirm the old code fails that same stress.

Result: all four steps done. Stress (8 parallel copies of each test per round, 5 rounds, 24 extra busy loops on a machine already at load ~130): old tests 20 of 80 failed with the reported `panicked at src/process.rs:337`, new tests 0 of 80. `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo nextest run --locked --all-targets` (572 passed) clean on macOS. Not run locally: the Linux and Windows arms, which change only in structure (same `/bin/sleep` and `PING.EXE` sleepers) and the new exit-status assertions; CI covers them.
### F6. Install ketch with mise

`mise use -g github:listepo/ketch` installs the release tarball through mise's GitHub backend with no change to the assets. Now a documented install path (README, `docs/COMMANDS.md`, `docs/TROUBLESHOOTING.md`), and ketch behaves around a binary mise owns: `self_update::mise_tool_dir` finds the running binary under `<mise data dir>/installs/<tool>` (`MISE_DATA_DIR`, `XDG_DATA_HOME/mise` or `~/.local/share/mise`, `%LOCALAPPDATA%\mise` on Windows). `ketch self upgrade` without a `ketch` package refuses to rewrite that file and points at `mise upgrade` or `ketch self install`. `ketch self uninstall` asks separately whether to run `mise --yes unuse -g <tool>` (`--yes` answers it too) and runs it last; on a decline it names the command. On Windows the running `ketch.exe` is renamed into `%TEMP%` first, since Windows will not delete a directory holding a mapped image. Unit tests for the path detection and the install-dir → tool-name mapping; `tests/mise.rs` runs on every OS with the real binary copied into a sandboxed mise tree and a `rustc`-built fake `mise` that deletes the tool directory while ketch is still running. Verified: `just check` clean, and the real binary against scratch mise dirs, `KETCH_ROOT` and `HOME` — refusal, `self install` from a mise-installed 0.4.7, uninstall with the mise question declined and accepted.
### F7. Clean up target dirs with dunnage after tests

`dunnage` (https://github.com/listepo/dunnage) shrinks cargo `target/` dirs losslessly (APFS compression + copy-on-write dedupe), never deletes, and keeps mtimes so nothing rebuilds. `just dunnage` runs `dunnage run target` (skipped when there is no `target/` yet), treating exit 2 (a build held the lock) as success and printing a one-line install hint instead of failing when the binary is missing. `just test` now runs it as a post-dependency (`test: && dunnage`), so a normal local test run also tidies the checkout's target dirs; `just check` picks it up the same way since it depends on `test`. CI never runs Just's `test` recipe (it calls `cargo nextest run` directly), so the guard's tolerance is for local machines without `dunnage` installed, not for keeping CI green. Installed with `ketch install dunnage` once `pyrlyn/ketch-registry#9` lands; already installed on this machine in the meantime. `toolchain.md` gained `ketch` and `dunnage` in the programs table and a new `ketch` package table. `AGENTS.md` notes the cleanup in one line under Commands.

### B62. A `bin` glob matching several payload files links the helper stub, not the binary

Observed 2026-09-26: `ketch install pyrlyn/rtok` on Windows linked the 390 KB `rtok-hook.exe` dist ships beside the 34 MB `rtok.exe` into `~/.ketch/bin/rtok.exe`, and every `rtok` invocation then hung reading stdin for a hook payload. `resolve_bin_specs` takes the FIRST walkdir match of a glob; the registry entry's `bin = [{ path = "rtok*", name = "rtok" }]` also matches `rtok-hook.exe`, and NTFS lists `rtok-hook.exe` ahead of `rtok.exe`. Unix readdir order happens to keep `rtok` first, so only Windows broke — by luck, not design. No `*`/`?` pattern matches `rtok`+`rtok.exe` while excluding `rtok-hook.exe`, so manifests cannot spell their way out.

Plan: `glob_preferred` in `model.rs` — among a glob's matches, prefer the file whose stem equals the spec's link `name` (case-insensitive, like `glob_match`), falling back to the first match — used by `resolve_bin_specs` on Windows and Unix; `name`-only specs keep the extension-aware discovery they already have. Follow-up once released: rtok's entry can also link the hook client deliberately (`{ path = "rtok-hook*", name = "rtok-hook" }`) so hosts get the fast `rtok-hook` on PATH.

Check: unit tests for `glob_preferred` (stem preference beats directory order, case-insensitive, first-match fallback, empty is None) and one `resolve_bin_specs` test per platform over a payload holding `rtok(.exe)` beside `rtok-hook(.exe)`; `just check` green; e2e — a locally built ketch installs pyrlyn/rtok into a scratch `--root` and `bin/rtok.exe` is the 34 MB binary answering `--version`.

Result: All green 2026-09-26. Unit tests in model.rs and platform/{windows,unix}.rs; the unix test compiles on the unix runners (the module is cfg'd out on Windows). `just check` passed on Windows in full: fmt-check, clippy, 474/474 nextest, lint-commits, lint-shell, dist-check, package (dist tarball built and checksum-verified), lint-cask skipped as macOS-only. E2E: the locally built ketch installed rtok v0.9.0 into a scratch root and `bin/rtok.exe` (34,342,912 bytes) answered `rtok 0.9.0 (c60f3694b)` through the unchanged `rtok*` registry entry.

Status: done 2026-09-26
Model: ZCode / glm-5.3

### R2. One Rust version, from mise.toml

`mise.toml` pins Rust (`rust = { version = "1.98.1", components = "rustfmt,clippy" }`) and is the single source of the Rust version for local work, CI and release builds. This reverses R1's "the Rust toolchain not pinned": the runner's default stable moved under every workflow independently, so a release could ship from a compiler no pull request had tested. ci.yml, verify.yml, sonarcloud.yml, release-plz.yml and bump.yml install Rust with `jdx/mise-action` (pinned to v4.3.0 by SHA, `install_args` limited to what each job needs), and `.github/build-setup.yml` does the same before rust-cache in the release build, then adds the matrix entry's cross targets with `rustup target add` (mise sets `RUSTUP_TOOLCHAIN`, so they land on the pinned toolchain). `release.yml` is regenerated with `just dist-generate`. rustfmt and clippy are requested explicitly because mise installs the minimal rustup profile. The MSRV (`rust-version = "1.86"` in `Cargo.toml`) is unchanged and separate.

Status: done 2026-09-27 (PR #150)

### M11. Bash completion for every command

Today clap_complete generates a static bash script (`ketch completions bash`, installed at `self install` as `share/ketch/completions/ketch`). It knows commands and flags, but not values.

Plan:
1. Test coverage: a test walks `Cli::command()` and asserts every visible subcommand, alias and flag appears in the generated script.
2. Dynamic values: installed package names for `uninstall`, `upgrade`, `pin`, `unpin`, `link`, `unlink`, `info`, `why`, `changelog` and `rollback`, read from the state file; registry names for `install` and `search`, read from the local registry copy. No network in completion. Use clap_complete `CompleteEnv` (`unstable-dynamic`) or a hidden `ketch __complete <kind>`; decide after checking how stable the feature is.
3. Docs: the bash ≥ 4 and bash-completion 2 note for macOS.

Check: bash smoke on Linux and macOS CI (`COMP_WORDS=(ketch un) COMP_CWORD=1` → `uninstall unlink unpin`; `ketch uninstall r<TAB>` → an installed name from a scratch root); `just check`.

Execution plan (as carried out):
1. Decision: a hidden `ketch __complete <installed|registry> [PREFIX]` command, not `CompleteEnv`. clap_complete 4.6's `unstable-dynamic` sits outside semver and its docs say the shell-to-binary protocol may change between releases, while ketch writes its bash script to disk at `self install`; a cargo patch update could break every installed script.
2. New `src/complete.rs`: the shell-agnostic candidate lists (state file, local registry copy, no network) and the bash script: clap_complete's static script plus a wrapper, generated from `Cli::command()`, that asks `ketch __complete` for positional package names. `main.rs` and `extra::write_ketch_docs` both call it.
3. Tests in `src/complete.rs`: every visible subcommand, alias and flag appears in the script; candidate filtering. `tests/`: bash smoke (`ketch un` → `uninstall unlink unpin`; `ketch uninstall r` → an installed fixture name) with `KETCH_ROOT` and `HOME` in a temp dir, skipped when no bash ≥ 4 is found.
4. Docs: `docs/COMMANDS.md` completions section, the bash ≥ 4 + bash-completion 2 note for macOS.

Result: `src/complete.rs` owns the completion scripts and `ketch __complete [--root DIR] <installed|registry> [PREFIX]`, intercepted in `main` before clap parses so no generated script, help or man page lists it. The bash script is clap_complete's plus `_ketch_packages`, registered in its place; the value-taking options it skips come from the clap tree, and names reach the command line only when they are plain (`[A-Za-z0-9._+@-]`), since registry folder names are someone else's input and `compgen -W` would expand them. Tests: unit tests in `src/complete.rs` (every visible subcommand, alias and flag in the script; candidate filtering; `self install` writes the same script); `tests/completion.rs` drives the script through the `bash` on PATH (3.2 on macOS) against a sandbox root. Docs: `docs/COMMANDS.md`.

Status: done 2026-09-30
Model: Claude Code / opus-5.5

### M12. Windows completion: PowerShell `Register-ArgumentCompleter` and doskey macros for cmd

Today clap_complete emits `Register-ArgumentCompleter -Native -CommandName 'ketch'`, and `ketch completions powershell --install` writes it to `Documents\PowerShell\Completions`. PowerShell does not load that directory by itself, so nothing is active until the user dot-sources it.

Plan:
1. PowerShell: a managed block in the CurrentUserAllHosts profile that dot-sources the script, for both PowerShell 7 (`Documents\PowerShell`) and Windows PowerShell 5.1 (`Documents\WindowsPowerShell`). Resolve Documents through the shell, not a fixed path, because OneDrive may redirect it. Use the same managed-block mechanism as the PATH blocks in `src/shell.rs`, so `self uninstall` removes it. Dynamic values come from M11's completer.
2. cmd: cmd.exe has no programmable argument completion, so ship doskey macros. Generate `share/ketch/ketch.doskey` (the macro list is for Ivan to choose; for example `ki=ketch install $*`, `ku=ketch upgrade $*`, `kl=ketch list $*`) and load it through `HKCU\Software\Microsoft\Command Processor\AutoRun` (`doskey /macrofile=<file>`). Append to an existing AutoRun value rather than replace it. Register the value in B66's inventory so `self uninstall` restores the old value.
3. Optional, only if Ivan wants real Tab completion in cmd: a clink Lua script generated from the CLI.

Check: Windows CI: `pwsh -c "TabExpansion2 'ketch ins' 9"` returns `install`; after install, AutoRun contains the doskey line and `ki` expands in a new cmd; after `self uninstall`, the profile block and the AutoRun addition are gone and an earlier AutoRun value is intact; `just check`.

Creator decisions: the macros are exactly `ki=ketch install $*`, `ku=ketch upgrade $*`, `kl=ketch list $*`, `kun=ketch uninstall $*`; no clink (step 3 skipped).

Result: the PowerShell script `ketch completions powershell` prints is clap_complete's with a package lookup spliced into its completer (`src/complete.rs`): the same command table as bash, answered by `ketch __complete`, written to run on Windows PowerShell 5.1 as well as 7. On Windows, `expose_self_docs` (run by `self install`, `self upgrade` and `completions --install`) now also calls `shell::install_powershell_profiles`, which puts a `# >>> ketch >>>` block dot-sourcing `Documents\PowerShell\Completions\ketch.ps1` into both editions' CurrentUserAllHosts profiles (Documents as PowerShell reports it; a new profile only when that edition is installed and its execution policy runs local scripts; a BOM for a new profile naming a non-ASCII path), and `shell::install_cmd_macros`, which writes `<root>\share\ketch\ketch.doskey` and appends ` & doskey /macrofile="…"` to `HKCU\Software\Microsoft\Command Processor\AutoRun`, keeping the value's kind and `%VAR%`s. Registry values reach PowerShell through environment variables. `self uninstall` (also with `--keep-packages`) lists and removes both: `shell::uninstall_powershell_profile` and `shell::uninstall_cmd_macros`, which removes exactly ketch's command and restores the earlier value or deletes the value when it held only ketch's. The AutoRun functions are small and named so B66's registry inventory can fold them in. Tests: AutoRun add/remove/round-trip, block rendering, quoting, BOM and policy as unit tests in `src/shell.rs`; the spliced script in `src/complete.rs`; `tests/completion_windows.rs` runs `TabExpansion2` in `pwsh` and `powershell`, and checks AutoRun, `doskey /macros` in a new cmd and the profiles after install and after `self uninstall`, saving and restoring the real AutoRun and profiles in a drop guard. Docs: `docs/COMMANDS.md`.

Status: done 2026-09-30
Model: Claude Code / opus-5.5

### M10. Man pages in roff for every command

Today `extra::render_manpage` writes one hand-rolled `ketch.1`, listing top-level commands with no options and no nested subcommands. `write_ketch_docs` places it under `share/man/man1/` at `self install`.

Plan:
1. Add `clap_mangen` (rtok already uses it for `rtok man`, a working reference). Generate `ketch.1` plus `ketch-<cmd>.1` for every visible subcommand, recursively (`ketch-config-create.1`, `ketch-self-uninstall.1`, …), with options, defaults, env vars and examples from the clap definitions. Replace `render_manpage`.
2. Write every page through `write_ketch_docs` as `ExtraPath` records, so uninstall and relink remove them with the same ownership proof as today's page.
3. A hidden `ketch man --out <dir>` (or a `just man` recipe) for packaging; the Homebrew cask may ship them.

Check: a test walks `Cli::command()` and asserts one page per visible command; `mandoc -Tlint` clean on macOS and Linux CI; `man ketch-install` works after `self install` in a scratch root; `just check`.

Execution: `src/man.rs` renders every page with `clap_mangen` 0.3 (`env` feature) from `Cli::command()`: `ketch.1`, then `ketch-<cmd>[-<sub>…].1` depth first for each subcommand that is not hidden and not clap's generated `help`. Titles are upper case, the source is `ketch <version>`, the date is `SOURCE_DATE_EPOCH` when set (reproducible packaging) or today. Two clap_mangen layouts that `mandoc -Tlint` warns about (a break beside a blank line before "Possible values") are tidied into one `.sp`. `extra::write_ketch_docs` writes every page under `share/man/man1/` of the store prefix and records each as an `ExtraPath`; `extra::render_manpage` is gone. A hidden `ketch man --out <DIR>` writes the same pages without a ketch root, for packaging. `just lint-man` (part of `just check`) runs `mandoc -Tlint -Wwarning` on them when mandoc is present and says it skipped otherwise; the Linux CI job does not install mandoc.

Tests: `src/man.rs` walks `Cli::command()` and asserts one page per visible command in order, nested pages exist (`ketch-self-uninstall.1`), hidden commands and `help` get none, a subcommand page names its full invocation and options, the tidy step, and `write_to`. `src/extra.rs` asserts every page is written into the prefix and recorded as a man extra. Verified by hand: `mandoc -Tlint -Wwarning` clean on all 42 pages on macOS; `self install` in a scratch root (KETCH_ROOT, HOME and XDG_* under `target/`, a local mock of the GitHub API serving the built binary) linked 42 pages into `$XDG_DATA_HOME/man/man1`, `man ketch-install` rendered, and `self uninstall` removed them all.

### B67. Uninstall deletes the package's install folder

Ivan: uninstall must delete the program's folder (for example the `ketch` folder).

Today `install::uninstall` removes the current prefix and every retained prefix with `remove_store_dir`, which then calls `remove_dir(store/<name>)`. That call only succeeds when the directory is empty, so any leftover keeps `store/<name>/` in place. Leftovers include `<version>.incoming` or `<version>.old` siblings from `move_into_store` (removed best-effort with `let _ = remove_any`), or a file Windows kept locked. For ketch itself, `self_update::remove_root_at` wipes the named directories and then `remove_dir(root)`. The root stays when anything else is in it. On Windows the running `ketch.exe` cannot delete its own image, so `~/.ketch` survives the uninstall. This is the same cause as B60.

Plan:
1. Package uninstall: after the prefixes are gone, remove `store/<name>/` whole (`remove_dir_all`). Guard it with `is_inside_store`, and only for a direct child of the store named exactly like the package. On failure, warn and name the path.
2. One helper sweeps stale `.incoming` / `.old` siblings. B68 reuses it.
3. Self uninstall on Windows: finish the root removal after the process exits, with a detached `cmd /c` that waits for the PID and then removes the root. `unsafe_code = "forbid"` rules out calling the Win32 API directly. The home-directory safety rule in `remove_root_at` stays: a root equal to `$HOME` is never wiped.
4. `ketch doctor` notes a `store/<name>/` that has no state record.

Check: e2e: install, uninstall, then `store/<name>` does not exist, including with a planted `1.0.0.old` sibling; Windows e2e: `self uninstall` leaves no `~/.ketch` after the process exits; `just check`.

Done (Claude Code / opus-5.5): `install::uninstall` removes `store/<name>/` whole through `remove_package_dir` — only a direct child of the store named exactly like the package, inside the store after symlinks resolve — so `.incoming` / `.old` leftovers go with it; prefixes outside that folder still go through `remove_store_dir`. Self uninstall collects what `remove_root_at` could not delete and, on Windows, hands it to a detached PowerShell that waits for the ketch PID, removes only those named paths and then deletes the root non-recursively (a root equal to `$HOME` is never handed on). Step 4 already existed: `ketch doctor`'s `orphans` check names a store folder with no state record. The shared stale-sibling sweep is left for B68, its first user. Tests: unit tests in `install.rs` and `self_update.rs`, e2e `tests/install.rs::uninstall_removes_the_package_folder_with_what_a_failed_swap_left_in_it` and `tests/self_uninstall_root.rs` (all OSes).

Status: done 2026-09-30

### F10. Coloured output: errors red, success green, warnings yellow

Most of this exists. `src/ui.rs` paints the `error` label red, `warning` yellow, success verbs green, steps blue and notes dim. It honours `--no-color`, `NO_COLOR`, `CLICOLOR_FORCE` and non-TTY output, and there is no `println!` outside `ui`. F10 is an audit plus the gaps:

1. Only the label is coloured today. Decide with Ivan whether the error headline and warning text are coloured as well.
2. Route every remaining status string through the helpers: prompts (`confirm`, `select`), `completed`, table markers (M9's yellow `update available`).
3. Windows: make sure ANSI works on legacy conhost (enable VT processing, or fall back to plain text).
4. Optional: a `color = "auto" | "always" | "never"` config key and `KETCH_COLOR`, next to `--no-color`.

Check: insta snapshots of each line kind with `CLICOLOR_FORCE=1`; no escape bytes with `NO_COLOR=1`, `--no-color`, or when piped; `just check`.

Decisions (Ivan): the whole error headline is red and the whole warning text yellow, not only the label; success lines green. Step 4 is skipped.

Execution plan (done):
1. `src/ui.rs`: paint the full error headline red and the full warning text yellow; success lines green.
2. `src/ui.rs` and callers: route `confirm`/`select` prompts, `completed` and table markers through the style helpers.
3. Windows: enable VT processing on legacy conhost through a maintained crate without `unsafe`, or fall back to plain text when it cannot be enabled.
4. Tests beside `ui.rs`: insta snapshots of each line kind with colour forced; no escape bytes with `NO_COLOR`, `--no-color` or a pipe.
5. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`.

Result: `ui::Tone` names the meaning of status text (step, success, warning, error, note, hint) and is the one place a meaning picks its colour. Success lines are green end to end, warnings yellow, error headlines red; details stay dim and the hint cyan. Prompts (`confirm`, `answer`, `choose`, `cancelled`), the `fetched` line, the counter label, `list`'s update marker, `doctor`'s ok/warn/fail and `lock`'s +/~/- markers go through the same helpers; the log-open warning no longer bypasses `ui`. On Windows `console` switches on virtual terminal processing, and colour falls back to plain text when it cannot. `completed` prints nothing outside the TUI, whose status colours already honour `--no-color`, so it was left as is.

### F11. Emoji icons per operation, `emoji` config key (default true)

Plan:
1. One table in `src/ui.rs` maps each operation to an icon (proposal: install 📦, upgrade ⬆️, uninstall 🗑️, download ⬇️, link 🔗, rollback ⏪, search 🔍, doctor 🩺, success ✅, warning ⚠️, error ❌, note ℹ️). Ivan picks the final set.
2. Config: `emoji = true` in `Config` / `Config::default_toml()`, the `KETCH_EMOJI` env var, and a `--no-emoji` global flag if wanted. Document it in the Configuration table in `README.md` and `docs/COMMANDS.md`, and in the `config reset` defaults test.
3. Icons appear only on human-facing status lines going to a terminal. They never appear in `--json`, `--names-only`, `ui::out` data, the log file, or when `TERM=dumb`.
4. Width: emoji are double-width, so pad the verb column with `unicode-width` and keep columns aligned with and without icons.

Check: snapshots with emoji on and off; JSON and piped output contain no emoji; `emoji = false` and `KETCH_EMOJI=0` turn them off; `just check`.

Execution plan (Claude Code / opus-5.5, done), with Ivan's final set: the proposal above as written.

1. `src/ui.rs`: an `EMOJI` switch beside `COLOR`, set by `ui::set_emoji` once the config is loaded; on only when wanted and stderr is a terminal whose `TERM` is not `dumb`. One icon table next to `Tone`: operation icons matched on the verb, then the tone's own icon (success, warning, error, note). The icon is padded to two columns with `unicode-width`, and lines without an icon get the same blank gutter, so every column stays aligned.
2. `src/config.rs`: `emoji` in `ConfigFile`, `Config` and `default_toml()`, `KETCH_EMOJI` through `env_bool`. `src/cli.rs`: global `--no-emoji`. `src/main.rs`: call `ui::set_emoji` after each `Config::load`.
3. Tests: insta snapshots of every line kind with emoji on and off; the resolver as a pure function (config off, `KETCH_EMOJI=0`, flag, pipe, `TERM=dumb`); e2e: piped stderr and `--json` carry no emoji; `config reset` defaults include `emoji = true`.
4. Docs: `README.md` Configuration table, `docs/COMMANDS.md` global flags. `Cargo.toml`: `unicode-width` as a direct dependency (already locked through indicatif), `toolchain.md` and `rust.md` rows.

Result: `src/ui.rs` holds one icon table beside `Tone`: an operation icon picked from the verb (install 📦, upgrade/update ⬆️, uninstall/remove/prune 🗑️, download/fetch ⬇️, link 🔗, rollback ⏪, search 🔍, doctor 🩺), else the tone's icon (success ✅, warning ⚠️, error ❌, note ℹ️); steps, debug lines and prompts get a blank gutter of the same width, measured with `unicode-width` (now a direct dependency), so the verb column stays aligned with and without icons. `emoji` (default `true`) is in `Config` and `default_toml()`, `KETCH_EMOJI` overrides it, and the global `--no-emoji` turns it off. `ui::set_emoji` runs after the config loads and keeps icons off a pipe and `TERM=dumb`; they never touch `ui::out`, tables, `--json`, `--names-only` or the log. Tests: insta snapshots of every line kind with emoji on and off, the resolver cases, icon widths and column alignment, config file and env tests, `config reset` writing `emoji = true`, and an end-to-end install whose piped output, `--json` and log carry no icon. Skipped: a schemars schema for `config.toml`, which the repository does not have for any key yet. `search` and `doctor` print data rather than status lines today, so their icons have no line to appear on until one is added.

### B66. Windows self-uninstall removes the registry entries ketch wrote at install

Ivan: uninstalling ketch on Windows must remove the registry entry that was added at install.

What ketch writes to the registry today: only `HKCU\Environment\Path`. `install.ps1` adds the bin dir there, and so does `ketch path install` (`shell::install_user`, through `[Environment]::SetEnvironmentVariable(..., 'User')`). There is no Apps & Features (`...\CurrentVersion\Uninstall\ketch`) key, because `dist-workspace.toml` sets `installers = []`. `self uninstall` removes the Path entry only when `UninstallPlan.user_path` is true (`shell::user_path_configured(cfg)`). That is false under `--keep-packages`, and it may miss an entry written by `install.ps1 -InstallDir <dir>` for a bin dir that is not `cfg.bin_dir`.

Plan:
1. Reproduce on a Windows runner: `install.ps1` (default and with `-InstallDir`), then `ketch self uninstall --yes`, then read `HKCU\Environment\Path` and list what is left.
2. Keep one inventory of every registry value ketch writes (a function in `src/shell.rs`, for example `registry_entries(cfg)`): the user Path entry today, and M12's `HKCU\Software\Microsoft\Command Processor\AutoRun` addition later. `self uninstall` removes each entry it finds, matching Path entries the way `install.ps1`'s `Normalize-PathKey` does (quotes, slashes, trailing separator, case).
3. `ketch doctor` warns when an inventory entry points into a ketch root that no longer exists.
4. Ask Ivan whether he also expects an Apps & Features entry. That would be new (register at `self install`, remove at uninstall), not a fix.

Check: Windows e2e (`tests/install_windows.rs` or `tests/install_ps1.rs`): after `install.ps1` + `ketch self uninstall --yes`, the user Path holds no entry for the ketch bin dir, with and without `-InstallDir`; `just check` green on all three OSes.

Decisions (Ivan): no Apps & Features entry; only clean up what ketch writes.

Result: `shell::registry_entries(cfg)` is the one inventory of registry values ketch writes (`RegistryEntry::UserPath` today; M12's AutoRun joins it), and `self uninstall` removes each one through `shell::remove_registry_entry`, before the root so the bin dir can still be resolved. A Path entry now matches the bin dir by spelling (case, quotes, slashes, trailing separator — as `install.ps1`'s `Normalize-PathKey`) or, while the folder exists, by resolving both paths, which covers 8.3 short names. `install.ps1 -InstallDir` never puts that dir on the user PATH — it always adds `<root>\bin` — so there was no second entry to chase. `--keep-packages` keeps the entry, like the shell blocks, because the packages left in the bin dir still need it. `ketch doctor` warns about user PATH entries that name a ketch bin dir whose folder is gone. Tests: unit tests for resolution matching, stale detection and the empty inventory off Windows; `tests/install_windows.rs` writes a quoted, upper-case, trailing-backslash entry for the sandbox bin dir and checks `self uninstall --yes` removes it. The Windows parts were verified only in CI.

### M11. Bash completion for every command

Today clap_complete generates a static bash script (`ketch completions bash`, installed at `self install` as `share/ketch/completions/ketch`). It knows commands and flags, but not values.

Plan:
1. Test coverage: a test walks `Cli::command()` and asserts every visible subcommand, alias and flag appears in the generated script.
2. Dynamic values: installed package names for `uninstall`, `upgrade`, `pin`, `unpin`, `link`, `unlink`, `info`, `why`, `changelog` and `rollback`, read from the state file; registry names for `install` and `search`, read from the local registry copy. No network in completion. Use clap_complete `CompleteEnv` (`unstable-dynamic`) or a hidden `ketch __complete <kind>`; decide after checking how stable the feature is.
3. Docs: the bash ≥ 4 and bash-completion 2 note for macOS.

Check: bash smoke on Linux and macOS CI (`COMP_WORDS=(ketch un) COMP_CWORD=1` → `uninstall unlink unpin`; `ketch uninstall r<TAB>` → an installed name from a scratch root); `just check`.

Execution plan (as carried out):
1. Decision: a hidden `ketch __complete <installed|registry> [PREFIX]` command, not `CompleteEnv`. clap_complete 4.6's `unstable-dynamic` sits outside semver and its docs say the shell-to-binary protocol may change between releases, while ketch writes its bash script to disk at `self install`; a cargo patch update could break every installed script.
2. New `src/complete.rs`: the shell-agnostic candidate lists (state file, local registry copy, no network) and the bash script: clap_complete's static script plus a wrapper, generated from `Cli::command()`, that asks `ketch __complete` for positional package names. `main.rs` and `extra::write_ketch_docs` both call it.
3. Tests in `src/complete.rs`: every visible subcommand, alias and flag appears in the script; candidate filtering. `tests/`: bash smoke (`ketch un` → `uninstall unlink unpin`; `ketch uninstall r` → an installed fixture name) with `KETCH_ROOT` and `HOME` in a temp dir, skipped when no bash ≥ 4 is found.
4. Docs: `docs/COMMANDS.md` completions section, the bash ≥ 4 + bash-completion 2 note for macOS.

Result: `src/complete.rs` owns the completion scripts and `ketch __complete [--root DIR] <installed|registry> [PREFIX]`, intercepted in `main` before clap parses so no generated script, help or man page lists it. The bash script is clap_complete's plus `_ketch_packages`, registered in its place; the value-taking options it skips come from the clap tree, and names reach the command line only when they are plain (`[A-Za-z0-9._+@-]`), since registry folder names are someone else's input and `compgen -W` would expand them. Tests: unit tests in `src/complete.rs` (every visible subcommand, alias and flag in the script; candidate filtering; `self install` writes the same script); `tests/completion.rs` drives the script through the `bash` on PATH (3.2 on macOS) against a sandbox root. Docs: `docs/COMMANDS.md`.

Status: done 2026-09-30
Model: Claude Code / opus-5.5


### B68. Update installs into a fresh folder so stale files cannot interfere

Ivan: update must clean or delete the program folder and install into a fresh one.

Today the per-version prefix is already fresh. `move_into_store` stages the payload as `<version>.incoming` and swaps it in through `<version>.old`, so a new version and a `--force` reinstall of the same version both replace the directory whole. Gaps: (a) stale `.incoming` / `.old` siblings survive when their best-effort removal fails; (b) old links and copied files the new version no longer has are removed by `platform.unplace(&stale)`, and a failure there is only a warning; (c) retained prefixes of earlier versions stay on purpose, because `ketch rollback` (M6) needs them.

Plan:
1. Sweep stale siblings (B67's helper) at the start of every install and upgrade, before hooks run.
2. If a stale sibling or a stale link cannot be removed, fail the update before anything is placed, naming the path. Do not warn and continue.
3. Decision for Ivan: "delete the program folder" must not break rollback. Proposal: keep retained prefixes (they are separate directories, so they cannot leak files into the new one) and say so in `docs/COMMANDS.md`. The alternative is to drop retention by default (`retain = 0`).
4. `ketch self upgrade` replaces the binary in place (`replace_binary`). Its leftovers are covered by B60, and nothing more is needed here.

Check: e2e: a file present in 1.0.0 and absent from 1.1.0 is gone after upgrade, and a same-version `--force` reinstall leaves no stale file; a planted `1.1.0.incoming` does not end up inside the new prefix; `just check`.

Done (Claude Code / opus-5.5): Ivan chose to keep retained prefixes (step 3), now stated in `docs/COMMANDS.md` under `ketch upgrade`. `install::commit` runs `sweep_swap_leftovers` before the hooks and before anything is placed: every `*.incoming` / `*.old` entry in `store/<name>/` goes, except a prefix the package still records; one that cannot be removed fails the install or upgrade with its path. `package_dir_candidate` is the guard shared with B67's `remove_package_dir`. Step 2's stale *links* stay a warning: they are only known after the new placement, which is placed first on purpose so a failed placement keeps the working links, and a leftover link points at a retained prefix, never into the new one. Tests: unit tests for the sweep (leftovers go, versions and recorded prefixes stay, a failure names the path), e2e `upgrade_installs_into_a_fresh_prefix_that_nothing_stale_reaches` and `a_forced_reinstall_of_the_same_version_leaves_no_stale_file` (macOS).

### B69. Uninstalling a package that is not installed prints only "not found"

Ivan: uninstalling an already-removed program prints only "program not found".

Today `cmd::pkg::uninstall` resolves every name first and fails with `Error::NotInstalled` (`` `<name>` is not installed ``, exit 4), rendered by `ui::error` with the `error` label.

Plan:
1. A missing name prints one line, `<name>: not found`, with no hint, no detail lines and no summary. Exit code 4 stays for scripts. Wording: ketch's docs say "package"; Ivan's phrase was "program not found". Confirm the final text with Ivan.
2. Several names: every missing one is reported, and nothing is removed (the up-front resolution stays, so a typo still stops the command).
3. After B67: a name with no state record but a leftover `store/<name>/` gets the leftover removed and still reports not found.

Check: e2e: uninstall twice → the second run prints exactly one line and exits 4; trycmd snapshot; `just check`.

Done (Claude Code / opus-5.5): wording confirmed by Ivan as `<name>: not found`. `cmd::pkg::uninstall` collects every name state cannot find, prints one `ui::bare_error` line per name (no label, hint or detail, logged at error level) and returns `Error::Reported(4)`, which `main` exits with without printing anything else — not even the log-path note. No installed package is removed when any name is missing. A missing name's leftover `store/<name>/` is removed through B67's `install::remove_package_dir`. Tests: `tests/uninstall_not_found.rs` (one name, several names, a leftover folder; all OSes).

Status: done 2026-09-30

### M13. JSON Schema for the TOML files ketch owns

`AGENTS.md` requires every config file this project owns to have a schema generated from its types with `schemars`, committed and checked by a drift test. There was none. Requested by the creator in chat.

Done when `config.toml` (`ConfigFile`) and `ketch.lock` (`Lockfile`) each have a committed JSON Schema generated from their types, and a test fails when a committed schema differs from the generated one.

Done (Claude Code / opus-5.5):

1. `schemars` 1.2 as a dev-dependency: only the tests export a schema, so the derives are `cfg_attr(test, ...)` and the binary does not change. Row in `toolchain.md`.
2. Derive `JsonSchema` on `ConfigFile`, `Lockfile`, `LockedPackage`; `PackageRef` is stored as a `scheme:id` string, so its fields use `#[schemars(with = "String")]`.
3. Commit `docs/config.schema.json` and `docs/lock.schema.json`.
4. Drift tests beside the types (the crate has no library target, so `tests/` cannot reach them): `config::assert_schema_current` generates the schema, drops `null` from `type` (TOML has none), and compares with the committed file; `KETCH_BLESS=1 cargo nextest run schema` rewrites it.
5. Checked: `cargo nextest run` (709 passed), `cargo clippy --all-targets -D warnings`, `cargo fmt --check`; a hand edit to a schema fails its test.

Not in scope: the package manifest (`ketch.toml`, `Manifest` in `model.rs`) — noted in `ideas.md`.

Status: done 2026-09-30
Model: Claude Code / opus-5.5

### F9. `ketch install <pkg>` on an installed package offers the update

Ivan: `ketch install <program>` when it is already installed asks "update?". Yes updates. With no update available, it says it cannot install because the package is already installed.

Today `install::prepare` returns `Error::AlreadyInstalled` (exit 5, hint "Use --force to reinstall.") only when the resolved tag equals the installed one. When a newer release exists, `ketch install` upgrades silently.

Plan:
1. Installed and a newer release resolves, with an unversioned spec: ask through `ui::confirm`: `<pkg> <installed> is installed; update to <latest>?`. The default answer is a decision for Ivan; the proposal is No, matching the other confirms. Yes runs the same path as `ketch upgrade <pkg>`, including the update hooks. No exits 0 with a note.
2. Installed and nothing newer: fail with `cannot install <pkg>: <version> is already installed and no update is available`, still exit 5. `--force` still reinstalls; whether its hint stays is a decision for Ivan.
3. `--yes` answers yes. Without a TTY and without `--yes`, fail and name `--yes` / `ketch upgrade`, instead of upgrading silently. This is a behaviour change, so it goes in `CHANGELOG.md`.
4. Pinned packages keep `Error::Pinned`. An explicit version (`pkg@1.2.0`) keeps today's behaviour. In a batch, each installed package is asked separately. `ketch sync` is unaffected.

Check: e2e with the mock release API: newer + yes → upgraded; newer + no → unchanged, exit 0; nothing newer → the message and exit 5; non-TTY without `--yes` → error; `docs/COMMANDS.md` updated; `just check`.

Done (Claude Code / opus-5.5): Ivan chose default No and to keep the `--force` hint. `InstallRequest.offer_update` (set by `ketch install` unless `--yes`) makes `install::prepare` stop an unversioned install of an installed package with `Error::UpdateAvailable` (newer release) or `Error::NoUpdate` (same release), both exit 5, before anything is downloaded; pinned packages and `pkg@version` keep their old errors. `cmd::pkg::install` asks after the parallel batch, one package at a time, and runs the approved ones as a second batch pinned to the exact tag the question named — the same prepare/commit path as `ketch upgrade`, update hooks included. Without a terminal the `UpdateAvailable` error stands, and its hint names `--yes` and `ketch upgrade <pkg>`. `ketch sync` and `self install` are unaffected (they never set the flag). Tests: e2e yes / no (on a pseudo-terminal through BSD `script`, macOS), `--yes`, no terminal, nothing newer; `docs/COMMANDS.md` updated.

Status: done 2026-09-30

### B70. Flaky `upgrade_stops_a_process_holding_the_binary_when_yes`

`tests/auto_update.rs` starts the installed sleeper, sleeps a fixed 400 ms, then runs `ketch upgrade --yes` and expects it to report the process as `in use`. On macOS under load (several cargo builds in parallel) it failed on 2026-09-30 and 2026-10-01 and passed when run alone. Done means the test waits on a condition, not a delay, and survives a stress loop.

Plan (Claude Code / opus-5.5):
1. Reproduce: run the built `auto_update` test binary 64-way in parallel for several rounds.
2. `tests/support/mod.rs`: `Entry::sleeper` creates the file named by `KETCH_TEST_SLEEPER_READY`, when set, before it starts waiting (sh and cmd).
3. `tests/auto_update.rs`: set that variable on the child and wait for the file (bounded at 60 s so a sleeper that never starts fails instead of hanging) in place of the 400 ms sleep.
4. Verify: the same stress loop, then `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`.

Done (Claude Code / opus-5.5): reproduced with the prebuilt test binary run 64-way in parallel — 2 of 192 runs failed with no `in use` line, because the shell had not opened the script within 400 ms, so `lsof` found nobody. After the fix: 0 of 800 runs failed (64- and 96-way), and the full suite passes. Running `cargo nextest` several times at once was no use as a reproducer: concurrent runs relink the binaries and macOS SIGKILLs them.

Status: done 2026-10-01

### F15. Liquid glass design in Figma

The creator chose variant 6, "Liquid glass", from the design explorations as the app's design (2026-10-01), and Figma as the design tool. The Figma file becomes the source the tokens and the screens follow.

Plan:
1. A Figma design file "ketch for macOS — Liquid glass" in the creator's team: a `Color` variable collection with Light and Dark modes (glass, glass-hi, glass-lo, glass-top, glass-sheet, ink, ink-2, hair, edge, rim-hi, rim-lo, drop, scrim, accent, accent-soft, accent-ink, on-accent, status colours), number variables for radius and spacing, text styles (Large title, Title, Headline, Body, Caption, Badge, Mono), effect styles for the glass elevations (drop shadow plus inner rim light and shade, background blur).
2. Components: window chrome, sidebar navigation, package row, app card, shelf, badge, button (primary, glass), segmented control, progress bar, sheet, menu-bar extra.
3. Screens on a Light page and a Dark page, 1180×760: Installed, Discover, Updates, Package detail, Activity, Doctor, Settings (with Appearance), binary-choice sheet, menu-bar extra; prototype links between them.
4. The file link and the variable-to-token mapping go into `desktop/macos/DESIGN.md` and `desktop/macos/design/README.md`.

Check: every colour, radius and text style on the screens is bound to a variable or style; screenshots of both pages reviewed; the mapping table lists every variable.

Done (Claude Code / opus-5.5): file https://www.figma.com/design/v7OJLmQEyCFbJ63uSYpJ9g (Ivan's Starter team). `Color` collection (32 variables, Light and Dark) and `Dimension` collection (13) synced to F16's `tokens.json`, each with its Swift name as iOS code syntax; 7 text styles; 5 effect styles built on Figma's Glass effect; components (icons, app icon, badge, button, nav item, search field, progress, segment, swatch, package row); nine screens on the Light page and their Dark-mode clones; prototype links from the sidebar, rows and Uninstall. Inter and JetBrains Mono stand in for SF Pro and SF Mono, which Figma's cloud renderer does not have. The variable-to-token mapping is `desktop/macos/design/figma.md`.

Status: done 2026-10-01

### F16. Tokens and Appearance settings for Liquid glass

The app moves to F15's palette and gets Appearance settings. Liquid Glass stays the system material (`.glassEffect`), so what the user can change is what the material allows: tint, regular or clear glass, accent, and the strength of the backdrop wash (creator, 2026-10-01).

Plan:
1. `desktop/macos/design/tokens.json`: the Liquid glass palette for light, dark and their increased-contrast variants (values from F15's variables), regenerated with `just design-tokens`; `DESIGN.md` prose updated; `just design-check` passes (drift, WCAG AA, lint).
2. `Theme.swift` reads `Tokens.swift` instead of literals; `project.yml` compiles the generated file.
3. Settings → Appearance: tint (clear plus presets plus custom), glass style (regular, clear), accent (system or a preset), backdrop wash strength; stored in `AppSettings`, applied through one environment value; Reduce Transparency and Increase Contrast still win over the user's choice.
4. Tests: Swift Testing for the settings model (defaults, persistence, accessibility overrides).

Check: `just design-check`, `xcodebuild test` for the app, a screenshot of both appearances.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/197. To meet WCAG AA the accent deepened to #0873e0 (white on #0a84ff is 3.65:1) and the dark wash colours changed; F15's Figma variables were synced to these values.

### F17. Screens rebuilt on the Liquid glass design

The SwiftUI views follow F15's screens: the glass sidebar with counts, an Updates screen, Discover as shelves of app cards, the detail page, Activity, Doctor, the binary-choice sheet and the menu-bar extra. Starts after F15's screens and F16's tokens land.

Plan (stacked on F16's branch `f16-liquid-glass-tokens` until #197 merges):
1. Read the Figma screens (`get_design_context` / `get_screenshot` per frame; node ids and the variable mapping in `desktop/macos/design/figma.md`); colours, radii and spacing come only from `Tokens.swift` through `Theme.swift`, never literals.
2. `ContentView.swift`: glass sidebar with counts (Installed, Discover, Updates, Activity, Doctor) and the root and package summary at its foot; `KetchStore` gains an update count if it lacks one.
3. New `UpdatesView.swift`: available updates with per-row Update and Update all, the busy banner (`Error::Busy`) with Retry, pinned packages held by `ketch.lock`.
4. `InstalledView`, `DiscoverView` (hero plus shelves of app cards), `PackageDetailView`, `ActivityView`, `DoctorView`, the uninstall sheet and `MenuBarContent` restyled to the frames; badges use the status pairs, an update badge `Status.update`.
5. Reduce Transparency and Increase Contrast keep working (the `Glass/Reduced` style, `highContrast` tokens).
6. Tests: Swift Testing for new store logic (update count, busy state); the UI test navigates every sidebar item.

Check: `xcodegen` + `xcodebuild test`, `just design-check`, screenshots of each screen in light and dark compared with the Figma frames.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/200. All nine screens in light and dark. Differs from Figma where the core has no data: no package sizes or changelog headlines, Discover without featured picks or categories, Doctor names each fix as text with no Fix buttons, monogram icons. Not verified: the real menu-bar popover's glass, and screenshots under Reduce Transparency and Increase Contrast.

### B64. Binary name must be an explicit config parameter (bug + fix)

Bug: when several binaries in a release share the same name prefix, selection differs per OS. On macOS and Linux the intended binary is picked (first in the list). On Windows, alphabetical sorting picks `rtok hook` instead of the intended binary, so the wrong CLI runs.

Fix:

1. The config must name the binary that the CLI invokes.
2. On project/config creation this parameter is required.
3. For configs that already exist it stays optional (backward compatible).
4. If it is missing and more than one binary matches the expected name, prompt the user in select mode listing all candidate binaries. Write the chosen binary into the config and use it on subsequent runs.

Decisions (creator, 2026-09-27): a choice made for a registry or inferred (`owner/repo`) package is stored in the package's state record and reused on upgrade and reinstall; the registry manifest keeps updating. A local project `ketch.toml` gets the choice written into the file itself. Without a prompt (no TTY or `--yes`), the binary whose name equals the package name wins (`rtok` over `rtok-hook`, `.exe` ignored, case-insensitive); if that still leaves more than one, it is an error listing the candidates and how to set `bin`.

Execution plan:

1. Reproduce: find where inference picks the binary when `bin` is empty (`discover_executables` + its caller in `src/platform/unix.rs` and `src/platform/windows.rs`) and why Windows differs (executable filter, sort order). Write the failing unit test first.
2. One OS-independent selection function (not duplicated per platform): exact package-name match → remembered choice from state → TTY select prompt through `ui::` → error with candidates.
3. State: an optional field on `InstalledPackage` for the chosen binary, old state files load unchanged (serde default + a state test).
4. Local `ketch.toml`: write the chosen `bin` entry back, leaving the rest of the file byte-for-byte (`toml_edit`), through the module that owns manifest editing.
5. Creation: the `ketch config` wizard (`src/cmd/config.rs::ask_bins`, `src/wizard.rs`) requires a binary name; `Manifest::validate` stays lenient for existing files.
6. Tests: unit tests for the selection order; e2e in `tests/` with a fixture holding `rtok` and `rtok-hook` (non-TTY exact match, non-TTY ambiguity error, remembered choice on upgrade, local file write-back). Docs: `docs/MANIFESTS.md`, `docs/TROUBLESHOOTING.md`.
7. Verify: `just check` clean; run the binary against a `KETCH_ROOT` scratch tree.

Findings: with no `bin`, every OS linked every discovered executable; only the order differed (`rtok-hook.exe` sorts before `rtok.exe`, `rtok` before `rtok-hook`). The Windows symptom of linking the hook as `rtok` was B62's glob fallback. The only file manifest a user installs from today is `~/.ketch/manifests/<name>.toml`, so that is where the choice is written back.

Follow-up decisions (creator, 2026-09-27):

- A choice drops only the losing binaries that share the package name; every other executable in the release is linked as before.
- `ketch.lock` records the choice (new optional field, validated, `docs/LOCKFILE.md` row); `ketch sync` reuses it, so a fresh machine without a TTY does not stop on the ambiguity.
- `ketch install --bin <name>` makes the choice without a TTY and wins over every other rule; it is stored in state like a prompt answer.
- B65 stays with its own owner (creator, 2026-09-27). B64's branch already has `tests/bin_choice.rs` (`rtok` against `rtok-hook`, not gated by OS); B65 builds on it rather than adding a second fixture.
- The same directory-order fallback in `glob_preferred` is recorded in `ideas.md`, not fixed here.

Status: done 2026-09-27, PR https://github.com/pyrlyn/ketch/pull/156.

### B71. Ambiguous bin glob refuses instead of taking directory order

`glob_preferred` (`crates/ketch-core/src/model.rs`, called from `platform/unix.rs` and `platform/windows.rs`) falls back to the first match in directory order when a `bin` glob matches several files and none has the entry's `name` as its stem, or the entry has no `name`. Directory order differs per OS (NTFS against ext4 and APFS), so the linked binary differs per OS: the same bug class as B62 and B64.

Decision (creator, 2026-10-01): refuse in that case. The error lists the candidate files, sorted so the message is the same on every OS, and says how to resolve it: set the `bin` entry's `name` or `path` in the manifest. `ketch install --bin` is not offered: it already refuses when the manifest names its binaries. A single match, or a stem equal to `name` (case-insensitive, `.exe` ignored), behaves as before. A glob matching nothing keeps its existing error. This is breaking: an install that used to link something now refuses, so the commit is `fix!:`.

Execution plan:

1. Add the failing unit test beside `glob_preferred_picks_the_stem_named_match_over_directory_order`.
2. Make `glob_preferred` return `Result<Option<&Path>>`, one OS-independent function; update both platform callers.
3. Update `docs/MANIFESTS.md` where `bin` globs are described.
4. Verify with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, and a run of the binary against a scratch `KETCH_ROOT` if it can be done offline.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/202. Unit and end-to-end tests in `tests/bin_choice.rs`.

### M9. `ketch list` refactor: `local`, `remote`, and both by default

Today `ketch list` (`cmd/query.rs:40`) prints only installed packages from the state file (package, version with `(pinned)` / `(+N retained)`, source), with `--json` and `--names-only`. The registry is visible only through `ketch search`, and newer versions only through `ketch outdated`.

New syntax: `ketch list [local|remote] [--json] [--names-only]`.

1. `ketch list local`: installed packages only, from the state file, no network. Columns `package`, `installed`, `source`, keeping the `(pinned)` and `(+N retained)` notes. This is today's `ketch list` output.
2. `ketch list remote`: packages in the registry that can be installed. Columns `package`, `latest`, `description` (trimmed to the terminal width). Needs the network for `latest`.
3. `ketch list` with no argument: every package, installed and available, sorted by name, one table with columns `package`, `installed`, `latest`, `source`:
   - Installed packages are marked: a `●` in the first column and bold name on a TTY (plain `*` without colour), with both versions. When `latest` is newer than `installed`, the row says `update available` (yellow on a TTY) and a footer prints `N updates available: ketch upgrade <names>`. A pinned package shows `(pinned)` and is not offered as an update.
   - Packages not installed show only `latest`, with `installed` empty.
   - Installed packages that are not in the registry (installed from `owner/repo` or a local config) are still listed, with `latest` from their own source.

Where `latest` comes from: the registry manifests (`manifest.rs`) carry no version, so `latest` is the newest release of each package's source, from the same lookup `ketch outdated` uses (`cmd/query.rs:87`, prerelease rules from `resolve::list_opts`). Requests run in parallel with a small limit, results are cached with a short TTL (reuse the existing cache directory), and a rate-limited or unreachable package shows `?` in `latest` with a one-line note under the table instead of failing the whole list. `ketch list` with no network prints the local part plus `latest: offline` and exits 0; `ketch list remote` with no network is an error. A spinner or progress bar (`N/M packages`) runs while versions load, per F8.

Output: a compact table through the existing `ui::table`, one row per package, no blank lines; `ketch list local` with nothing installed prints `nothing installed`; `ketch list remote` with an empty registry prints `registry is empty; run ketch update`.

`--json`:
- `local`: an array of `{"name","installed","pinned","retained","source"}`.
- `remote`: an array of `{"name","latest","description","source"}`.
- no argument: `{"packages":[{"name","installed":null|"x.y.z","latest":null|"x.y.z","update_available":bool,"pinned":bool,"source"}],"unreachable":["name"]}`.
- `--names-only` prints names only, for each of the three modes.

Compatibility: `ketch list` without an argument changes from installed-only to everything, and its JSON shape changes. Scripts should use `ketch list local`; note it in `CHANGELOG.md` and the docs as a breaking change, and keep `ketch list --installed` as a hidden alias of `local` for one release.

Documentation (required; the task is not done without it):
- Update the `ketch list` section of `docs/COMMANDS.md`, written so a user understands it without reading the code:
  - the three modes (`local`, `remote`, no argument), what each shows and whether it needs the network;
  - every column (`package`, `installed`, `latest`, `source`, `description`) and the markers (`●` / `*`, bold, `update available`, `(pinned)`, `(+N retained)`, `?`);
  - how `update available` is decided: `latest` is the newest release of the package's source from the same lookup as `ketch outdated`, compared with the installed version, prerelease rules as in `resolve::list_opts`, never for pinned packages;
  - pinned packages: listed with both versions, marked `(pinned)`, not offered as an update, not in the footer;
  - packages installed from outside the registry (`owner/repo`, local config): listed, with `latest` from their own source;
  - offline behaviour: `ketch list` shows the local part and `latest: offline`, `ketch list remote` errors; unreachable packages show `?` and are named under the table; the cache and its TTL;
  - `--json` for each mode, with the full field list, and `--names-only`;
  - the breaking change from installed-only to everything, `ketch list local` for scripts, and the hidden `--installed` alias kept for one release.
- Each mode gets a command example with real output copied from a run (not invented), including one row with `update available` and one pinned row.
- Links: README's command overview links the section (`[ketch list](docs/COMMANDS.md#ketch-list)`); `docs/TROUBLESHOOTING.md` gets an entry for `?` / `latest: offline`; `CHANGELOG.md` names the breaking change and links the section. The landing site picks the docs up through the existing docs sync (not edited by this task).

Execution plan:

1. CLI: `ListMode { Local, Remote }` positional in `src/cli.rs`, hidden `--installed` alias of `local`; body stays thin in `src/cmd/query.rs`.
2. Merge logic (state + registry tiers + per-package `latest`) in its own module with the unit tests listed below; `latest` reuses the `ketch outdated` lookup and `resolve::list_opts`, parallel with a small limit, cached with a short TTL in the existing cache directory.
3. Output through `ui::table` and the existing `indicatif` progress in `src/ui.rs` (`N/M packages`); F8 is not a blocker, it generalises the same helper later.
4. `--json` and `--names-only` for all three modes; offline and unreachable handling as specified.
5. Tests as listed, snapshots with `insta`/`trycmd`, colour off; `docs/COMMANDS.md`, README link, `docs/TROUBLESHOOTING.md`, breaking-change commit (`feat!:`), examples copied from a real run against a scratch `KETCH_ROOT`.
6. Verify: `just check` clean.


Tests (required; all must pass in `just check` and CI on macOS, Linux and Windows):
- Unit:
  - merging state and registry: installed only, available only, both, installed but not in the registry, pinned;
  - `update_available`: newer, equal, older, prerelease versus stable per `list_opts`, pinned always false.
- Integration with a fake registry and a mock release API (the existing test HTTP fixtures), with colour off so snapshots are stable:
  - table snapshots of `ketch list local`, `ketch list remote` and `ketch list`;
  - `--json` snapshots of all three modes, checked against the documented fields;
  - bare `ketch list` marks installed packages (`*` without colour, `●` and bold with colour forced on) and shows both `installed` and `latest` for them, and only `latest` for the rest;
  - `update available` and the footer appear only for installed, unpinned packages with a newer `latest`;
  - a pinned package with a newer `latest` shows `(pinned)` and no `update available`;
  - a package installed from `owner/repo` or a local config, not in the registry, is listed with `latest` from its own source;
  - offline: bare `ketch list` prints the local part and `latest: offline` and exits 0; `ketch list remote` exits non-zero with a clear message;
  - one unreachable package: its `latest` is `?`, the note under the table names it, it appears in `unreachable` in JSON, and every other row is still printed;
  - `--names-only` in each of the three modes;
  - `ketch list --installed` gives the same output as `ketch list local`;
  - empty cases: `nothing installed` for `local`, `registry is empty; run ketch update` for `remote`.

Status: done 2026-09-27, PR https://github.com/pyrlyn/ketch/pull/152. Implemented with unit, snapshot and end-to-end tests.

### R4. Fuzz testing with cargo-fuzz / libFuzzer

Plan:
1. Library target: ketch is binary-only (no `src/lib.rs`), so the fuzz crate cannot reach the parsers. Add `src/lib.rs` holding the modules, and make `main.rs` a thin caller. Keep `unsafe_code = "forbid"`. Alternatively, a `#[cfg(fuzzing)]` `fuzzing` module with entry points, as rtok does.
2. `fuzz/`: a standalone cargo-fuzz workspace excluded from the root one (`cargo-fuzz = true`, `libfuzzer-sys` 0.4, `arbitrary`), the same shape as rtok's `test/cargo-fuzz` branch.
3. Targets: `cli_argv` (`Cli::try_parse_from` over arbitrary argv); `package_spec` (`PackageSpec::parse`); `manifest_toml` (`manifest::parse_registry` + `Manifest::validate`); `lockfile` (parse + validate); `state` (state file load from bytes); `checksum_file` (`source::github::parse_checksum_file`, `parse_digest`); `archive_extract` (tar.gz, tar.bz2, tar.xz and zip through `extract` into a temp dir, asserting every written path stays under the root through `safe_member_path`); `extra_paths` (classification in `src/extra.rs`); `plugin_protocol` (the `ketch-source-*` JSON in `src/source/plugin.rs`); `hook_line` (Windows hook-line quoting, `src/hooks.rs`); `printable` (`ui::printable` filtering). Seed corpora come from `tests/fixtures`.
4. Verify: `cargo +nightly fuzz build` for every target, then a short run of each (`cargo +nightly fuzz run <target> -- -max_total_time=60`). Every crash becomes a minimized regression test in `tests/`, with the fix in its own PR.
5. Optional CI: a non-required nightly job (build plus a short run) on Linux. Do not touch `dependabot.yml` or `sync-docs.yml`.
6. Deliver as a PR; do not merge it.

Execution plan (Claude Code / opus-5.5):
1. `src/lib.rs` compiled only under `cfg(fuzzing)` (empty crate on stable), re-declaring the same modules as `main.rs` plus a `fuzzing` module of entry points; `main.rs` is not touched. Private items the targets need get `#[cfg(fuzzing)]` wrappers in their own module. `unexpected_cfgs` learns `cfg(fuzzing)` in `Cargo.toml`.
2. `fuzz/` with its own `[workspace]`, one `fuzz_targets/<target>.rs` per target above, `fuzz/seed.sh` building seed corpora from `tests/fixtures`, `ketch.toml`, `src/builtin.toml` and archives it makes on the fly.
3. `cargo-fuzz` pinned in `mise.toml`; nightly stays a rustup toolchain used only by `cargo +nightly fuzz`. `just fuzz` recipe, rows in `toolchain.md` and `rust.md`.
4. Verify: stable `cargo fmt`/`clippy`/`nextest` clean, `cargo +nightly fuzz build`, 60 s per target; any crash gets a regression test in `tests/`, not a fix.

Done in the pull request: all eleven targets build and ran 60 s each without a crash in ketch. The optional nightly CI job is not added.

Check: `cargo +nightly fuzz build` succeeds for all targets; each target runs 60 s with no crash (or the crash is filed with a repro test); stable `just check` does not compile `fuzz/`.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/177. All eleven targets build and ran 60 s each without a crash in ketch. The optional nightly CI job was not added.

### R7. Decisions out of the pipeline

`ui::confirm`, `ui::prompt` and `ui::prompt_required` read the terminal from inside the pipeline (`install.rs` calls `confirm`). A GUI has no stdin. Each decision becomes an up-front option (`yes`, the chosen binary, …) or a `Decider` the frontend implements. The CLI keeps its current prompts and non-TTY behaviour.

Done when the core never reads stdin, and a unit test drives each decision through a scripted `Decider`.

Correction after surveying the code: every `confirm`/`prompt` call already sits in `cmd/` (`pkg.rs`, `system.rs`, `lock.rs`, `registry.rs`, `config.rs`). The one decision inside the pipeline is the binary choice: `install.rs` (around line 466) calls `ui::select` when `InstallRequest::interactive` is set. So this task is small, and depends on B64 (which reshapes that choice) being merged.

Plan:
1. Core: `pub trait Decider: Send + Sync { fn choose_binary(&self, package: &str, candidates: &[String]) -> Option<usize>; }` plus a `NoDecider` (always `None`, today's non-interactive path). `InstallRequest::interactive: bool` is replaced by the decider in the context from R6; `--yes` and non-person installs pass `NoDecider`.
2. Binary: a `TerminalDecider` in `ui.rs` wrapping today's `ui::select` (TTY checks and TUI pause unchanged).
3. Confirmations that stay in `cmd/` stay there: they are frontend decisions made before calling the core, which is what a GUI does with its own dialogs. Document that rule in AGENTS.md next to "keep `cmd/` thin".
4. Audit that nothing in the core reads stdin (`grep` for `stdin()`, `read_line`, `IsTerminal` outside `ui.rs`/`tui/`).

Check: unit tests with a scripted decider (picks the second candidate; declines → the existing ambiguity error); the B64 end-to-end tests pass unchanged.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/204. `decide::Decider` (`choose_binary`, `stop_processes`) sits in `Ctx`; `Reporter::choose`/`offer` and `InstallRequest::interactive` are gone. `process::offer_to_stop`, the other mid-run question, went through the same trait. `ui::ctx_asking` opts the CLI in (install, upgrade, rollback, link, self upgrade; off under `--yes`). Nothing in `crates/` reads stdin.

### R9. `ketch-ffi`: the core exported through UniFFI

A `ketch-ffi` crate in the workspace wraps `ketch-core` with UniFFI (proc-macro mode, `uniffi::setup_scaffolding!()`). The generated scaffolding is `extern "C"`, so this crate alone relaxes `unsafe_code` from `forbid` to `deny` with the generated module allowed, and says why in its `//!` header; `ketch-core` and `ketch` keep `forbid`. The surface is coarse: operations (list, search, install, upgrade, uninstall, changelog, doctor), plain records for results, a callback interface for R6's reporter and R7's decider, a typed error enum, cancellation. It builds an XCFramework for both macOS architectures, generates Swift bindings, and has a Swift test that runs one operation against a scratch `KETCH_ROOT`. The binding stays language-neutral so the Windows front end can reuse it later.

Plan:
1. Crate `crates/ketch-ffi`: `crate-type = ["lib", "staticlib"]`, `publish = false`, `dist = false`; `uniffi` at the latest version at start (0.32.2 on 2026-09-30), added to `toolchain.md` and `rust.md`. Check first whether `unsafe_code = "deny"` plus the generated code compiles, or whether the lint must be `allow` for this crate; record the answer in the header.
2. API, one object: `KetchCore::new(root: Option<String>)` builds `Config` per call (R8). Methods (sync; Swift calls them off the main actor): `installed() -> Vec<InstalledPackage>`, `search(query) -> Vec<RegistryPackage>`, `outdated() -> Vec<Upgrade>`, `install(spec, options)`, `upgrade(names)`, `uninstall(names)`, `changelog(name, from, to) -> String` (already sanitized by `changelog::sanitize`), `doctor() -> Vec<Finding>`. Records are FFI-only mirror types converted from `model.rs`, so the core keeps no UniFFI attributes.
3. Callbacks: `#[uniffi::export(callback_interface)]` `Reporter { fn event(e: Event) }` and `Decider { fn choose_binary(package, candidates) -> Option<u32> }`, bridged to the R6/R7 traits; a `CancelToken` object wrapping R8's token.
4. Errors: `#[derive(uniffi::Error)] enum KetchError { Busy { pid }, Cancelled, NotFound { name }, Network { message }, Verification { message }, Other { message } }` mapped from `crate::error::Error`.
5. Build script `scripts/xcframework.sh` (`just xcframework`): `cargo build --release -p ketch-ffi` for `aarch64-apple-darwin` and `x86_64-apple-darwin` with `MACOSX_DEPLOYMENT_TARGET=26.0`, `lipo` into one static lib, `uniffi-bindgen generate --library … --language swift`, `xcodebuild -create-xcframework`, output into a local Swift package `desktop/macos/KetchCore/` (Package.swift, `platforms: [.macOS(.v26)]`). Generated sources and the XCFramework are build output, gitignored.
6. Rust tests for the conversions and error mapping; a Swift test (`swift test` in the package) that installs a fixture package from the local source into a scratch root, with a recording reporter.
7. CI: a macOS job building the XCFramework and running `swift test`; not part of the CLI release.

Check: `just xcframework` builds on a clean checkout; `swift test` passes; `cargo clippy --workspace --all-targets` clean; `grep unsafe crates/ketch-core src` still empty.

Execution plan (Claude Code / opus-5.5), after surveying the merged R6/R7/R8 code:

1. `crates/ketch-ffi` as above, with uniffi 0.32.2 (latest on crates.io, 2026-09-23). It inherits `[workspace.lints]` with `unsafe_code` still `forbid`: UniFFI's generated `unsafe` comes from external-macro expansions, which rustc does not lint, so no relaxation is needed (the reason is in its `//!` header); the `uniffi-bindgen` binary sits behind a `bindgen` feature so the static library does not compile the generator.
2. `ketch doctor`'s checks live in the binary (`src/cmd/system.rs`), out of an FFI's reach: move the check gathering into a core `doctor` module, unchanged; the command keeps `--fix` and the rendering. A `TaskId::get` accessor in `report.rs` so events cross the boundary with their ids.
3. `KetchCore` methods build `Config` and call `log::init` per operation, take `state::Lock` for the mutating ones, and compose existing core calls (`listing`, `Resolver::search`, `install::batch`/`uninstall`/`latest_release`, `changelog`); no install logic in the FFI crate. Each mutating call takes an optional `CancelToken` that is passed into every `InstallRequest`.
4. Reporter and decider are callback interfaces handed to the constructor; Rust adapters implement the core traits.
5. `scripts/xcframework.sh` + `just xcframework` + `just ffi-test`: both Darwin targets (`rustup target add` for the missing one), `lipo`, bindings from the arm64 library, `xcodebuild -create-xcframework` into `desktop/macos/KetchCore/` (gitignored output, committed `Package.swift` and Swift test). The Swift test installs a `local:` fixture into a scratch root.
6. CI: one macOS job (`ketch-ffi`) running what `just ffi-test` runs, on every gate run: almost any core change can change the bindings, and a path filter would need another action.

Creator decisions (2026-10-01, from R10's `docs/research-desktop-windows-linux.md`):

- Keep UniFFI 0.32.x, the latest. Whether the Windows front end gets C# bindings from a third-party generator or another route is decided later, with that front end.
- `ketch-ffi` also builds as a `cdylib` beside the `staticlib`, so a later Windows front end can load `ketch_ffi.dll`. The XCFramework still wraps the static library.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/208. Surface: `KetchCore` (installed, search, outdated, install, upgrade, uninstall, changelog, doctor), records, `Reporter`/`Decider` callbacks, `CancelToken`, `KetchError`. `changelog` returns a structured `Changelog` for one version rather than a from–to string; the SwiftUI adapter maps it in F12.

### R10. Toolkit choice for the Windows and Linux desktop apps

`ROADMAP.md` ("Native desktop apps for Windows and Linux", approved 2026-09-30) gives each OS a native UI over the same core: Windows reuses R9's UniFFI binding from a native front end, Linux may link `ketch-core` directly from a Rust toolkit native to the desktop. It says the toolkit choice is its own research task, with sources; the creator asked on 2026-10-01 to take it now. This task is that research only: the apps themselves stay on the roadmap, and no implementation task is added here.

Scope: Windows — WinUI 3 / Windows App SDK (C# over UniFFI or a C ABI), WPF, and Rust options (windows-rs with WinUI, Slint, iced/egui as non-native contrast). Linux — GTK4 + libadwaita (gtk4-rs, relm4) linking `ketch-core` directly, Qt (cxx-qt), Slint, COSMIC/iced. For each: maintenance (latest release and date from the registry or the repository's releases), licence fit with ketch's triple licence, native look and accessibility, packaging (MSIX/winget, Flatpak), how it consumes the core (UniFFI binding or direct Rust), fit with the core's threading rules (Lock, Cancel, per-operation Config), effort.

Done when `docs/research-desktop.md` (or a page it links) holds the comparison with a primary source and a version or check date on every fact, secondary-only facts marked **unverified**, a recommendation per OS and the open decisions for the creator.

Plan:
1. Read `docs/research-desktop.md`, R9 and the licences; extend that research in the same style instead of contradicting it.
2. Collect facts from primary sources only: crates.io, NuGet and GitHub releases APIs, vendor docs (Microsoft Learn, GNOME, Flathub, Qt, Slint, UniFFI and its C# generator), checked 2026-10-01.
3. Write the comparison, per-OS recommendations and open decisions; decide whether it lives in `docs/research-desktop.md` or a linked page and say why.
4. Close: readiness 90%, a `Status:` line with the PR link and the recommendation; PR against `main`, CI checked once.

Check: every table row cites a URL plus a version or date; `just check` (or the docs-relevant part of CI) passes on the PR.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/205. Research in `docs/research-desktop-windows-linux.md` (linked from `docs/research-desktop.md`). Recommendation: WinUI 3 in C# over R9's UniFFI binding (via `uniffi-bindgen-cs`, unpackaged through winget) for Windows, with `windows-reactor` as the Rust alternative to spike, and GTK 4 + libadwaita through gtk4-rs linking `ketch-core` directly for Linux; the open decisions are listed at the end of the research page.

### M14. JSON Schema for the package manifest

`ketch.toml` (`Manifest` in `src/model.rs`) is the third TOML file ketch owns, after `config.toml` and `ketch.lock` (M13). `AGENTS.md` requires a schema for it too. Its nested types and custom (de)serializers (`PackageRef` as `scheme:id`, the `trust` and `hooks` tables) make it larger than M13.

Done when `docs/manifest.schema.json` is generated from `Manifest` with `config::assert_schema_current`, committed, checked by a drift test, and linked from `docs/MANIFESTS.md`. Every field a registry `ketch.toml` in `pyrlyn/ketch-registry` uses validates against it.

Plan:
1. `model.rs`: `cfg_attr(test, derive(schemars::JsonSchema))` on `Manifest` and every type it holds, the way M13 did `ConfigFile` and `Lockfile`. `PackageRef` is described as the string its `TryFrom<String>` accepts (a `pattern` for `scheme:id` or anything with a `/`), `ExtraPath` as the untagged string-or-table it is, `trust` and `hooks` as the closed tables `deny_unknown_fields` makes them. `name` is left out of `required`, because a registry package folder supplies it.
2. `docs/manifest.schema.json` from `config::assert_schema_current::<Manifest>`, with a drift test beside the M13 ones.
3. A test that validates the root `ketch.toml`, every `[[package]]` in `builtin.toml` and the examples in `docs/MANIFESTS.md` against the committed schema, with the `jsonschema` crate as a dev-dependency (maintained, draft 2020-12, no network with default features off); plus cases the deserializer rejects, so the schema is not looser than the reader where it can say so.
4. `docs/MANIFESTS.md`: link the schema and show the taplo `#:schema` directive; the root `ketch.toml` carries it. `toolchain.md` row for `jsonschema`.
5. Check against the live `pyrlyn/ketch-registry` files; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/206. Schema generated and drift-tested; the root `ketch.toml`, `builtin.toml`, the docs examples and all six `pyrlyn/ketch-registry` files validate; a property test holds each pattern to its Rust check. Found on the way: `bin` entries and `[asset]` lack `deny_unknown_fields`, so a misspelt key there is still ignored (the schema follows ketch).

### M15. `log_level` and `log_format` as enums in `config.toml`

`ConfigFile` holds both as free strings and checks them at load time, so the schema cannot list the allowed values. `AGENTS.md`: constraints live in the types.

Done when `ConfigFile` uses `crate::log::Level` and `crate::log::Format` (serde, lower case) directly, a bad value still fails with an error naming the file and the key, `docs/config.schema.json` lists the values, and the environment variables keep working as before.


Execution plan: give `Level` and `Format` in `crates/ketch-core/src/log.rs` serde (lower case) and `schemars` derives; switch `ConfigFile.log_level` and `log_format` in `crates/ketch-core/src/config.rs` to `Option<Level>` and `Option<Format>`; keep the environment variables parsed by `FromStr` as before; make the TOML parse error name the file and the key; regenerate `docs/config.schema.json` with the existing schema export; add tests for a bad value, each environment variable, the schema enum and unchanged loading of valid files; then run fmt, clippy, nextest and `ketch doctor` against a scratch root.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/203.

### R11. Desktop apps on macOS, Windows and Linux: capabilities, shared layer and per-platform interfaces

The creator asked (2026-10-01) for research on the desktop app for Windows and Linux, macOS included: what the app can do, what is common and written once versus what each platform does its own way, the common interface and each platform's interface, and tasks for each platform. Decided by the creator the same day: Windows is C# + WinUI 3 (Windows App SDK) over `ketch-ffi` (UniFFI, kept at 0.32 for now); Linux is written in Vala, with the toolkit (GTK or Qt/KDE) and the UI markup to be chosen by this research.

Done when `docs/research-desktop-platforms.md` (linked from `docs/research-desktop.md` and `docs/research-desktop-windows-linux.md`) holds a capability matrix per platform with the native API and its maturity, the common/specific split, the common core and app contract (with the gaps R9 left), the common UI and each platform's departures from it per its HIG, the way forward for the C# binding gap and for Vala reaching the core, a recommendation and the open decisions; every fact with a primary source and a version or check date, secondary-only facts marked **unverified**. The resulting tasks are in `plan.md` as `todo`, mirrored in `todo.md`.

Execution plan (Claude Code / opus-5.5):
1. Read the repository facts: R9's exported surface (`crates/ketch-ffi`), the macOS app's `KetchCoreProtocol.swift` and views, the design pipeline (`desktop/macos/design/`), R10's page; list the contract gaps between the app and `ketch-ffi`.
2. Primary sources, checked 2026-10-01: Apple, Microsoft Learn and GNOME/KDE developer docs and HIGs; crates.io, NuGet and GitHub/GitLab release APIs for UniFFI, `uniffi-bindgen-cs`, cbindgen, Vala, GTK, libadwaita, Blueprint, Qt binding projects for Vala, third-party UniFFI generators.
3. Write the page: matrix, common vs specific, interfaces, the C#/UniFFI version gap and the Vala/C ABI route, recommendation, open decisions.
4. Tasks: new `D` ids (desktop), shared first, then macOS (referencing F12/F13/F14 rather than repeating them), Windows, Linux; rows appended to the table, cards appended at the end, `todo.md` in sync.
5. Close at 90% with a `Status:` line; PR against `main`, CI watched until green.

Status: done 2026-10-01, PR https://github.com/pyrlyn/ketch/pull/211. The creator's answers to all ten open decisions are recorded in docs/research-desktop-platforms.md; D6 moved to ideas.md (no localisation for now).

### D1. `ketch-ffi`: foreign traits and per-operation callbacks

UniFFI calls callback interfaces "(soft) deprecated" in favour of foreign traits (new in 0.32), and `KetchCore::new` takes the `Reporter` and `Decider` once, while every app wants them per operation so two screens can each watch their own work. Research: `docs/research-desktop-platforms.md`, section 3a, gap G1.

Done when `Reporter` and `Decider` are foreign traits, `install`, `upgrade` and `uninstall` take a reporter, a decider and a `CancelToken` per call, the constructor no longer takes them, the Swift binding test covers a per-call reporter and `stop_processes`, and `crates/ketch-ffi`'s docs say why. A breaking change to the binding, marked as such.

Execution plan (Claude Code / opus-5.5):

1. `crates/ketch-ffi/src/callbacks.rs`: `Reporter` and `Decider` become `#[uniffi::export(foreign)]` traits, held as `Arc<dyn …>`; the adapters onto the core traits stay.
2. `crates/ketch-ffi/src/lib.rs`: `KetchCore::new(root)` only. `install`, `upgrade` and `uninstall` take `reporter`, `decider` and `cancel`, each optional; `uninstall` checks the token before each package. `search`, `outdated`, `changelog` and `doctor` take an optional `reporter`, so their warnings still reach the caller; `root` and `installed` read files and take none. The crate header says why the callbacks are per call.
3. Rust unit tests: two installs on one core, each reporting to its own reporter only; a cancelled uninstall removes nothing.
4. `desktop/macos/KetchCore/Tests/KetchCoreTests/KetchCoreTests.swift`: the existing cases on the new signatures, plus an upgrade through a `ketch-source-test` shell plugin (1.0 then 1.1, a looping shell script as the asset) while the installed binary runs, so a Swift decider receives the holder and declines.
5. Verify: `cargo nextest run`, `cargo clippy --all-targets`, `cargo fmt --check`, `just ffi-test`. Commit as `feat(ffi)!:`.

Status: done. Read-only calls (`search`, `outdated`, `changelog`, `doctor`) also take an optional reporter, so their warnings keep reaching the caller; `uninstall` checks the token before each package (progress and the leftover store folder stay with D2). On macOS the Swift test runs a looping shell script as the held binary: a relocated `/bin/sleep` is killed by the OS.

### D9. macOS: VoiceOver pass

The macOS app's accessibility was only checked on the fake core. F14 keeps its own Accessibility Inspector pass on the real glass; this task covers labels. Localisation is not needed for now (creator, 2026-10-01, open decision 7), so no String Catalog. Research: section 1.

Done when every icon-only control has an accessibility label, and a VoiceOver walk through the nine screens finds no unlabeled control.

Execution plan:

1. Read every view for symbol-only controls and bare text fields; the existing icon-only buttons already name themselves through their title.
2. `KetchUITests`: one test visits the nine screens (Installed, Discover, Updates, Activity, Doctor, General and Appearance settings, package detail, uninstall sheet) and runs `performAccessibilityAudit(for: [.sufficientElementDescription, .elementDetection])`, ignoring layout containers; it also looks up by label the controls a quiet screen does not show (colour wells, the running operation's Cancel button, the menu-bar extra).
3. Fix what it finds: the search field, the colour wells, the wash slider, the menu-bar label.
4. Verify: `just macos-test`.

Status: done. No manual VoiceOver walk was possible, so the automated audit stands in for it; it finds elements with no description but cannot judge whether a label reads well. Fixed: the search field (its prompt was only a placeholder, now labelled, glyph hidden), the two colour wells (labelled "Custom tint" and "Custom accent" instead of both "Custom"), the wash slider (label and percentage value), and the menu-bar extra ("Ketch, 2 updates available"). The symbol-only Clear, Dismiss and Cancel buttons already carry their titles; Cancel is checked in the test, Clear (needs typed text, which XCUITest could not enter here) and Dismiss (needs a held lock) are not. The Touch Bar, layout containers and a slider thumb are excluded from the audit.

### D2. `ketch-ffi`: records and operations the apps need

The macOS app's protocol needs things `ketch-ffi` does not give: a changelog across a version range, pinned packages in `outdated` with what holds them, `latest` in search results, and an `uninstall` that can be cancelled, reports progress and removes a leftover store folder for a name with no record, as the CLI does. Research: section 3a, gaps G2–G5.

Done when `changelog_range(package, from, to)`, `Upgrade.pinned` (and the lock that holds it, when known), `RegistryPackage.latest` and the new `uninstall` exist with unit tests, the Swift binding test exercises each, and D4's fixtures can model them.

Execution plan (Claude Code / opus-5.5):

1. `ketch-core::changelog`: a `published_range` beside `published`, sharing its manifest resolution, plus a pure `between` that keeps the releases newer than `from` and no newer than `to`, newest first, drafts and unasked-for prereleases out. Unit tests on `between`.
2. `ketch-core::listing`: `fill_cached`, which answers `latest` from the listing cache only — no network — so search can show it.
3. `ketch-ffi`: `changelog_range` (`from` defaults to the installed version, `to` to the newest); `Upgrade.pinned` and `Upgrade.held_by` (ketch does not record which `ketch.lock` restored a pin, so `held_by` stays `None` until it does), `outdated` reports pinned packages and `upgrade` still skips them; `RegistryPackage.latest` from the cache; `uninstall` removes a leftover `store/<name>/` through `install::remove_package_dir` for a name with no record, then answers `NotFound` as the CLI does. Unit tests in `lib.rs` / `records.rs`.
4. Swift binding test: a range over the `test:` plugin's releases with notes, `pinned`/`heldBy` on an upgrade, `latest` on a search result, a cancelled uninstall that reports nothing removed, and a leftover folder removed.
5. Verify: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, `just ffi-test`.

Status: done. `changelog_range` reads release notes only: the installed payload's file belongs to the old version and has no sections for the newer ones. `held_by` is always `None` for now, because `ketch sync` restores a pin without recording which `ketch.lock` it came from; recording that is a state change left for a later task. `outdated` now reports pinned packages, marked, and `upgrade` still skips them. `uninstall` reports a `removing` status and a `removed` success per package. Core gained `changelog::published_range` / `between` and `listing::fill_cached`; `published` shares its manifest fallback with the range through `manifest_for`. A breaking change to the binding (new record fields, pinned rows in `outdated`).
