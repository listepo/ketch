# AGENTS.md

Notes for coding agents working in this repository. Humans are welcome to read
it too — nothing here is agent-specific except the framing.

## What ketch is

A single-binary CLI package manager that installs command-line tools and macOS
apps straight from GitHub releases. No taps, no formulae, no build step: it
downloads what a project already ships, verifies it, unpacks it into a store,
and links it onto `PATH`.

### Host app, client app

Two words used throughout this file and the code, because "app" alone is
ambiguous in a package manager:

- **host app** — ketch itself: this repository, the binary in `target/`, the
  thing being changed. Its own version, release process and `~/.ketch` tree are
  the host's.
- **client app** — anything ketch installs and manages: ripgrep, a `.app`
  bundle, whatever a `Manifest` names. It is written by someone else, so
  everything about it — asset names, archive members, `CHANGELOG.md`, release
  notes — is untrusted input, not ketch's own data.

Where the distinction matters most: `ketch self update` upgrades the host,
`ketch upgrade` upgrades clients; `scripts/release.sh` releases the host,
`ketch.lock` pins clients; `src/changelog.rs` reads a client's changelog, while
the host's history is git.

macOS is the only implemented platform. `src/platform/mod.rs` gates it with
`#[cfg(target_os = "macos")]` and returns a clear error elsewhere, so a Linux
backend means implementing the `Platform` trait — nothing above it changes.

## Commands

```bash
cargo test                       # unit tests and the end-to-end suite; no network
cargo test --test install        # just the end-to-end suite
cargo clippy --all-targets       # must be clean
cargo fmt                        # must be clean
cargo build                      # debug binary at target/debug/ketch
```

Run the binary against a throwaway tree instead of your real `~/.ketch`:

```bash
KETCH_ROOT=/tmp/ketch-scratch cargo run -- doctor
```

CI runs `fmt --check`, `clippy -D warnings` and `test` on macOS. All three must
pass before a change is done.

## Layout

| Path | Owns |
| --- | --- |
| `src/main.rs` | argument parsing, config construction, dispatch — nothing else |
| `src/cli.rs` | the clap surface, kept separate so `cmd/` takes its args directly |
| `src/cmd/` | thin command bodies: arguments, output, confirmations |
| `src/install.rs` | the install/uninstall/relink pipeline every command shares |
| `src/source/` | where releases come from: GitHub built in, plugins external |
| `src/extract/` | archive formats, selected by sniffing content not file names |
| `src/platform/` | OS-specific placement, linking, trust checks |
| `src/shell.rs` | putting the bin dir on PATH in bash, zsh and fish |
| `src/registry.rs` | the fetched package registry (see `docs/REGISTRY.md`) |
| `src/manifest.rs` | resolving a name to a `Manifest` across four tiers |
| `src/model.rs` | every type that crosses a module boundary |
| `src/state.rs` | the installed-package record and the process lock |
| `src/log.rs` | the log file, in text or JSON Lines |
| `src/changelog.rs` | finding and slicing a client app's changelog |
| `src/lockfile.rs` | `ketch.lock`: what is installed, pinned to exact releases |
| `src/ui.rs` | all terminal output |
| `tests/` | end-to-end tests that drive the real binary |
| `scripts/package.sh` | the release tarball, shared by CI and the release workflow |
| `scripts/release.sh` | the version bump and the release pull request |

The rule that keeps `cmd/` thin: anything touching the install tree belongs in
`install.rs`, `state.rs`, or a trait implementation, so the same logic serves
every command. If you are about to write install logic inside a command, you
are in the wrong file.

`src/shell.rs` is the one module that writes outside the ketch root, and it
does so only when asked: `ketch path install` and `ketch doctor --fix`. It edits
a shell startup file between two markers, so the block can be found again,
rewritten when the root moves, and removed without guessing which line was
ketch's. It follows a symlinked startup file to its target before writing,
because that file is very often a link into a dotfiles repository.

## Conventions

These are observed throughout; match them rather than introducing your own.

- **Every file opens with a `//!` header** saying what the module owns and why
  it exists separately. Every public item has a doc comment.
- **Comments explain *why*, never *what*.** The code already says what it does.
  A comment earns its place by recording a decision, a constraint, or a
  failure that motivated the shape of the code.
- **All output goes through `ui::`.** There is no `println!` outside `ui.rs`.
  Data goes to stdout via `ui::out`/`ui::table`; progress, warnings and errors
  go to stderr, so output can be piped. `ui.rs` is also the only caller of
  `log::record`, so a new command cannot forget to be logged, and a status line
  written any other way is invisible to whoever reads the log afterwards.
- **Errors are `crate::error::Error`**, built with `Error::msg`/`io`/`parse`.
  The `Result<T>` alias is from the same module.
- **No `unwrap`, `expect`, `panic!`, `todo!` or `unimplemented!` outside
  tests.** The two `unwrap`s in `model.rs` sit immediately after the `peek`
  that proves them; if you add one, prove it on the line above.
- **Tests live in `#[cfg(test)] mod tests` at the bottom of the file they
  test**, and are named as sentences: `latest_prefers_highest_stable`,
  `drafts_are_never_selected`. A test name should read as the claim it proves.
- **`tests/` is the exception**, and only for what a unit test cannot reach:
  the pipeline end to end, through the real binary. `tests/support/` builds a
  throwaway root, fixture archives and a source plugin that serves them, so the
  suite stays offline. Add a case there when a bug could pass every unit test
  in the tree — most of them could.
- **Best-effort where a partial answer beats no answer.** A broken plugin, an
  unreadable manifest or one unreachable source is warned about and skipped,
  never fatal. A malformed *built-in* registry is a ketch bug and does fail.

## Trust boundaries

Most of what ketch handles was written by someone else: GitHub API responses,
release asset names and bytes, archive member paths, registry `ketch.toml`
files, and source-plugin subprocess output — everything about a client app, in
other words. Anything from those reaching a filesystem path, a URL, a process
or the user's terminal is a trust boundary.

Reuse the guards that exist rather than writing new ones:

- `extract::safe_member_path` — rejects archive entries that escape the
  destination (`..`, absolute, Windows drive/stream syntax).
- `config::sanitize_component` — makes a string usable as one path component.
  To *reject* rather than rewrite, ask whether it changes the value; that is
  what `Manifest::validate` does, because a package that installs somewhere
  other than where it says is worse than one that refuses to install.
- `Manifest::validate` — the single guard every manifest tier passes through
  (registry, user manifests, built-in). Add new checks there, not at a caller.
- `config::validate_repo` — anything that becomes `github.com/owner/repo`.
- `changelog::sanitize` — drops escape sequences and bidi overrides from client
  prose before it is printed. A changelog is the one place ketch shows a whole
  file someone else wrote; an unfiltered one can rewrite the screen above it.

Simplicity never removes one of these. If a change makes a guard unnecessary,
delete the guard deliberately and say why in the commit.

## Adding things

- **A package that inference gets wrong** → an entry in the registry, or
  `src/builtin.toml` if it must work offline out of the box. `docs/MANIFESTS.md`
  is the schema; `docs/REGISTRY.md` is the folder-per-package layout.
- **A new archive format** → implement `Extractor` in `src/extract/`, add it to
  the platform's list. Detection sniffs content; do not trust the extension.
- **A new package source** → implement `Source`. Prefer an external plugin
  (`src/source/plugin.rs`) over a built-in one: it needs no recompile. The wire
  protocol is `docs/PLUGINS.md`; changing it means bumping `PROTOCOL_VERSION`.
- **A new command** → a variant in `cli.rs`, a thin body in `cmd/`, and the
  work itself in `install.rs` or a trait.
- **A field in `ketch.lock`** → `src/lockfile.rs`, and a row in
  `docs/LOCKFILE.md`. Anything a lockfile can say has to pass `validate`
  first: it is a file a colleague may have written.

## Releasing

```bash
scripts/release.sh 0.2.0          # --dry-run to see it first
```

Run it on a clean, up-to-date default branch. It bumps `Cargo.toml` and
`Cargo.lock` on a `release/v0.2.0` branch, pushes it, and opens a pull request
whose body lists the commits since the last tag and gives the tag command to
publish. It refuses a version that goes backwards, one that is already current,
and one written with a leading `v`; it rewrites only the version inside
`[package]`, and re-reads the result through `cargo metadata` before pushing, so
a bad rewrite fails with nothing published.

Merging the pull request does not release anything. Tagging the merge commit
does:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

The release workflow then re-runs the whole gate, refuses a tag that disagrees
with `Cargo.toml` (`ketch self update` compares the two, so a mismatched tag
breaks upgrades for everyone already installed), builds both macOS
architectures, and publishes the tarballs with an aggregate `SHA256SUMS`.

Bumping the version by hand is what `scripts/release.sh` exists to stop: the
version is written in one place and checked in two, and the two must agree.

Asset names are load-bearing: `install.sh` and `ketch self update` both look for
`ketch-<target>.tar.gz` and `SHA256SUMS`. Renaming either strips the upgrade
path from every copy already out there. CI runs the same `scripts/package.sh` on
every pull request so packaging breaks before a tag is pushed, not after.

## Before you call it done

1. `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` all clean.
2. Non-trivial logic left a test behind that fails if the logic breaks.
3. You ran the actual binary against a `KETCH_ROOT` scratch tree if the change
   touches installation, linking, or the registry.
4. You reported what you did *not* do, if anything was skipped.

## TypeScript rewrite (in progress on `ts-rewrite`) — latest info, 2026-08-28

The Rust tree in `src/` is the executable spec being ported; everything above
this heading describes it and still applies to reading it. The port lives in a
Moon monorepo: `packages/schemas` (Zod v4 schemas + generated JSON Schema for
every data file — TOML became JSON), `packages/core` (the pipeline and every
module `src/` had), `apps/cli` (Commander 15 + Clack UI). The site will be
`apps/web` (Astro 7 + Tailwind 4) and `apps/docs` (Docusaurus 3.10).

### Toolchain (pinned via mise.toml and package.json — check, don't assume)

| Tool | Version | Role |
| --- | --- | --- |
| TypeScript | 7.0.2 (native compiler) | strict everywhere; project references |
| Node.js | 26 | canonical runtime; CI runs the suite here |
| Bun | 1.3 | fastest runtime: the dev/agent test loop runs on it |
| Deno | 2.9 | supported runtime, smoke-tested in CI |
| Perry (`@perryts/perry`) | 0.5 | compiles the CLI to a native binary for releases |
| pnpm | 10 | dependency management (workspaces in pnpm-workspace.yaml) |
| Moon | 2.5 | task runner (`pnpm exec moon ci`) |
| Vitest | 4 | the whole suite; colocated `*.test.ts`, names are sentences |
| oxlint | 1.80 | the linter (no ESLint) |
| Biome | 2.5 | the formatter (no Prettier) |
| Zod | 4 | schemas; `z.toJSONSchema` emits the published JSON Schemas |
| pino | 10 | the log file (JSON Lines native; pino-pretty for text) |

### Rules that keep the port honest

- Runtime portability is a feature: `node:` builtins only, no Bun/Deno/Perry
  specific APIs anywhere. Run tests on Bun because it is fastest, never
  because something only works there.
- JSON field names match the Rust serde names byte-for-byte: the TS binary
  must read a `state.json` the Rust binary wrote.
- All trust-boundary guards survive the port by name: `safeMemberPath`,
  `sanitizeComponent`, `validateRepo`, changelog `sanitize`, manifest and
  lockfile validation.
- Gates before done: `tsc --build`, `oxlint`, `biome format`, `vitest run` —
  all clean, run for every package you touched.

### Delegating work to agents

Match the model to the thinking the task needs: **Fable 5 or Opus 5** for
work that requires real reasoning (pipeline concurrency, parsers, security
boundaries, architecture); **Sonnet** for straightforward well-specified
jobs; **Haiku** for mechanical ones. When a "simple" task turns out to need
thinking, escalate the model rather than accepting a shallow result.
