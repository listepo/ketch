# ketch
https://github.com/listepo/ketch
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
| B61 | in progress | P2 | 2 | 80% | Cursor / grok 4.7 |
| B63 | in progress | P1 | 3 | 95% | Cursor / grok 4.7 |
| B64 | in progress | P0 | 4 | 0% | Claude Code / opus-5.5 |
| B65 | todo | P0 | 2 | 0% | |
| R3 | todo | P1 | 3 | 0% | |
| F8 | todo | P2 | 3 | 0% | |
| M9 | in progress | P2 | 5 | 0% | Claude Code / opus-5.5 |

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
4. Registry has no CI — resolved as dropped (F2): `listepo/ketch-registry` removed its only workflow (commit `5a9bbd6`, "no CI is wanted in this repo"), so there is no upstream to land a validating workflow in. Ketch side documents local validation plus a pre-push hook (`docs/REGISTRY.md`); name/alias collisions are fatal in `ketch registry validate` and warnings on `ketch update` by design (best-effort client).
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

### B61. `self update` swap fails on a transient Windows lock

Observed on Windows: `self update` downloaded and verified the release, then failed with `…in\ketch.exe: Access is denied. (os error 5)` during the swap; an identical retry minutes later updated cleanly, and the restore path left the running 0.4.4 working throughout. That error at exactly this moment is the signature of antivirus real-time protection (Defender) holding `ketch.exe` or the `.old` destination across the rename or copy — a lock that lasts milliseconds and is gone by the next attempt.

Plan: a bounded retry with backoff around the filesystem operations in `replace_binary` — `rename(exe, backup)`, `copy(fresh, exe)` and the backup removal — retrying only `PermissionDenied` and sharing violations (Windows `os error 5` and `os error 32`), a few attempts 100–250 ms apart. The existing restore-on-failure stays the response once the retries run out. Unit tests drive the retry helper with a closure that fails N times before succeeding, and assert that other error kinds surface immediately.

Execution: `io_retry` pauses 100, 150, 200, then 250 ms and gives up. It wraps the rename, the copy, and the restore rename/remove. The success-path removal of the backup is not retried: that file is the running image until this process exits, so every attempt fails the same way and would add about a second to every Windows upgrade. B60 sweeps it on the next self command. Tests: a `PermissionDenied` that clears on the third call, a `NotFound` that returns on the first call with no pause, and Windows raw errors 5 and 32 counting as transient.

Implemented in `src/self_update.rs`, wired into `doctor` from `src/cmd/system.rs`, and noted in `docs/TROUBLESHOOTING.md`. `cargo fmt` is clean. `cargo test --locked --bin ketch -- process:: self_update::` passed (28, 0 failed) after an Avast folder exception. `cargo clippy --all-targets` and the full suite have not run.

### B63. `self update` hangs forever when a spawned process never starts

Observed on Windows with Avast running: `ketch self update` fetched the release, then the process never returned. A second run never logged `checking` either. Both were stuck before any ketch code ran, or inside a wait that has no deadline: `Get-CimInstance Win32_Process` in the in-use listing (`offer_to_stop`), and `ketch --version` after `replace_binary` copies the new image. Spawning the new unsigned `ketch.exe` creates a process that stays in `Initialized` with zero CPU and survives `Stop-Process -Force`. The previous binary, copied aside and renamed back to `.exe`, starts immediately. Defender real-time protection was off; Avast's services were up.

Kimi Code (`session_12034d9b-d032-4fb3-92be-3154393b064f`) wrote the deadline and died on a compile error (`Command` is not `Default`). ZCode (`sess_64ed7f70-a5e3-465b-a237-76aa3320a65c`) fixed the by-value call sites; its `cargo test` produced a test binary that then hit the same loader stall and never executed.

Plan: keep the deadlines already in the tree. `process::run_bounded` runs the child off the caller's wait: after 30s a process listing is stopped and warned about by program and pid; after 60s `ketch --version` is stopped and the error says so, then restore puts the previous binary back. A spawn that never returns has no pid; the message says the wait was ended, and the spawn thread stops the child if creation later succeeds. Tests: the stopped-probe wording, a command that outlives a short budget and is no longer alive, and on Windows `tree.com` (exit 0) / `where.exe` (exit 1).

Verify: `cargo fmt` is clean, and `cargo test --locked --bin ketch -- process:: self_update::` passed (28, 0 failed), including the stopped-child cases. `cargo clippy --all-targets -- -D warnings` and the full suite have not run.

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

### F8. Spinner and progress bar

Show a spinner while a command is running so the user sees that it started. Use a progress bar where measurable progress is available, and a spinner elsewhere. Match the behavior in rtok.

### R3. Cross-platform CI

Run verification on macOS, Windows and Linux. A ketch config is either a local file in the project or pushed to a registry; keep that model. The cross-platform check must catch OS-specific binary selection bugs like the one in B64.

Current state: `ci.yml` has separate jobs on macOS, Linux and Windows running lint, the full nextest suite and the `tui`-feature tests on all three. Formatting and commit-message checks run on macOS only, PowerShell syntax on Windows only. The packaging matrix runs on all three OSes. `Swatinem/rust-cache` is already in every job. `verify.yml` mirrors the same three-OS checks before a release.

To add:

1. Binary selection regression test: see B65 below.
2. Both config paths, local file and registry: cover the select-mode prompt when the binary name is missing and several candidates match, and assert the chosen binary is written back into the config.
3. Caching: rust-cache is already in place. Also evaluate caching the mise toolchain and the target directories on all three OSes. Do this in any case, and base the decision on the before/after build-time numbers from the cox and ketch infra-template PRs.

### B65. Binary selection regression test

Add a fixture with two similarly named binaries (for example `rtok` and `rtok-hook`) and assert the intended one is chosen on every OS: macOS, Windows and Linux. This is the test that would have caught the Windows alphabetical-sort bug, where `rtok hook` was selected instead of the intended binary.

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

### Priorities

Set in the task table above (creator, 2026-09-27).
