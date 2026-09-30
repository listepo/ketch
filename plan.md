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
| B68 | todo | P1 | 3 | 0% | |
| F9 | todo | P2 | 2 | 0% | |
| M12 | todo | P2 | 3 | 0% | |
| F11 | todo | P3 | 2 | 0% | |
| R4 | todo | P2 | 3 | 0% | |
| B70 | in progress | P1 | 1 | 10% | Claude Code / opus-5.5 |

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




Plan:


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



Plan:


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

### B70. Flaky `upgrade_stops_a_process_holding_the_binary_when_yes`

`tests/auto_update.rs` starts the installed sleeper, sleeps a fixed 400 ms, then runs `ketch upgrade --yes` and expects it to report the process as `in use`. On macOS under load (several cargo builds in parallel) it failed on 2026-09-30 and 2026-10-01 and passed when run alone. Done means the test waits on a condition, not a delay, and survives a stress loop.

Plan (Claude Code / opus-5.5):
1. Reproduce: run the built `auto_update` test binary 64-way in parallel for several rounds.
2. `tests/support/mod.rs`: `Entry::sleeper` creates the file named by `KETCH_TEST_SLEEPER_READY`, when set, before it starts waiting (sh and cmd).
3. `tests/auto_update.rs`: set that variable on the child and wait for the file (bounded at 60 s so a sleeper that never starts fails instead of hanging) in place of the 400 ms sleep.
4. Verify: the same stress loop, then `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo nextest run`.
