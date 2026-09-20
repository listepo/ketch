# Roadmap

What ketch does not do yet, and what it would take. Order is rough priority, not
a schedule.

## Linux and Windows

Shipped: `src/platform/linux.rs` and `src/platform/windows.rs`, selected from
`platform::host()`. Linux places bin-dir symlinks; Windows copies into the bin
dir and records `CopiedFile`.

Publisher signature checks (below) run on every platform. The platform-local
trust verdict stays macOS-only: elsewhere `verify_trust` reports
`NotApplicable`, because only macOS exposes a system code-signature check
to ask.

`ketch path install` on Windows writes the user PATH. `install.sh` fetches the
host tarball on macOS, Linux and Windows (Git Bash). `release.yml` publishes
Linux and Windows tarballs next to the signed macOS ones. CI runs each OS's
suite on that OS.

## Verifying signatures

Shipped as M3 (`src/trust.rs`). A checksum proves the file was not corrupted;
a signature says who published it. The manifest's `trust` table declares the
expected identity where the package is declared rather than assuming it at
install time, and `Manifest::validate` refuses a broken policy before anyone
tries to install it:

- **sigstore** — a bundle checked offline against the public-good trusted root
  embedded in ketch, including the Rekor signed entry timestamp that makes the
  log's timestamp (and so the certificate's validity window) mean anything.
- **minisign** — against the key the manifest pins.
- **GPG detached signatures** — against the key block the manifest carries,
  pinned by its full fingerprint. No keyring is ever read: a key imported for
  some other reason is not this publisher's authorisation.

Verification fails closed unless `mode = "warn"`. The result lands in
`ketch info` (text and JSON), the install report, and the log — reporting what
the manifest pinned, never what a signature file says about itself. See
[docs/MANIFESTS.md](docs/MANIFESTS.md) for the `trust` table.

## Man pages and shell completions

Shipped as M4 (`src/extra.rs`). `extra_paths` entries are classified as a man
page or a completion from explicit `{ path, kind }` metadata or the path rules
in [docs/MANIFESTS.md](docs/MANIFESTS.md); ambiguous or untyped entries are
refused at validate rather than guessed. They resolve only under the extracted
payload and land in user directories (the man root and per-shell completion
directories that `ketch doctor` reports), recorded in state so uninstall and
relink remove them with the same ownership proof as binaries.

## Registry maturity

Partially shipped (M5, `ketch registry validate`) and deliberately capped. The
registry is a plain GitHub repository, one folder per package (see
[docs/REGISTRY.md](docs/REGISTRY.md)). It works, and it is deliberately dumb.

What exists:

- **`ketch registry validate`** — every `ketch.toml` parsed, checked against
  its folder name, refused when `source` is `local:`, passed through
  `Manifest::validate`, with name and alias collisions failing the run. The
  `--fixture` / `--changed` flags additionally offline-install changed entries
  in a throwaway root, so a `github:` source never reaches the network.
- **Name collisions** — fatal in `validate`, so they cannot land; already
  published ones stay warnings on `ketch update`, because a client must keep
  resolving the rest of the tree rather than refuse it whole.
- **Staleness** — `ketch registry status` and the `ketch doctor` registry line
  already report the local copy's age and source from `registry.meta.toml` with
  no network call. `ketch update` remains the only refresh, by design.

Dropped: CI on the registry itself (plan.md F2). The ketch-registry repository
deliberately removed its only workflow (commit `5a9bbd6`, "no CI is wanted in
this repo"), so there is no upstream to land a validating workflow in. The
ketch-side answer is local validation before push — see
[docs/REGISTRY.md](docs/REGISTRY.md) for the exact command and a pre-push hook.

## Smaller things

- **Rollback.** Shipped. An upgrade keeps the previous prefix; `ketch rollback <pkg>` relinks it with no redownload. `ketch prune` drops prefixes beyond the retention policy.
- **`ketch why <pkg>`.** Shipped as M7 (`src/resolve.rs`, `ketch why`). Explains a resolution end to end without installing: which tier the manifest came from, which release matched, which asset scored highest — and which assets and releases were rejected, and why.

## Deliberately out of scope

- **Building from source.** ketch installs what a project already publishes. If
  there is no release asset, that is the project's answer, not a gap to fill.
- **Dependency resolution.** These are self-contained release artefacts. A
  package manager that resolves a graph is a different program.
- **Running as root, or installing outside the ketch root.** Everything lives
  under `~/.ketch`, with `/Applications` the single documented exception.
