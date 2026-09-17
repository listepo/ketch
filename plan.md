# ketch
https://github.com/listepo/ketch
Catch releases straight from GitHub — a package manager for GitHub-released binaries and apps.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| F1 | in progress | P2 | 3 | 95% | Cursor / grok 4.6 |
| F2 | in progress | P2 | 3 | 70% | Cursor / grok 4.6 |
| M3 | in progress | P2 | 5 | 95% | Claude Code / claude-opus-5 |

### F1. Notarisation

The release binaries are signed with a Developer ID but not notarised, which is fine for a `curl`-fetched tarball and not fine the day ketch ships anything a browser downloads. Needs an App Store Connect key, `xcrun notarytool submit --wait` in the build job, and a stapled check in the smoke test.

Plan (Claude Code / claude-opus-5), prepared without a key: none exists yet, so everything ships switched off.

1. `release.yml` build job: a `Notarise` step for the two signed targets, gated on the repository variable `KETCH_NOTARIZE == 'true'`. Once on, it fails on a missing `APPSTORE_CONNECT_KEY` (the `.p8`, base64), `APPSTORE_CONNECT_KEY_ID` or `APPSTORE_CONNECT_ISSUER_ID`, the same way a missing certificate fails. It zips the packed binary with `ditto`, runs `xcrun notarytool submit --wait`, and fails unless the status is `Accepted`, printing the notary log.
2. Smoke test, under the same flag: `spctl --assess --type install` must report `source=Notarized Developer ID`. A bare Mach-O cannot be stapled (`stapler` takes bundles, disk images and packages only), so the check relies on Gatekeeper's online ticket lookup, not a stapled ticket.
3. `AGENTS.md` Releasing: document the switch and the three secrets.
4. Check: the workflow parses and `just check` is clean. The first real run needs the key; switching it on is the creator's step: add the secrets, set the variable, and run a release.

Plan (Cursor / grok 4.6): remaining Check. The Notarise step, smoke `spctl` gate, and AGENTS.md switch already exist. Add `tests/release-yml-notarize.sh` (YAML parse + load-bearing strings), wire into `just lint-shell` and CI, run `just check`, then move to done.md. Switching `KETCH_NOTARIZE` on stays the creator's step.

### F2. Registry CI in ketch-registry

`ketch registry validate` exists in this repo (tree checks, name/alias collisions, optional `--fixture` / `--changed` offline-install). The ketch-registry repository still needs a workflow that runs that command on every pull request.

### M3. Provenance and signatures

Plan (Claude Code / claude-opus-5): `trust` table on Manifest (verifier sigstore|minisign|gpg, mode require|warn, signature/signed sidecar templates, issuer + repository/identity, public_key, fingerprint), checked in `Manifest::validate`; docs/MANIFESTS.md + `site/sync-docs.py`. Optional `InstalledPackage.provenance` with old/new state tests. New `src/trust.rs`: sigstore-rs offline against an embedded trusted root plus a Rekor SET check, minisign-verify with the pinned key, pgp with an inline key pinned by fingerprint (never a keyring); fail closed unless `mode = "warn"`. Results in `info` (text + JSON), the install report and the log, identities sanitised. Fixtures in tests/fixtures/trust, unit tests in trust.rs, e2e in tests/trust.rs. Verify: `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`. Crates: sigstore, minisign-verify, pgp. No commit.

Signatures and provenance for release assets so install can verify more than a checksum sidecar.


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
