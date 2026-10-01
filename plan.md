# ketch
https://github.com/pyrlyn/ketch
Catch releases straight from GitHub — a package manager for GitHub-released binaries and apps.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| F1 | done (ketch side) | P2 | 3 | 100% | Cursor / grok 4.6 |
| F2 | dropped (upstream declined) | P2 | 3 | — | Cursor / grok 4.6 |
| M3 | done | P2 | 5 | 100% | Claude Code / claude-opus-5 |
| F5 | done (ketch side) | P1 | 3 | 100% | Cursor / grok 4.6 |
| A2 | done | P1 | 2 | 100% | Muse Spark |
| A3 | evaluated (already shipped) | P3 | 1 | 100% | Muse Spark |
| B60 | in progress | P3 | 1 | 80% | Cursor / grok 4.7 |
| B64 | in progress | P0 | 4 | 70% | Claude Code / opus-5.5 |
| B65 | in progress | P0 | 2 | 0% | Cursor / grok 4.7 high |
| B71 | in progress | P1 | 2 | 90% | Claude Code / sonnet-5.5 |
| R3 | in progress | P1 | 3 | 67% | Cursor / grok 4.7 high |
| F8 | in progress | P2 | 3 | 0% | Cursor / grok 4.7 high |
| M9 | in progress | P2 | 5 | 90% | Claude Code / opus-5.5 |
| R4 | in progress | P2 | 3 | 90% | Claude Code / opus-5.5 |
| M14 | in progress | P2 | 3 | 90% | Claude Code / opus-5.5 |
| M15 | in progress | P3 | 2 | 90% | Claude Code / sonnet-5.5 |
| M16 | todo | P2 | 4 | 0% | |
| R5 | in progress | P2 | 4 | 90% | Claude Code / opus-5.5 |
| R6 | in progress | P2 | 4 | 90% | Claude Code / opus-5.5 |
| R7 | in progress | P2 | 2 | 90% | Claude Code / sonnet-5.5 |
| R8 | in progress | P3 | 3 | 90% | Claude Code / sonnet-5.5 |
| R9 | in progress | P3 | 3 | 90% | Claude Code / opus-5.5 |
| R10 | in progress | P3 | 3 | 90% | Claude Code / opus-5.5 |
| F12 | in progress | P3 | 5 | 50% | Claude Code / opus-5.5 |
| F13 | in progress | P3 | 4 | 80% | Claude Code / opus-5.5 |
| F14 | in progress | P2 | 3 | 90% | Claude Code / opus-5.5 |
| R11 | in progress | P2 | 3 | 90% | Claude Code / opus-5.5 |
| D1 | todo | P2 | 3 | 0% | |
| D2 | todo | P2 | 3 | 0% | |
| D3 | todo | P3 | 3 | 0% | |
| D4 | todo | P3 | 2 | 0% | |
| D5 | todo | P3 | 3 | 0% | |
| D6 | todo | P3 | 2 | 0% | |
| D7 | todo | P3 | 2 | 0% | |
| D8 | todo | P3 | 2 | 0% | |
| D9 | todo | P3 | 2 | 0% | |
| D10 | todo | P2 | 3 | 0% | |
| D11 | todo | P3 | 5 | 0% | |
| D12 | todo | P3 | 4 | 0% | |
| D13 | todo | P3 | 3 | 0% | |
| D14 | todo | P3 | 4 | 0% | |
| D15 | todo | P2 | 4 | 0% | |
| D16 | todo | P3 | 5 | 0% | |
| D17 | todo | P3 | 4 | 0% | |
| D18 | todo | P3 | 3 | 0% | |
| D19 | todo | P3 | 4 | 0% | |

### F1. Notarisation

The release binaries are signed with a Developer ID but not notarised, which is fine for a `curl`-fetched tarball and not fine the day ketch ships anything a browser downloads. Needs an App Store Connect key, `xcrun notarytool submit --wait` in the build job, and a stapled check in the smoke test.

Plan (Claude Code / claude-opus-5), prepared without a key: none exists yet, so everything ships switched off.

1. `release.yml` build job: a `Notarise` step for the two signed targets, gated on the repository variable `KETCH_NOTARIZE == 'true'`. Once on, it fails on a missing `APPSTORE_CONNECT_KEY` (the `.p8`, base64), `APPSTORE_CONNECT_KEY_ID` or `APPSTORE_CONNECT_ISSUER_ID`, the same way a missing certificate fails. It zips the packed binary with `ditto`, runs `xcrun notarytool submit --wait`, and fails unless the status is `Accepted`, printing the notary log.
2. Smoke test, under the same flag: `spctl --assess --type install` must report `source=Notarized Developer ID`. A bare Mach-O cannot be stapled (`stapler` takes bundles, disk images and packages only), so the check relies on Gatekeeper's online ticket lookup, not a stapled ticket.
3. `AGENTS.md` Releasing: document the switch and the three secrets.
4. Check: the workflow parses and `just check` is clean. The first real run needs the key; switching it on is the creator's step: add the secrets, set the variable, and run a release.

Plan (Cursor / grok 4.6): done on the ketch side. The Notarise step, smoke `spctl` gate, and AGENTS.md switch already exist. `tests/release-yml-notarize.sh` (YAML parse + load-bearing strings) is wired into `just lint-shell` and CI. Remaining step is the creator's: add the secrets, set `KETCH_NOTARIZE=true`, and run a release. Verified: script passes, `cargo fmt --check` and `cargo clippy --all-targets` clean.

### F2. Registry CI in ketch-registry

`ketch registry validate` exists in this repo (tree checks, name/alias collisions, optional `--fixture` / `--changed` offline-install). There is deliberately no registry-side workflow to run it: see Dropped below. The documented substitute is local validation plus a pre-push hook (`docs/REGISTRY.md`).

Dropped: ketch-registry deliberately removed its only workflow (commit `5a9bbd6`, "no CI is wanted in this repo"), so there is no upstream to land this in. Ketch side stays as is — the validator, docs, and fixture flow are the deliverable.

### M3. Provenance and signatures — done

`trust` table on Manifest (verifier sigstore|minisign|gpg, mode require|warn, signature/signed sidecar templates, issuer + repository/identity, public_key, fingerprint), checked in `Manifest::validate`; docs/MANIFESTS.md. `InstalledPackage.provenance` with old/new state tests. `src/trust.rs`: sigstore offline against an embedded trusted root plus a Rekor SET check, minisign-verify with the pinned key, pgp with an inline key pinned by fingerprint (never a keyring); fail closed unless `mode = "warn"`. Results in `info` (text + JSON), the install report and the log, identities sanitised. Fixtures in tests/fixtures/trust, unit tests in trust.rs, e2e in tests/trust.rs. Deps in Cargo.toml: sigstore, minisign-verify, pgp. Verified present in tree (`src/trust.rs`, `TrustPolicy`/`Provenance` in model.rs, install wiring, `trust` docs section).

### F5. Config reset and shared file backup — done (ketch side)

`ketch config reset` writes `config.toml` with compiled defaults after confirming. Existing file is backed up beside itself as `config.toml.bak-<unix-seconds>` via the shared `packages/file-backup` crate (missing file or byte-identical sibling backup → no copy). No daemon — ketch has none.

Done in this change: `ConfigCommand::Reset { yes }` in `src/cli.rs`, `Config::default_toml()` in `src/config.rs`, `reset()` in `src/cmd/config.rs`, `file-backup` crates.io dep in `Cargo.toml` (+ lockfile) with a gitignored `paths` override for local work (`just setup`), unit tests `default_toml_parses_back_to_compiled_defaults` and `a_reset_file_loads_back_to_the_effective_defaults` in `src/config.rs` (plus an `ENV_GUARD`/`CleanEnv` fix for the flaky token-fallback test they exposed), e2e in `tests/config_reset.rs` (defaults + backup, missing file, confirm gate), docs in `README.md` Configuration and `docs/COMMANDS.md`. Verified: `cargo fmt --check` clean, `cargo clippy --all-targets` clean, `config` unit suite green (12 passed), `config_reset` e2e green (3 passed).

Not done (out of ketch scope, needs rtok owner): `packages/file-backup` already exists standalone with its own tests; `rtok-agent-sdk::backup` still has its own `_backup/`-dir copy and does not re-export the shared crate — plan step 1's "move the rtok backup tests there / re-export through anyhow" is a rtok-side change.


---

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

### B71. Ambiguous bin glob refuses instead of taking directory order

`glob_preferred` (`crates/ketch-core/src/model.rs`, called from `platform/unix.rs` and `platform/windows.rs`) falls back to the first match in directory order when a `bin` glob matches several files and none has the entry's `name` as its stem, or the entry has no `name`. Directory order differs per OS (NTFS against ext4 and APFS), so the linked binary differs per OS: the same bug class as B62 and B64.

Decision (creator, 2026-10-01): refuse in that case. The error lists the candidate files, sorted so the message is the same on every OS, and says how to resolve it: set the `bin` entry's `name` or `path` in the manifest. `ketch install --bin` is not offered: it already refuses when the manifest names its binaries. A single match, or a stem equal to `name` (case-insensitive, `.exe` ignored), behaves as before. A glob matching nothing keeps its existing error. This is breaking: an install that used to link something now refuses, so the commit is `fix!:`.

Execution plan:

1. Add the failing unit test beside `glob_preferred_picks_the_stem_named_match_over_directory_order`.
2. Make `glob_preferred` return `Result<Option<&Path>>`, one OS-independent function; update both platform callers.
3. Update `docs/MANIFESTS.md` where `bin` globs are described.
4. Verify with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`, and a run of the binary against a scratch `KETCH_ROOT` if it can be done offline.

Status: implemented with unit and end-to-end tests in `tests/bin_choice.rs`; PR https://github.com/pyrlyn/ketch/pull/202

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

Status: implemented on branch `m9-list-modes`, PR https://github.com/pyrlyn/ketch/pull/152; waiting for CI and merge.

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

Done in the pull request: all eleven targets build and ran 60 s each without a crash in ketch. Left: review and merge; the optional nightly CI job is not added.

Check: `cargo +nightly fuzz build` succeeds for all targets; each target runs 60 s with no crash (or the crash is filed with a repro test); stable `just check` does not compile `fuzz/`.

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

Status: PR https://github.com/pyrlyn/ketch/pull/187, CI green. The version moved to `[workspace.package]`; `release.sh` and `crate-version.sh` read it there. Known follow-ups: release-plz stops with "cannot find package ketch-core" until a tag contains the crate, so the first release after the merge must be cut with `bump.yml` or `just release`; 15 doc examples that became doctests are marked `ignore` and should be rewritten; open PRs need a rebase across the renames.

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

Status: PR https://github.com/pyrlyn/ketch/pull/189 (stacked on #187), CI green on all three OSes. `ui.rs` and `tui/` left the core; the core reports through `Ctx { cfg, report }`, with `LogReporter` and a test `Recorder`. The binary choice goes through `Reporter::choose`, the hook R7 replaces. Left as is: colour and verbosity stay global inside `ui.rs`; the TUI still receives events through `ui::Terminal`; the `--tui` check ran on a zero-size pty, so it is weak.

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

Status: PR https://github.com/pyrlyn/ketch/pull/204. `decide::Decider` (`choose_binary`, `stop_processes`) sits in `Ctx`; `Reporter::choose`/`offer` and `InstallRequest::interactive` are gone. `process::offer_to_stop`, the other mid-run question, went through the same trait. `ui::ctx_asking` opts the CLI in (install, upgrade, rollback, link, self upgrade; off under `--yes`). Nothing in `crates/` reads stdin.

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

Status: PR https://github.com/pyrlyn/ketch/pull/184, CI green, awaiting review. Pid re-entrancy had no callers and was removed. Not done: SIGINT wiring for the CLI (no signal handler exists; tokens are never cancelled there), and `self upgrade` / registry downloads pass a never-cancelled token.

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

Status: PR https://github.com/pyrlyn/ketch/pull/208. Surface: `KetchCore` (installed, search, outdated, install, upgrade, uninstall, changelog, doctor), records, `Reporter`/`Decider` callbacks, `CancelToken`, `KetchError`. `changelog` returns a structured `Changelog` for one version rather than a from–to string; the SwiftUI adapter maps it in F12.

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

Status: PR https://github.com/pyrlyn/ketch/pull/205, research in `docs/research-desktop-windows-linux.md` (linked from `docs/research-desktop.md`). Recommendation: WinUI 3 in C# over R9's UniFFI binding (via `uniffi-bindgen-cs`, unpackaged through winget) for Windows, with `windows-reactor` as the Rust alternative to spike, and GTK 4 + libadwaita through gtk4-rs linking `ketch-core` directly for Linux; the open decisions are listed at the end of the research page.

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

Status: part 1 (app shell on a fake core) is PR https://github.com/pyrlyn/ketch/pull/185, stacked on #183, CI green including a `macos-app` job. XcodeGen 2.46.0 chosen over Tuist and plain SwiftPM (sources in `docs/research-desktop.md`). Remaining: a `LiveKetchCore` adapter once R9 lands, switching `Theme.swift` to F14's generated tokens, and the manual checks against the real core.

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

Status: PR https://github.com/pyrlyn/ketch/pull/190 (stacked on #185), CI green. Sparkle 2.10.0 with EdDSA; the feed is `releases/download/desktop-appcast/appcast.xml`, independent of `/releases/latest`. Two real bugs fixed on the way: `cliff.toml`'s unanchored `tag_pattern` and `select_release` ranking `desktop-v*` above CLI tags under `--pre`. Waiting on the creator: secrets `APPSTORE_CONNECT_KEY`, `APPSTORE_CONNECT_KEY_ID`, `APPSTORE_CONNECT_ISSUER_ID`, `SPARKLE_ED_PRIVATE_KEY`, and the matching `SUPublicEDKey` committed. No real signed release has run; the R9 XCFramework step is a marked hook.

### F14. Design system for the macOS app: `DESIGN.md` and tokens

The creator asked for a new, polished design with tokens (2026-10-01), following the F12 glass requirement: frosted or glossy glass with a 3D effect, macOS 26 Liquid Glass.

Plan:
1. Research the `DESIGN.md` format (a design spec agents read: tokens in front matter plus prose) with primary sources; follow it if it is a maintained spec, else a documented equivalent.
2. `desktop/macos/DESIGN.md`: principles, glass and depth rules, colour (light and dark, accent, semantic status colours for installed / update / error / busy), typography (SF Pro scale), spacing, radii, elevation levels (shadow and highlight per layer for the 3D effect), motion, icons (SF Symbols), components (sidebar, package row, card, progress, sheet, menu-bar extra, buttons), accessibility fallbacks (Reduce Transparency, Increase Contrast, Reduce Motion).
3. Tokens as one source: `desktop/macos/design/tokens.json` in the W3C Design Tokens format, generated into Swift (`Tokens.swift`, header says it is generated) by a maintained generator (e.g. Style Dictionary), run by a `just` recipe, with a drift check.
4. `desktop/macos/design/preview.html`: a static page rendering the tokens and key components in light and dark, for review.
5. Hand-off to F12: the app uses the generated tokens instead of literals.

Check: the generator reproduces `Tokens.swift` byte-for-byte; the preview renders both themes; text colours meet WCAG AA contrast on their glass backgrounds (checked by the script or documented per pair).

Status: PR https://github.com/pyrlyn/ketch/pull/188, CI green (new `design` job). Google Labs DESIGN.md spec (alpha, `@google/design.md` 0.4.0) with generated front matter; tokens in W3C DTCG 2025.10; Style Dictionary 5.5.5 with custom formats for Swift (four appearances), CSS and the front matter; `just design-check` covers drift, WCAG AA and lint. Research in `docs/research-design-system.md`. Remaining: an Accessibility Inspector pass on the real Liquid Glass material, and the F12 switch of `Theme.swift` to `Tokens.swift`.

### M14. JSON Schema for the package manifest

`ketch.toml` (`Manifest` in `src/model.rs`) is the third TOML file ketch owns, after `config.toml` and `ketch.lock` (M13). `AGENTS.md` requires a schema for it too. Its nested types and custom (de)serializers (`PackageRef` as `scheme:id`, the `trust` and `hooks` tables) make it larger than M13.

Done when `docs/manifest.schema.json` is generated from `Manifest` with `config::assert_schema_current`, committed, checked by a drift test, and linked from `docs/MANIFESTS.md`. Every field a registry `ketch.toml` in `pyrlyn/ketch-registry` uses validates against it.

Plan:
1. `model.rs`: `cfg_attr(test, derive(schemars::JsonSchema))` on `Manifest` and every type it holds, the way M13 did `ConfigFile` and `Lockfile`. `PackageRef` is described as the string its `TryFrom<String>` accepts (a `pattern` for `scheme:id` or anything with a `/`), `ExtraPath` as the untagged string-or-table it is, `trust` and `hooks` as the closed tables `deny_unknown_fields` makes them. `name` is left out of `required`, because a registry package folder supplies it.
2. `docs/manifest.schema.json` from `config::assert_schema_current::<Manifest>`, with a drift test beside the M13 ones.
3. A test that validates the root `ketch.toml`, every `[[package]]` in `builtin.toml` and the examples in `docs/MANIFESTS.md` against the committed schema, with the `jsonschema` crate as a dev-dependency (maintained, draft 2020-12, no network with default features off); plus cases the deserializer rejects, so the schema is not looser than the reader where it can say so.
4. `docs/MANIFESTS.md`: link the schema and show the taplo `#:schema` directive; the root `ketch.toml` carries it. `toolchain.md` row for `jsonschema`.
5. Check against the live `pyrlyn/ketch-registry` files; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`.

Status: PR https://github.com/pyrlyn/ketch/pull/206. Schema generated and drift-tested; the root `ketch.toml`, `builtin.toml`, the docs examples and all six `pyrlyn/ketch-registry` files validate; a property test holds each pattern to its Rust check. Found on the way: `bin` entries and `[asset]` lack `deny_unknown_fields`, so a misspelt key there is still ignored (the schema follows ketch).

### M15. `log_level` and `log_format` as enums in `config.toml`

`ConfigFile` holds both as free strings and checks them at load time, so the schema cannot list the allowed values. `AGENTS.md`: constraints live in the types.

Done when `ConfigFile` uses `crate::log::Level` and `crate::log::Format` (serde, lower case) directly, a bad value still fails with an error naming the file and the key, `docs/config.schema.json` lists the values, and the environment variables keep working as before.

Status: PR https://github.com/pyrlyn/ketch/pull/203.

Execution plan: give `Level` and `Format` in `crates/ketch-core/src/log.rs` serde (lower case) and `schemars` derives; switch `ConfigFile.log_level` and `log_format` in `crates/ketch-core/src/config.rs` to `Option<Level>` and `Option<Format>`; keep the environment variables parsed by `FromStr` as before; make the TOML parse error name the file and the key; regenerate `docs/config.schema.json` with the existing schema export; add tests for a bad value, each environment variable, the schema enum and unchanged loading of valid files; then run fmt, clippy, nextest and `ketch doctor` against a scratch root.

### M16. One module owns config file I/O

`AGENTS.md`: one module owns all config loading, validation and editing, and the rest of the code does not import `toml` or `toml_edit`. Today `registry.rs`, `manifest.rs`, `extra.rs`, `push.rs`, `wizard.rs` and `model.rs` use them directly, besides `config.rs` and `lockfile.rs`.

Done when TOML parsing, rendering and editing for the files ketch owns go through one module, and no other module imports `toml` or `toml_edit`. Behaviour does not change. Before starting, confirm with the creator whether `ketch.toml` manifests and `ketch.lock` belong to that module or keep their own, with only the TOML calls moved.

### R11. Desktop apps on macOS, Windows and Linux: capabilities, shared layer and per-platform interfaces

The creator asked (2026-10-01) for research on the desktop app for Windows and Linux, macOS included: what the app can do, what is common and written once versus what each platform does its own way, the common interface and each platform's interface, and tasks for each platform. Decided by the creator the same day: Windows is C# + WinUI 3 (Windows App SDK) over `ketch-ffi` (UniFFI, kept at 0.32 for now); Linux is written in Vala, with the toolkit (GTK or Qt/KDE) and the UI markup to be chosen by this research.

Done when `docs/research-desktop-platforms.md` (linked from `docs/research-desktop.md` and `docs/research-desktop-windows-linux.md`) holds a capability matrix per platform with the native API and its maturity, the common/specific split, the common core and app contract (with the gaps R9 left), the common UI and each platform's departures from it per its HIG, the way forward for the C# binding gap and for Vala reaching the core, a recommendation and the open decisions; every fact with a primary source and a version or check date, secondary-only facts marked **unverified**. The resulting tasks are in `plan.md` as `todo`, mirrored in `todo.md`.

Execution plan (Claude Code / opus-5.5):
1. Read the repository facts: R9's exported surface (`crates/ketch-ffi`), the macOS app's `KetchCoreProtocol.swift` and views, the design pipeline (`desktop/macos/design/`), R10's page; list the contract gaps between the app and `ketch-ffi`.
2. Primary sources, checked 2026-10-01: Apple, Microsoft Learn and GNOME/KDE developer docs and HIGs; crates.io, NuGet and GitHub/GitLab release APIs for UniFFI, `uniffi-bindgen-cs`, cbindgen, Vala, GTK, libadwaita, Blueprint, Qt binding projects for Vala, third-party UniFFI generators.
3. Write the page: matrix, common vs specific, interfaces, the C#/UniFFI version gap and the Vala/C ABI route, recommendation, open decisions.
4. Tasks: new `D` ids (desktop), shared first, then macOS (referencing F12/F13/F14 rather than repeating them), Windows, Linux; rows appended to the table, cards appended at the end, `todo.md` in sync.
5. Close at 90% with a `Status:` line; PR against `main`, CI watched until green.

Status: PR https://github.com/pyrlyn/ketch/pull/211, research in `docs/research-desktop-platforms.md`, tasks D1–D19. Recommendation: fix the `ketch-ffi` contract once (D1, D2), then WinUI 3 in C# through `uniffi-bindgen-cs` built from PR #176 pinned by commit, and on Linux Vala + GTK 4 + libadwaita with Blueprint over a small `ketch-capi` C ABI, since no Qt binding for Vala exists; the open decisions are listed at the end of the research page.

### D1. `ketch-ffi`: foreign traits and per-operation callbacks

UniFFI calls callback interfaces "(soft) deprecated" in favour of foreign traits (new in 0.32), and `KetchCore::new` takes the `Reporter` and `Decider` once, while every app wants them per operation so two screens can each watch their own work. Research: `docs/research-desktop-platforms.md`, section 3a, gap G1.

Done when `Reporter` and `Decider` are foreign traits, `install`, `upgrade` and `uninstall` take a reporter, a decider and a `CancelToken` per call, the constructor no longer takes them, the Swift binding test covers a per-call reporter and `stop_processes`, and `crates/ketch-ffi`'s docs say why. A breaking change to the binding, marked as such.

### D2. `ketch-ffi`: records and operations the apps need

The macOS app's protocol needs things `ketch-ffi` does not give: a changelog across a version range, pinned packages in `outdated` with what holds them, `latest` in search results, and an `uninstall` that can be cancelled, reports progress and removes a leftover store folder for a name with no record, as the CLI does. Research: section 3a, gaps G2–G5.

Done when `changelog_range(package, from, to)`, `Upgrade.pinned` (and the lock that holds it, when known), `RegistryPackage.latest` and the new `uninstall` exist with unit tests, the Swift binding test exercises each, and D4's fixtures can model them.

### D3. `ketch-ffi`: the remaining CLI operations

Screens the apps already draw (Activity history, package info, pin, rollback, Doctor fixes, PATH status) have no core call behind them. Research: section 3a, gap G6.

Done when history (`stats.db`), info, pin/unpin, rollback, prune, registry refresh, `path` status and install, a doctor fix action and reading ketch's config are exported as thin calls into existing core code, each with a test; `registry push` stays out (it owns a tokio runtime).

### D4. Contract fixtures for every app's fake core

Three apps each test against a fake core; if each fake invents its own records and event streams, they will drift from the real one and from each other. Research: section 2, "Written once".

Done when a set of language-neutral JSON scenarios (records, event streams with progress and `Abandoned`, `Busy`, `Cancelled`, decisions) is generated from the Rust types by a test that fails on drift, and the macOS app's fake core reads them; the Windows and Linux fakes read the same files when they exist.

### D5. Design tokens for XAML and GTK

`tokens.json` feeds only Swift today. The Windows and Linux apps should share ketch's brand (accent, status colours, spacing, radii, type scale) without imitating the glass. Style Dictionary has no XAML or GTK format, so it takes two custom ones. Research: section 2.

Done when the token source lives in `desktop/design/`, `just design-tokens` also writes a XAML `ResourceDictionary` (Light, Dark, HighContrast theme dictionaries) and a GTK stylesheet setting libadwaita's CSS variables, glass and elevation tokens stay macOS-only, generated files say so in their first lines, and a drift check covers all outputs. Needs the creator's answer on brand tokens (open decision 6).

### D6. String keys and glossary for three apps

Three apps with three native string formats (String Catalog, `.resw`, gettext) will translate the same phrases differently unless the keys and terms are agreed once. Research: section 2, "Strings".

Done when a short convention for string keys and an English glossary of ketch's terms (package, source, pin, hold, store, link) are in the desktop docs, and each app's localisation task points at them. Needs the creator's answer on localisation (open decision 7).

### D7. macOS: update notifications

The macOS app checks for updates on a timer (F12) but tells nobody unless the window or menu-bar panel is open. Builds on F12's live core; F12's remaining work (the `LiveKetchCore` adapter and the manual checks) stays in F12. Research: section 1.

Done when new upgrades since the last notice post one `UNUserNotificationCenter` notification, authorisation is asked only when the user turns notifications on in Settings, clicking it opens Updates, and a unit test covers which upgrades count as new.

### D8. macOS: `ketch://` links

A link on a web page or in the registry could open a package in the app. Whatever a link carries is untrusted input, so it is validated by the core and never installs without a confirmation. Research: section 1, "Deep links".

Done when `CFBundleURLTypes` registers `ketch`, `onOpenURL` routes the actions the creator allowed (open decision 10) through the core's validation, an install from a link always shows a confirmation, and tests cover malformed and hostile links.

### D9. macOS: String Catalog and VoiceOver pass

The macOS app has no localisation catalog, and its accessibility was only checked on the fake core. F14 keeps its own Accessibility Inspector pass on the real glass; this task covers strings and labels. Research: section 1.

Done when user-facing strings are in a String Catalog following D6's keys, every icon-only control has an accessibility label, and a VoiceOver walk through the nine screens finds no unlabeled control.

### D10. Windows: C# binding for `ketch-ffi`

The Windows app is C# + WinUI 3 over `ketch-ffi` (creator, 2026-10-01). `uniffi-bindgen-cs` last released for UniFFI 0.31.0; `ketch-ffi` is on 0.32.2. Decision recorded by R11: first try the generator built from PR #176 (UniFFI 0.32.0), pinned to commit `0fc022aa1d73fb1dda91a778b63f2824d7dca58b`; if it fails against 0.32.2, use the C ABI from D15 through `LibraryImport`, or with the creator's approval pin `ketch-ffi` to UniFFI 0.31.2 and generator v0.11.0. Research: section 4.

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

### D15. Linux: `ketch-capi`, a C ABI and VAPI for Vala

UniFFI has no Vala or C generator, so the Vala app needs a C ABI. R11 recommends one small crate with an opaque core handle, callbacks with user data, records as JSON strings and a hand-written VAPI; it is also the fallback C# route in D10. Depends on D1 and D2 for the per-call shape. Research: section 5.

Done when `crates/ketch-capi` exports the contract over `extern "C"` with a cbindgen header checked for drift, its `unsafe_code` exception is scoped and explained, a `.vapi` binds it, a Vala test built with Meson calls `installed`, `doctor` and a cancelled install against a scratch root on Linux CI, and the JSON payloads have a schema. Needs the creator's answer on open decision 3.

### D16. Linux: Vala + GTK 4 app shell on a fake core

The Linux app's screens in Vala with GTK 4 and libadwaita, following the GNOME HIG, built before the C ABI is ready. GTK 4 + libadwaita over KDE and Blueprint for the markup were decided by the creator (2026-10-01, open decisions 1 and 2). Research: sections 3b, 3c and 5.

Done when a Meson project builds a libadwaita app with the common screens in an `AdwNavigationSplitView` that adapts to narrow windows, Blueprint files (pinned as a Meson subproject) for the UI, `AdwAlertDialog`, `AdwBanner` and toasts for decisions, busy and finished work, dark and high-contrast styles, all on a fake core reading D4's fixtures, with a Linux CI job that builds and runs its tests.

### D17. Linux: the app on the real core

Swap the Linux app's fake core for `ketch-capi`. Depends on D15 and D16.

Done when every screen runs on the C ABI, calls run off the main loop with events returned through the `GLib.MainContext`, cancel and `Busy` behave as in the contract, and a manual pass against a scratch root is recorded.

### D18. Linux: notifications, background and autostart

GNOME has no tray in its HIG; an app that checks in the background asks the Background portal and notifies through `GNotification`. Research: sections 1 and 3c.

Done when update notices go through `GNotification` (the Notification portal under Flatpak), background running and start at login are requested through the Background portal (libportal) with an XDG autostart entry outside a sandbox, `ketch://` links follow D8's rules, and the tray is either absent or the creator's chosen StatusNotifierItem option (open decision 8).

### D19. Linux: packaging and release

How the Linux app reaches users. R10 left Flatpak against distribution packages open, and Flatpak needs home access for PATH work.

Done when the creator has chosen the format, CI builds it on `desktop-linux-v*` tags without touching `/releases/latest`, the app ships AppStream metadata and a `.desktop` file that validate, and updates come from the chosen package manager.
