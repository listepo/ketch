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

Generated by `scripts/cask.sh` into `listepo/homebrew-tap` on every release. Homebrew keeps only the bootstrap binary.

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

Fail-closed check of a registry tree: every `ketch.toml` through `Manifest::validate`, plus name/alias collisions. The client still warns and skips; this is what `listepo/ketch-registry` CI should run. `--json` for machines.

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

