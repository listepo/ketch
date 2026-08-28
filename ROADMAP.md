# Roadmap

What ketch does not do yet, and what it would take. Order is rough priority, not
a schedule.

## Linux and Windows

macOS is the only implemented platform. Everything OS-specific sits behind the
`Platform` interface in `packages/core/src/platform/`, and nothing above that
layer contains platform-specific code — the install pipeline, sources,
extractors and commands are already portable.

A Linux backend is one new file plus one branch in `hostPlatform()`, which
already refuses anything but `darwin` with a message pointing here:

- `scoreAsset` — read `linux`, `gnu`, `musl`, `x86_64`, `aarch64` from asset
  names, and prefer static builds when the host libc is uncertain.
- `place` / `unplace` — symlinks into the bin dir. No `.app` handling, so
  `appBundleExtension()` returns null and this is simpler than macOS.
- `verifyTrust` / `clearQuarantine` — there is no system-wide equivalent of
  `codesign` or the quarantine bit; `NotApplicable` and a no-op are honest
  answers, with signature verification arriving alongside the sigstore work
  below.
- `extractors` — the tar, zip and compression handling is already shared; only
  the macOS-specific `dmg`/`pkg` extractors drop off the list.
- `doctor` — is the bin dir on `PATH`, is the store writable.

Windows needs the same, plus `.exe` handling and a `place` that copies rather
than symlinks by default.

## Publishing to npm

`apps/cli` is `@listepo/ketch` and is still marked private. The release binary
that `install.sh` downloads is the supported way to get ketch and will stay
that way — but the package is a plain Node CLI with no native dependencies, so
`npx @listepo/ketch` and `pnpm add -g @listepo/ketch` are both a publish step
away for people who already live in that ecosystem.

What has to happen first: drop `private`, decide whether the published package
ships TypeScript sources (which Node runs directly) or a build, and make the
release workflow publish on the same tag that builds the binaries, so the two
can never report different versions.

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
today stay valid — but nothing is done with it yet. The intent is to link a
package's own completions into the shell's directory and its man pages onto
`MANPATH`, both undone cleanly on uninstall.

This is about *client apps*. ketch's own completions already work:
`ketch completions bash|zsh|fish` prints a script generated from the command
surface itself, so it cannot fall behind the commands.

## Registry maturity

The registry is a plain GitHub repository, one folder per package (see
[docs/REGISTRY.md](docs/REGISTRY.md)). It works, and it is deliberately dumb.

Still missing:

- **The registry's own migration to JSON.** Package files are `ketch.json` now,
  validated against a published JSON Schema. The registry repository
  (`listepo/ketch-registry`) still holds `ketch.toml` files and has to be
  converted — a mechanical rewrite of every entry plus a `$schema` line. That
  work belongs in that repository, not this one, and until it lands a fetched
  registry resolves nothing.
- **CI on the registry itself** — every `ketch.json` should be parsed against
  the published schema, validated, and test-installed before merge. The schema
  is generated and committed here precisely so the registry can consume it.
- **Name collisions** are reported as warnings on `ketch update`; they ought to
  be rejected at the registry, before anyone fetches them.
- **`ketch update` is manual by design** — no hidden network calls — but there is
  no way to ask "is my registry stale?" short of running it.

## Smaller things

- **Rollback.** Every version stays in the store, so `ketch rollback <pkg>`
  should be little more than a relink to an older prefix.
- **`ketch why <pkg>`.** Explain a resolution end to end: which tier the
  manifest came from, which release matched, which asset scored highest. Most of
  the pieces exist already, scattered across `ketch info --assets --verbose`.

## Deliberately out of scope

- **Building from source.** ketch installs what a project already publishes. If
  there is no release asset, that is the project's answer, not a gap to fill.
- **Dependency resolution.** These are self-contained release artefacts. A
  package manager that resolves a graph is a different program.
- **Running as root, or installing outside the ketch root.** Everything lives
  under `~/.ketch`, with `/Applications` the single documented exception.
