# ketch
https://github.com/listepo/ketch
Catch releases straight from GitHub — a package manager for GitHub-released binaries and apps.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| F1 | done (ketch side) | P2 | 3 | 100% | Cursor / grok 4.6 |
| F2 | dropped (upstream declined) | P2 | 3 | — | Cursor / grok 4.6 |
| M3 | done | P2 | 5 | 100% | Claude Code / claude-opus-5 |
| F5 | done (ketch side) | P1 | 3 | 100% | Cursor / grok 4.6 |

### F1. Notarisation

The release binaries are signed with a Developer ID but not notarised, which is fine for a `curl`-fetched tarball and not fine the day ketch ships anything a browser downloads. Needs an App Store Connect key, `xcrun notarytool submit --wait` in the build job, and a stapled check in the smoke test.

Plan (Claude Code / claude-opus-5), prepared without a key: none exists yet, so everything ships switched off.

1. `release.yml` build job: a `Notarise` step for the two signed targets, gated on the repository variable `KETCH_NOTARIZE == 'true'`. Once on, it fails on a missing `APPSTORE_CONNECT_KEY` (the `.p8`, base64), `APPSTORE_CONNECT_KEY_ID` or `APPSTORE_CONNECT_ISSUER_ID`, the same way a missing certificate fails. It zips the packed binary with `ditto`, runs `xcrun notarytool submit --wait`, and fails unless the status is `Accepted`, printing the notary log.
2. Smoke test, under the same flag: `spctl --assess --type install` must report `source=Notarized Developer ID`. A bare Mach-O cannot be stapled (`stapler` takes bundles, disk images and packages only), so the check relies on Gatekeeper's online ticket lookup, not a stapled ticket.
3. `AGENTS.md` Releasing: document the switch and the three secrets.
4. Check: the workflow parses and `just check` is clean. The first real run needs the key; switching it on is the creator's step: add the secrets, set the variable, and run a release.

Plan (Cursor / grok 4.6): done on the ketch side. The Notarise step, smoke `spctl` gate, and AGENTS.md switch already exist. `tests/release-yml-notarize.sh` (YAML parse + load-bearing strings) is wired into `just lint-shell` and CI. Remaining step is the creator's: add the secrets, set `KETCH_NOTARIZE=true`, and run a release. Verified: script passes, `cargo fmt --check` and `cargo clippy --all-targets` clean.

### F2. Registry CI in ketch-registry

`ketch registry validate` exists in this repo (tree checks, name/alias collisions, optional `--fixture` / `--changed` offline-install). The ketch-registry repository still needs a workflow that runs that command on every pull request.

Dropped: ketch-registry deliberately removed its only workflow (commit `5a9bbd6`, "no CI is wanted in this repo"), so there is no upstream to land this in. Ketch side stays as is — the validator, docs, and fixture flow are the deliverable.

### M3. Provenance and signatures — done

`trust` table on Manifest (verifier sigstore|minisign|gpg, mode require|warn, signature/signed sidecar templates, issuer + repository/identity, public_key, fingerprint), checked in `Manifest::validate`; docs/MANIFESTS.md. `InstalledPackage.provenance` with old/new state tests. `src/trust.rs`: sigstore offline against an embedded trusted root plus a Rekor SET check, minisign-verify with the pinned key, pgp with an inline key pinned by fingerprint (never a keyring); fail closed unless `mode = "warn"`. Results in `info` (text + JSON), the install report and the log, identities sanitised. Fixtures in tests/fixtures/trust, unit tests in trust.rs, e2e in tests/trust.rs. Deps in Cargo.toml: sigstore, minisign-verify, pgp. Verified present in tree (`src/trust.rs`, `TrustPolicy`/`Provenance` in model.rs, install wiring, `trust` docs section).

### F5. Config reset and shared file backup — done (ketch side)

`ketch config reset` writes `config.toml` with compiled defaults after confirming. Existing file is backed up beside itself as `config.toml.bak-<unix-seconds>` via the shared `packages/file-backup` crate (missing file or byte-identical sibling backup → no copy). No daemon — ketch has none.

Done in this change: `ConfigCommand::Reset { yes }` in `src/cli.rs`, `Config::default_toml()` in `src/config.rs`, `reset()` in `src/cmd/config.rs`, `file-backup` path dep in `Cargo.toml` (+ lockfile), e2e in `tests/config_reset.rs` (defaults + backup, missing file, confirm gate), docs in `README.md` Configuration and `docs/COMMANDS.md`. Verified: `cargo fmt --check` clean, `cargo clippy --all-targets` clean, `config_reset` + `config_create` suites green (14 passed).

Not done (out of ketch scope, needs rtok owner): `packages/file-backup` already exists standalone with its own tests; `rtok-agent-sdk::backup` still has its own `_backup/`-dir copy and does not re-export the shared crate — plan step 1's "move the rtok backup tests there / re-export through anyhow" is a rtok-side change.


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
