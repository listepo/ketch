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
| R3 | in progress | P1 | 3 | 67% | Cursor / grok 4.7 high |
| F8 | in progress | P2 | 3 | 0% | Cursor / grok 4.7 high |
| M9 | in progress | P2 | 5 | 90% | Claude Code / opus-5.5 |
| B66 | todo | P1 | 2 | 0% | |
| B68 | todo | P1 | 3 | 0% | |
| F9 | todo | P2 | 2 | 0% | |
| B69 | todo | P2 | 1 | 0% | |
| M10 | todo | P2 | 2 | 0% | |
| M12 | todo | P2 | 3 | 0% | |
| F11 | todo | P3 | 2 | 0% | |
| R4 | todo | P2 | 3 | 0% | |
| R5 | in progress | P2 | 4 | 0% | Claude Code / opus-5.5 |
| R6 | todo | P2 | 4 | 0% | |
| R7 | todo | P2 | 2 | 0% | |
| R8 | in progress | P3 | 3 | 0% | Claude Code / sonnet-5.5 |
| R9 | todo | P3 | 3 | 0% | |
| F12 | in progress | P3 | 5 | 0% | Claude Code / opus-5.5 |
| F13 | todo | P3 | 4 | 0% | |

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

### B66. Windows self-uninstall removes the registry entries ketch wrote at install

Ivan: uninstalling ketch on Windows must remove the registry entry that was added at install.

What ketch writes to the registry today: only `HKCU\Environment\Path`. `install.ps1` adds the bin dir there, and so does `ketch path install` (`shell::install_user`, through `[Environment]::SetEnvironmentVariable(..., 'User')`). There is no Apps & Features (`...\CurrentVersion\Uninstall\ketch`) key, because `dist-workspace.toml` sets `installers = []`. `self uninstall` removes the Path entry only when `UninstallPlan.user_path` is true (`shell::user_path_configured(cfg)`). That is false under `--keep-packages`, and it may miss an entry written by `install.ps1 -InstallDir <dir>` for a bin dir that is not `cfg.bin_dir`.

Plan:
1. Reproduce on a Windows runner: `install.ps1` (default and with `-InstallDir`), then `ketch self uninstall --yes`, then read `HKCU\Environment\Path` and list what is left.
2. Keep one inventory of every registry value ketch writes (a function in `src/shell.rs`, for example `registry_entries(cfg)`): the user Path entry today, and M12's `HKCU\Software\Microsoft\Command Processor\AutoRun` addition later. `self uninstall` removes each entry it finds, matching Path entries the way `install.ps1`'s `Normalize-PathKey` does (quotes, slashes, trailing separator, case).
3. `ketch doctor` warns when an inventory entry points into a ketch root that no longer exists.
4. Ask Ivan whether he also expects an Apps & Features entry. That would be new (register at `self install`, remove at uninstall), not a fix.

Check: Windows e2e (`tests/install_windows.rs` or `tests/install_ps1.rs`): after `install.ps1` + `ketch self uninstall --yes`, the user Path holds no entry for the ketch bin dir, with and without `-InstallDir`; `just check` green on all three OSes.

### B68. Update installs into a fresh folder so stale files cannot interfere

Ivan: update must clean or delete the program folder and install into a fresh one.

Today the per-version prefix is already fresh. `move_into_store` stages the payload as `<version>.incoming` and swaps it in through `<version>.old`, so a new version and a `--force` reinstall of the same version both replace the directory whole. Gaps: (a) stale `.incoming` / `.old` siblings survive when their best-effort removal fails; (b) old links and copied files the new version no longer has are removed by `platform.unplace(&stale)`, and a failure there is only a warning; (c) retained prefixes of earlier versions stay on purpose, because `ketch rollback` (M6) needs them.

Plan:
1. Sweep stale siblings (B67's helper) at the start of every install and upgrade, before hooks run.
2. If a stale sibling or a stale link cannot be removed, fail the update before anything is placed, naming the path. Do not warn and continue.
3. Decision for Ivan: "delete the program folder" must not break rollback. Proposal: keep retained prefixes (they are separate directories, so they cannot leak files into the new one) and say so in `docs/COMMANDS.md`. The alternative is to drop retention by default (`retain = 0`).
4. `ketch self upgrade` replaces the binary in place (`replace_binary`). Its leftovers are covered by B60, and nothing more is needed here.

Check: e2e: a file present in 1.0.0 and absent from 1.1.0 is gone after upgrade, and a same-version `--force` reinstall leaves no stale file; a planted `1.1.0.incoming` does not end up inside the new prefix; `just check`.

### F9. `ketch install <pkg>` on an installed package offers the update

Ivan: `ketch install <program>` when it is already installed asks "update?". Yes updates. With no update available, it says it cannot install because the package is already installed.

Today `install::prepare` returns `Error::AlreadyInstalled` (exit 5, hint "Use --force to reinstall.") only when the resolved tag equals the installed one. When a newer release exists, `ketch install` upgrades silently.

Plan:
1. Installed and a newer release resolves, with an unversioned spec: ask through `ui::confirm`: `<pkg> <installed> is installed; update to <latest>?`. The default answer is a decision for Ivan; the proposal is No, matching the other confirms. Yes runs the same path as `ketch upgrade <pkg>`, including the update hooks. No exits 0 with a note.
2. Installed and nothing newer: fail with `cannot install <pkg>: <version> is already installed and no update is available`, still exit 5. `--force` still reinstalls; whether its hint stays is a decision for Ivan.
3. `--yes` answers yes. Without a TTY and without `--yes`, fail and name `--yes` / `ketch upgrade`, instead of upgrading silently. This is a behaviour change, so it goes in `CHANGELOG.md`.
4. Pinned packages keep `Error::Pinned`. An explicit version (`pkg@1.2.0`) keeps today's behaviour. In a batch, each installed package is asked separately. `ketch sync` is unaffected.

Check: e2e with the mock release API: newer + yes → upgraded; newer + no → unchanged, exit 0; nothing newer → the message and exit 5; non-TTY without `--yes` → error; `docs/COMMANDS.md` updated; `just check`.

### B69. Uninstalling a package that is not installed prints only "not found"

Ivan: uninstalling an already-removed program prints only "program not found".

Today `cmd::pkg::uninstall` resolves every name first and fails with `Error::NotInstalled` (`` `<name>` is not installed ``, exit 4), rendered by `ui::error` with the `error` label.

Plan:
1. A missing name prints one line, `<name>: not found`, with no hint, no detail lines and no summary. Exit code 4 stays for scripts. Wording: ketch's docs say "package"; Ivan's phrase was "program not found". Confirm the final text with Ivan.
2. Several names: every missing one is reported, and nothing is removed (the up-front resolution stays, so a typo still stops the command).
3. After B67: a name with no state record but a leftover `store/<name>/` gets the leftover removed and still reports not found.

Check: e2e: uninstall twice → the second run prints exactly one line and exits 4; trycmd snapshot; `just check`.

### M10. Man pages in roff for every command

Today `extra::render_manpage` writes one hand-rolled `ketch.1`, listing top-level commands with no options and no nested subcommands. `write_ketch_docs` places it under `share/man/man1/` at `self install`.

Plan:
1. Add `clap_mangen` (rtok already uses it for `rtok man`, a working reference). Generate `ketch.1` plus `ketch-<cmd>.1` for every visible subcommand, recursively (`ketch-config-create.1`, `ketch-self-uninstall.1`, …), with options, defaults, env vars and examples from the clap definitions. Replace `render_manpage`.
2. Write every page through `write_ketch_docs` as `ExtraPath` records, so uninstall and relink remove them with the same ownership proof as today's page.
3. A hidden `ketch man --out <dir>` (or a `just man` recipe) for packaging; the Homebrew cask may ship them.

Check: a test walks `Cli::command()` and asserts one page per visible command; `mandoc -Tlint` clean on macOS and Linux CI; `man ketch-install` works after `self install` in a scratch root; `just check`.

### M12. Windows completion: PowerShell `Register-ArgumentCompleter` and doskey macros for cmd

Today clap_complete emits `Register-ArgumentCompleter -Native -CommandName 'ketch'`, and `ketch completions powershell --install` writes it to `Documents\PowerShell\Completions`. PowerShell does not load that directory by itself, so nothing is active until the user dot-sources it.

Plan:
1. PowerShell: a managed block in the CurrentUserAllHosts profile that dot-sources the script, for both PowerShell 7 (`Documents\PowerShell`) and Windows PowerShell 5.1 (`Documents\WindowsPowerShell`). Resolve Documents through the shell, not a fixed path, because OneDrive may redirect it. Use the same managed-block mechanism as the PATH blocks in `src/shell.rs`, so `self uninstall` removes it. Dynamic values come from M11's completer.
2. cmd: cmd.exe has no programmable argument completion, so ship doskey macros. Generate `share/ketch/ketch.doskey` (the macro list is for Ivan to choose; for example `ki=ketch install $*`, `ku=ketch upgrade $*`, `kl=ketch list $*`) and load it through `HKCU\Software\Microsoft\Command Processor\AutoRun` (`doskey /macrofile=<file>`). Append to an existing AutoRun value rather than replace it. Register the value in B66's inventory so `self uninstall` restores the old value.
3. Optional, only if Ivan wants real Tab completion in cmd: a clink Lua script generated from the CLI.

Check: Windows CI: `pwsh -c "TabExpansion2 'ketch ins' 9"` returns `install`; after install, AutoRun contains the doskey line and `ki` expands in a new cmd; after `self uninstall`, the profile block and the AutoRun addition are gone and an earlier AutoRun value is intact; `just check`.

### F11. Emoji icons per operation, `emoji` config key (default true)

Plan:
1. One table in `src/ui.rs` maps each operation to an icon (proposal: install 📦, upgrade ⬆️, uninstall 🗑️, download ⬇️, link 🔗, rollback ⏪, search 🔍, doctor 🩺, success ✅, warning ⚠️, error ❌, note ℹ️). Ivan picks the final set.
2. Config: `emoji = true` in `Config` / `Config::default_toml()`, the `KETCH_EMOJI` env var, and a `--no-emoji` global flag if wanted. Document it in the Configuration table in `README.md` and `docs/COMMANDS.md`, and in the `config reset` defaults test.
3. Icons appear only on human-facing status lines going to a terminal. They never appear in `--json`, `--names-only`, `ui::out` data, the log file, or when `TERM=dumb`.
4. Width: emoji are double-width, so pad the verb column with `unicode-width` and keep columns aligned with and without icons.

Check: snapshots with emoji on and off; JSON and piped output contain no emoji; `emoji = false` and `KETCH_EMOJI=0` turn them off; `just check`.

### R4. Fuzz testing with cargo-fuzz / libFuzzer

Plan:
1. Library target: ketch is binary-only (no `src/lib.rs`), so the fuzz crate cannot reach the parsers. Add `src/lib.rs` holding the modules, and make `main.rs` a thin caller. Keep `unsafe_code = "forbid"`. Alternatively, a `#[cfg(fuzzing)]` `fuzzing` module with entry points, as rtok does.
2. `fuzz/`: a standalone cargo-fuzz workspace excluded from the root one (`cargo-fuzz = true`, `libfuzzer-sys` 0.4, `arbitrary`), the same shape as rtok's `test/cargo-fuzz` branch.
3. Targets: `cli_argv` (`Cli::try_parse_from` over arbitrary argv); `package_spec` (`PackageSpec::parse`); `manifest_toml` (`manifest::parse_registry` + `Manifest::validate`); `lockfile` (parse + validate); `state` (state file load from bytes); `checksum_file` (`source::github::parse_checksum_file`, `parse_digest`); `archive_extract` (tar.gz, tar.bz2, tar.xz and zip through `extract` into a temp dir, asserting every written path stays under the root through `safe_member_path`); `extra_paths` (classification in `src/extra.rs`); `plugin_protocol` (the `ketch-source-*` JSON in `src/source/plugin.rs`); `hook_line` (Windows hook-line quoting, `src/hooks.rs`); `printable` (`ui::printable` filtering). Seed corpora come from `tests/fixtures`.
4. Verify: `cargo +nightly fuzz build` for every target, then a short run of each (`cargo +nightly fuzz run <target> -- -max_total_time=60`). Every crash becomes a minimized regression test in `tests/`, with the fix in its own PR.
5. Optional CI: a non-required nightly job (build plus a short run) on Linux. Do not touch `dependabot.yml` or `sync-docs.yml`.
6. Deliver as a PR; do not merge it.

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

### F12. Native macOS app (SwiftUI) on `ketch-ffi`

A SwiftUI app in the repository (e.g. `desktop/macos/`), consuming R9's XCFramework and Swift package: installed and registry packages (list, search), install, upgrade, uninstall, changelog, doctor, with progress from the reporter callback and dialogs for the decider. It manages the same ketch root as the CLI and respects the same lock (R8).

Decided (creator, 2026-09-30): the app shares the ketch root (`~/.ketch`, or `KETCH_ROOT`) with the CLI — one state, one lock, one store. Also decided: a menu-bar extra is in scope (status and pending upgrades at a glance, quick actions), and the app follows ketch's triple licence (GPL-3.0-only, royalty-free, commercial). Minimum macOS version: 26 (the deployment target of the app and of R9's XCFramework).

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

### F13. macOS app release pipeline

A separate workflow for the app: `xcodebuild` archive, Developer ID signing with the existing certificate, notarisation and stapling (a `.app`/`.dmg` can be stapled, unlike the bare CLI binary; needs F1's App Store Connect key), and an update mechanism chosen in the task (a sourced comparison first). cargo-dist keeps releasing the CLI; the CLI's asset names do not change.

Constraint found in the survey: `install.sh`, `install.ps1` and the GitHub source (`src/source/github.rs`) resolve the host through `/releases/latest`. An app release in this repository that GitHub marks as latest would send every CLI installer and `ketch self upgrade` to a release without CLI assets.

Plan:
1. Decide where app releases live (creator): a separate repository (e.g. `pyrlyn/ketch-desktop` releases, built from this repo) or this repository with every app release created `make_latest: false` and a `desktop-v*` tag. Add a regression check either way: a script in `tests/` asserting the app workflow never creates a release eligible for latest.
2. Versioning: the app has its own version (`desktop-vX.Y.Z`), separate from the CLI's; release-plz and `scripts/release.sh` ignore it. Changelog section for the app via git-cliff with a path filter on `desktop/` and `crates/ketch-ffi/`.
3. Workflow `.github/workflows/desktop-release.yml`, `workflow_dispatch` only: macOS runner with Xcode 26; `just xcframework`; `xcodebuild archive` + `-exportArchive` with a Developer ID export options plist; signing with the existing `MACOS_CERTIFICATE`/`MACOS_CERTIFICATE_PWD` (hardened runtime on); a `.dmg`; `xcrun notarytool submit --wait` with the App Store Connect secrets from F1; `xcrun stapler staple` on the `.dmg`; `spctl --assess` as the smoke test; SHA256 checksum next to the asset.
4. Updates: compare Sparkle 2 and alternatives with primary sources (versions, dates, licence, EdDSA signing) and pick one; its signing key becomes a repository secret; the appcast is published with the release.
5. Homebrew: optionally a second cask (`ketch-app`) generated by a script like `scripts/cask.sh` and pushed to the tap — only after the first signed release works.
6. Docs: AGENTS.md "Releasing" gets an app subsection (secrets, tag scheme, the latest-release constraint).

Check: a dry run on a branch produces a signed, notarised, stapled `.dmg` that opens on a clean macOS 26 machine without a Gatekeeper prompt; `install.sh` still resolves the CLI release afterwards; the update feed moves an older build to the new one.
