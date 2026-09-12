# Roadmap

What ketch does not do yet, and what it would take. Order is rough priority, not
a schedule.

## Linux and Windows

Shipped: `src/platform/linux.rs` and `src/platform/windows.rs`, selected from
`platform::host()`. Linux places bin-dir symlinks; Windows copies into the bin
dir and records `CopiedFile`. Trust is `NotApplicable` until signature
verification exists.

`ketch path install` on Windows writes the user PATH. `install.sh` fetches the
host tarball on macOS, Linux and Windows (Git Bash). `release.yml` publishes
Linux and Windows tarballs next to the signed macOS ones. CI runs each OS's
suite on that OS.

## Verifying signatures

ketch verifies checksums today: a published SHA-256 is compared against what
landed on disk, and `require_checksums` refuses installs that publish none. That
proves the file was not corrupted, not that the project published it.

Wanted, roughly in order of how often projects actually use them:

- **sigstore / cosign bundles** — `.sigstore` and `.intoto.jsonl` sidecars are
  already recognised and skipped as non-payload; verifying them means checking a
  transparency-log entry and an identity, not just a hash.
- **minisign / signify** — small, common among CLI projects, and verifiable with
  a pinned public key in the manifest.
- **GPG detached signatures** (`.asc`) — widespread but only meaningful with a
  trust path to the key, which is the hard part.

The manifest would carry the expected identity, so trust is declared where the
package is declared rather than assumed at install time.

## Man pages and shell completions

`extra_paths` is already parsed, validated and recorded — manifests written
today stay valid — but nothing is done with it yet. The intent is to link
completions into the shell's own directory and man pages onto `MANPATH`, both
undone cleanly on uninstall.

## Registry maturity

The registry is a plain GitHub repository, one folder per package (see
[docs/REGISTRY.md](docs/REGISTRY.md)). It works, and it is deliberately dumb.

Still missing:

- **CI on the registry itself** — every `ketch.toml` should be parsed,
  validated, and test-installed before merge.
- **Name collisions** are reported as warnings on `ketch update`; they ought to
  be rejected at the registry, before anyone fetches them.
- **`ketch update` is manual by design** — no hidden network calls — but there is
  no way to ask "is my registry stale?" short of running it.

## Smaller things

- **Rollback.** Every version stays in the store, so `ketch rollback <pkg>`
  should be little more than a relink to an older prefix.
- **`ketch why <pkg>`.** Explain a resolution end to end: which tier the
  manifest came from, which release matched, which asset scored highest.

## Deliberately out of scope

- **Building from source.** ketch installs what a project already publishes. If
  there is no release asset, that is the project's answer, not a gap to fill.
- **Dependency resolution.** These are self-contained release artefacts. A
  package manager that resolves a graph is a different program.
- **Running as root, or installing outside the ketch root.** Everything lives
  under `~/.ketch`, with `/Applications` the single documented exception.
