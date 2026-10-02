# AGENTS.md

Notes for coding agents working in this repository. Humans are welcome to read
it too — nothing here is agent-specific except the framing and the rule below.

## Mandatory for every agent

- The human is the only author. No agent adds a Co-Authored-By trailer, a
  "Generated with …" line or itself as author to a commit, merge or PR —
  whatever its harness defaults to.
  `no-agent-attribution` in `commitlint.config.mjs` rejects such a trailer or
  line in every commit a pull request brings, and in the commit-msg hook. A
  pull request description is not checked — that part is on the agent.
- If a directory above this repository contains an `AGENTS.md` or
  `CLAUDE.md`, follow it too. If it conflicts with this file, ask the creator.

## What ketch is

A single-binary CLI package manager that installs command-line tools and apps
on macOS, Linux, and Windows straight from GitHub releases. No taps, no formulae, no build step: it
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

Where the distinction matters most: `ketch self upgrade` upgrades the host,
`ketch upgrade` upgrades clients; `scripts/release.sh` releases the host,
`ketch.lock` pins clients; `crates/ketch-core/src/changelog.rs` reads a
client's changelog, while the host's own `CHANGELOG.md` is written for it by
git-cliff. The host is
also a client of itself: `ketch self install` records it in `state.json` as
the package `ketch`, and the root `ketch.toml` is its manifest.

macOS, Linux and Windows each have a `Platform` backend in
`crates/ketch-core/src/platform/`. `host()` selects it. Another OS still means implementing that trait — nothing
above it changes. End-to-end tests are gated with `#[cfg(target_os = "...")]`
so `cargo test` on a host runs that OS's suite. CI runs that suite on macOS,
Linux and Windows.

## Commands

```bash
cargo nextest run --workspace    # unit tests and the end-to-end suite; no network
cargo nextest run -E 'binary(install)'  # just the end-to-end suite
cargo clippy --workspace --all-targets  # must be clean
cargo fmt --all                  # must be clean
cargo build                      # debug binary at target/debug/ketch
```

The repository is a Cargo workspace of three crates: the root package `ketch`
(the binary), `crates/ketch-core` (the library it is built on) and
`crates/ketch-ffi` (the core exported through UniFFI for the desktop apps).
All are default members, so a bare `cargo test` or `cargo clippy` at the root
covers them; `--workspace` says so explicitly, and is what the Justfile and CI
pass. `just xcframework` builds `ketch-ffi` into the macOS app's XCFramework
and Swift bindings; `just ffi-test` builds a debug one and runs its Swift test.

The Justfile wraps the same commands with `--locked`: `just fmt`, `just clippy`
(or `just lint`), `just test`, and `just check` runs what CI runs on this
host — format, clippy, `cargo nextest run --workspace --all-targets`, commitlint
fixtures, shell syntax on `install.sh` and the release scripts, `mandoc -Tlint`
on the generated man pages when mandoc is present, whether `release.yml` is what
`dist generate` produces, `dist build` for the host target, and on macOS
`brew style` on the generated cask. Cross-target
builds and the Linux/Windows jobs are CI-only.

`just test` ends with a lossless `swarfr` cleanup of this checkout's cargo
`target/` dirs (compress + dedupe, never deletes); a machine without `swarfr`
just gets a note to install it, not a failure.

Commitlint checks commit messages against the conventional-commit format:

```bash
just deps             # one-time: mise install + npm ci
just hooks            # opt in to the commit-msg hook
just lint-commits     # commitlint over fixture messages; part of just check
```

The commit-msg hook is opt-in via `just hooks`: commitlint then runs on every
`git commit`, rejecting a malformed subject outright and letting merge and
revert subjects through.

`file-backup` comes from crates.io (pinned in `Cargo.lock`). For local work
against the sibling checkout, `just setup` writes a gitignored
`.cargo/config.toml` with a `paths` override pointing at
`packages/crates/file-backup`. CI and release builds never run it, so they
always resolve the registry version. A `paths` override — not a `[patch]` —
swaps the source without touching `Cargo.lock`, so `--locked` builds work on
the same committed lock in both directions.

Run the binary against a throwaway tree instead of your real `~/.ketch`:

```bash
KETCH_ROOT=/tmp/ketch-scratch cargo run -- doctor
```

## Rust CLI testing

Keep fast, deterministic business-logic tests beside the Rust module they
exercise. Put binary-level behavior in `tests/`, using `assert_cmd` and
`assert_fs`; use `trycmd` for commands whose complete output is easier to
review as a fixture. Use `rstest` for a genuine matrix of equivalent cases,
`insta` for stable reviewed snapshots, and `pretty_assertions` for rich value
diffs. Combine `predicates` with `assert_cmd` rather than parsing output
manually. Every bug fix should add the narrowest regression test that would
fail without the fix.

CI runs `fmt --check`, `clippy -D warnings` and `test` on macOS, and clippy
plus `test` on Linux and Windows. All of those must pass before a change is
done. The workflow runs on pushes to `main` and on pull requests targeting
`main` — drafts are skipped until marked ready. A `/review` comment in a pull
request (owner, member or collaborator) dispatches CI fresh on the PR's
branch; `workflow_dispatch` does the same by hand:

```bash
gh workflow run ci.yml --ref <branch>
gh run watch   # or: gh run list --workflow=ci.yml --branch <branch> -L 1
```

Merge to `main` only when that run succeeded. A failed push to `main` is
reverted by the auto-revert path described in the changelog / recent fixes;
do not rely on that — verify on the branch first.

## Installing tools

No program is installed system-wide to get work done: no `brew install`, no
`curl | bash`, no `sudo`. Anything a task needs that the machine lacks gets
pinned in `mise.toml` under `[tools]` and fetched with `mise install`, so the
pin is a reviewed change like any other and every machine — and every agent —
runs the same version. `just deps` installs everything currently pinned. A
tool that is a crate dependency belongs in `Cargo.toml` instead, not here.

## Cargo cache maintenance

`cargo-cache` is a developer utility, not a crate dependency, and it is pinned
in `mise.toml` so every machine runs the same one; so is the Node that
commitlint runs on. `mise install` fetches both.

The Rust toolchain is pinned there too, with rustfmt and clippy: `mise.toml` is
the one source of the Rust version for local work, every CI workflow and the
release build, which all install it with `jdx/mise-action`. To move to a new
Rust, bump the `rust` entry in `mise.toml` and nothing else, then run `just
dist-generate` if `release.yml` changes. The MSRV (`rust-version` in
`Cargo.toml`, 1.86) is a separate promise and does not follow the pin.

```bash
just cache            # $CARGO_HOME sizes and the build output, no deletes
just cache-dry-run    # preview removal of source and git checkouts
just cache-autoclean  # remove source and git checkouts
```

Run `just cache-dry-run` before any cleanup. `just cache-autoclean` is
destructive but safe for build correctness: Cargo will download sources again
when needed. Do not remove registry indexes or all cached data unless the task
explicitly requires reclaiming that space.

`just cache` reports the build output separately because `cargo-cache` does not
count it, and on a machine that redirects `build.target-dir` the two figures
differ by orders of magnitude — the cargo home is the small one. Set
`CARGO_CACHE=cargo-cache` to bypass mise if you have it activated already.

## Layout

| Path | Owns |
| --- | --- |
| `Cargo.toml` | the `ketch` package, and the workspace: members, the one shared version, edition, MSRV and lints |
| `src/main.rs` | argument parsing, config construction, dispatch — nothing else |
| `src/lib.rs` | empty except under `cfg(fuzzing)`: the same modules again, and the entry points `fuzz/` drives |
| `src/cli.rs` | the clap surface, kept separate so `cmd/` takes its args directly |
| `src/cmd/` | thin command bodies: arguments, output, confirmations |
| `src/complete.rs` | completion scripts, and `ketch __complete`: the package names they ask for at <TAB>, for every shell |
| `src/man.rs` | the host's own man pages, one per visible command, rendered from `Cli::command()` |
| `src/self_docs.rs` | hands `man.rs` and `complete.rs` to the core as `SelfDocs`, for `ketch self` to place |
| `crates/ketch-core/` | the library: everything besides the command line (see its `README.md`) |
| `crates/ketch-core/src/lib.rs` | which core modules the binary may reach (`pub`) and which stay inside (`pub(crate)`) |
| `crates/ketch-core/src/install.rs` | the install/uninstall/relink pipeline every command shares |
| `crates/ketch-core/src/hooks.rs` | a manifest's `[hooks]` commands, run by `install.rs` around install, update and uninstall — and only from a user-tier manifest |
| `crates/ketch-core/src/resolve.rs` | side-effect-free resolution trace shared by install and `ketch why` |
| `crates/ketch-core/src/bin_choice.rs` | which binary to link when several share the package's name, the same on every OS |
| `crates/ketch-core/src/source/` | where releases come from: GitHub built in, plugins external |
| `crates/ketch-core/src/extract/` | archive formats, selected by sniffing content not file names |
| `crates/ketch-core/src/platform/` | OS-specific placement, linking, trust checks (`macos.rs`, `linux.rs`, `windows.rs`) |
| `crates/ketch-core/src/shell.rs` | putting the bin dir on PATH in bash, zsh and fish, and on Windows the user environment; on Windows also the PowerShell profile blocks and cmd AutoRun that switch completion on |
| `crates/ketch-core/src/registry.rs` | the fetched package registry (see `docs/REGISTRY.md`) |
| `crates/ketch-core/src/manifest.rs` | resolving a name to a `Manifest` across four tiers |
| `crates/ketch-core/src/model.rs` | every type that crosses a module boundary |
| `crates/ketch-core/src/state.rs` | the installed-package record and the process lock |
| `crates/ketch-core/src/stats.rs` | `stats.db`: the history of what was installed, in SQLite |
| `crates/ketch-core/src/log.rs` | the log file, in text or JSON Lines |
| `crates/ketch-core/src/changelog.rs` | finding and slicing a client app's changelog |
| `crates/ketch-core/src/lockfile.rs` | `ketch.lock`: what is installed, pinned to exact releases |
| `crates/ketch-core/src/listing.rs` | `ketch list`: installed and registry packages merged, `latest` looked up in parallel and cached |
| `crates/ketch-core/src/push.rs` | `ketch registry push`: a project's `ketch.toml` as a registry pull request, via octocrab |
| `crates/ketch-core/src/self_update.rs` | `ketch self`: installing, updating and removing the host as a package |
| `crates/ketch-core/src/report.rs` | how the core says what happens: `Event`, `Reporter`, the `Report` handle, `Ctx`, `LogReporter` and `Recorder` |
| `crates/ketch-core/src/decide.rs` | what the core asks a person mid-run: the `Decider` trait and `NoDecider`, carried in `Ctx` |
| `crates/ketch-core/src/text.rs` | byte counts and truncation, spelled the same by the core and every renderer |
| `crates/ketch-core/src/doctor.rs` | `ketch doctor`'s checks, shared by the command and `ketch-ffi` |
| `crates/ketch-ffi/` | the core through UniFFI: a coarse, language-neutral surface of plain records, foreign traits for `Reporter` and `Decider` passed with a cancel token to each call, and a typed `KetchError` |
| `scripts/xcframework.sh` | `ketch-ffi` as an XCFramework for both macOS architectures, and its generated Swift bindings; `just xcframework` |
| `desktop/macos/KetchCore/` | the Swift package wrapping that XCFramework and bindings (both build output), and the Swift test that drives the real core through them |
| `src/ui.rs` | all terminal output, `Terminal`: the `Reporter` that draws the core's events, and `TerminalDecider`: the `Decider` that prompts on the terminal |
| `src/tui/` | the opt-in full-screen renderer (`tui` feature), driven by `ui.rs` |
| `crates/ketch-core/src/builtin.toml` | the manifests compiled into the binary, the offline registry tier |
| `crates/ketch-core/migrations/` | the `stats.db` schema, embedded by `stats.rs` |
| `ketch.toml` | the host's own package file, what `ketch registry push` sends |
| `tests/` | end-to-end tests that drive the real binary |
| `fuzz/` | cargo-fuzz targets, its own workspace on nightly; `just fuzz`, see `fuzz/README.md` |
| `dist-workspace.toml` | what cargo-dist builds, signs and publishes; the source of `release.yml` |
| `scripts/dist-generate.sh` | `dist generate` plus the patches to `release.yml` dist has no setting for |
| `.github/build-setup.yml`, `.github/build-check.yml` | steps dist splices into each release build: before it, and before upload |
| `.github/workflows/release.yml` | generated by dist; builds every target, then tags and publishes the release |
| `.github/workflows/bump.yml` | the only release path (pyrlyn/infra `bump.yml`): version PR, required checks, rebase merge, tag, draft release, dispatch |
| `.github/workflows/tap.yml` | dist's publish job: the Homebrew cask, pushed to the tap |
| `scripts/release.sh` | the one place a release version is decided; `just release` |
| `release-plz.toml` | release-plz kept from tagging, releasing or publishing (local preview only) |
| `cliff.toml` | the `CHANGELOG.md` entry format, for release-plz and `scripts/release.sh` alike |
| `plan.md` | what is being built next, and what each piece would take |
| `scripts/cask.sh` | the Homebrew cask, generated into `pyrlyn/homebrew-tap` on release |
| `install.sh` | the `curl | bash` installer for macOS and Linux; only bootstraps `ketch self install` |
| `install.ps1` | the `irm | iex` installer for Windows; same bootstrap as `install.sh` |
| `.github/dependabot.yml` | weekly `chore(deps)` pull requests for cargo, npm and GitHub Actions; not `mise.toml` |
| `desktop/macos/` | the SwiftUI macOS app: `project.yml` (XcodeGen), `Ketch/` sources, `KetchTests/`, `KetchUITests/`; see its `README.md` |
| `desktop/macos/DESIGN.md` | the macOS app's design system in the DESIGN.md format; its front matter is generated |
| `desktop/macos/design/` | `tokens.json`, the one source of design tokens, and `build.mjs`, which generates `generated/Tokens.swift`, the DESIGN.md front matter and `preview.html`'s CSS (`just design-tokens`) |
| `.github/workflows/desktop-release.yml` | the macOS app's release: signed, notarised `.dmg` under a `desktop-v*` tag, and its Sparkle appcast |
| `scripts/desktop-version.sh`, `scripts/desktop-dmg.sh`, `scripts/desktop-appcast.sh` | the app release's version check, disk image and appcast, shared with `tests/desktop-appcast.sh` |
| `desktop/cliff.toml` | the app's release notes: commits under `desktop/` and `crates/ketch-ffi/` since the last `desktop-v*` tag |

The rule that keeps `cmd/` thin: anything touching the install tree belongs in
`install.rs`, `state.rs`, or a trait implementation, so the same logic serves
every command. If you are about to write install logic inside a command, you
are in the wrong file.

The same rule decides the crate. The binary keeps what only a command line
needs: `main.rs`, `cli.rs`, `cmd/`, `complete.rs`, `man.rs` and
`self_docs.rs`. Every other module lives in `ketch-core`, and nothing in the
core may name the binary — when the core needs something only the CLI can produce, the binary
passes it in, as `SelfDocs` does. The binary imports the core's modules at its
crate root under their old names, so `crate::config` and `crate::ui` still
resolve in `cmd/`. A core item the binary calls is `pub`; everything else
stays `pub(crate)`. The core has one version, the workspace's, because it
reports it: `ketch-core` is `publish = false` and dist skips it, and
`tests/workspace.rs` fails if either promise drifts.

Several things write outside the ketch root. `ketch self install`, the bootstrap
installers and the Homebrew cask each place a bootstrap binary outside it.
`crates/ketch-core/src/platform/` links `.app` bundles into `/Applications` (or
`KETCH_APPS_DIR`), and man pages and completions into the user directories
`ketch doctor` reports; those destinations are recorded in state so uninstall
can take them back. `crates/ketch-core/src/shell.rs` edits shell startup files
and the user PATH only when asked: `ketch path install`, `ketch doctor --fix` and `ketch self
uninstall`, which takes the block back out of every startup file that has one
rather than only the shell running now, and on Windows takes the bin dir out of
the user PATH. On Windows, `self install` and `completions --install` also
put a completion block in the PowerShell profiles and append ketch's doskey
line to cmd's `AutoRun`; `self uninstall` takes back exactly those. It edits a shell startup file between two
markers, so the block can be found again, rewritten when the root moves, and
removed without guessing which line was ketch's. It follows a symlinked
startup file to its target before writing, because that file is very often a
link into a dotfiles repository. On Windows `ketch path install` writes
`HKCU\Environment\Path` via `[Environment]::SetEnvironmentVariable` so a new
terminal sees it without a logoff; `setx` is not used, because it truncates.

## macOS app

`desktop/macos/` is a SwiftUI app (Swift 6, strict concurrency, macOS 26)
described by XcodeGen's `project.yml`; the generated `Ketch.xcodeproj` and
`build/` are gitignored. `just macos-app` builds it unsigned for arm64 and
x86_64, `just macos-test` runs the Swift Testing unit tests and the UI smoke
test; the `macos-app` CI job runs the same plus `swift format lint --strict`.
It has no Rust in it and does not touch the CLI gate.

Views talk to `KetchStore`, the store talks to `KetchCoreProtocol`, and
`CoreFactory.swift` alone decides which core that is — `FakeKetchCore`
until `ketch-ffi` (R9) exists. Keep it that way: no view or test reaches past
the protocol. The UI uses system Liquid Glass (`glassEffect`, glass button
styles), never a drawn imitation. The app updates itself with Sparkle
(`Ketch/Store/AppUpdater.swift`, "Check for Updates…" in the app menu); only
a Release build with a real `SUPublicEDKey` starts it. Releases are
[Releasing → macOS app](#macos-app-releases). `desktop/macos/README.md` has
the architecture and the steps to wire R9.

## Conventions

These are observed throughout; match them rather than introducing your own.

- **Every file opens with a `//!` header** saying what the module owns and why
  it exists separately. Every public item has a doc comment.
- **A generated file says so in its first lines**, and the generator writes
  that header, not a person or a second script: `ketch lock` for `ketch.lock`,
  `site/sync-docs.py` for `site/content/docs/`, `scripts/cask.sh` for the
  tap's `Casks/ketch.rb`, `desktop/macos/design/build.mjs` for `Tokens.swift`
  and the generated blocks of `DESIGN.md` and `preview.html`. To change such a
  file, change its generator.
- **Comments explain *why*, never *what*.** The code already says what it does.
  A comment earns its place by recording a decision, a constraint, or a
  failure that motivated the shape of the code.
- **The core reports; only `ui.rs` prints.** `ketch-core` prints nothing and
  never calls `ui::` — `ui.rs` lives in the binary. Code in the core takes a
  `report::Ctx` (config plus `Report`) or a `&Report` and says what happens as
  typed `report::Event`s: `stage`, `step`, `success`, `warn`, `note`, `debug`,
  and `activity`/`download`/`batch`/`counter` handles for long work. The core
  never reads stdin: a question goes to the `decide::Decider` in `Ctx`
  (`choose_binary`, `stop_processes`), and `NoDecider`, the default, answers
  like a script. The binary asks through `ui::ctx_asking`, which puts
  `ui::TerminalDecider` there unless `--yes` has already answered. A
  confirmation a command makes before calling the core stays in `cmd/`: it is
  the front end's own dialog, and a GUI makes its own.
  The binary hands in `ui::Terminal`, which draws each event through the same
  `ui::` helper a command body calls, so its output is unchanged by who said
  it. In the binary there is no `println!` outside `ui.rs`: data goes to
  stdout via `ui::out`/`ui::table`; progress, warnings and errors go to
  stderr, so output can be piped. `ui.rs` is also the binary's only caller of
  `log::record` — `report::LogReporter` is the core's, for another front end —
  so a new command cannot forget to be logged, and a status line written any
  other way is invisible to whoever reads the log afterwards.
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
- **Core calls are callable from any thread and from a host that outlives
  the operation.** No `Rc`, thread-local or once-per-process initialisation
  sits in the install pipeline. Two rules bind whoever calls it:
  - *Lock.* A mutating operation holds `state::Lock` for its whole run. The
    lock is non-blocking and exclusive per lock file across processes *and*
    within this one (a static set of held paths), so a second acquire fails
    with the typed `Error::Busy { pid, lock }`, never waits and never adopts
    the lock because the file names our own pid. A lock file naming our own
    pid with no holder in the set is a stale leftover and is reclaimed. Do not
    acquire the lock inside an operation that already holds it; it would be
    `Busy` against itself. The CLI prints the same `another ketch process
    holds the lock (pid N)` and exits 8.
  - *Cancel.* `cancel::Cancel` is a cloneable shared flag. A host puts one
    clone in `InstallRequest::cancel` and keeps another; `cancel()` makes the
    pipeline return `Error::Cancelled` (exit 130) at its next check: before a
    package is prepared, between download chunks (`Http::download`,
    `Source::download` take the token), and before `commit` places anything.
    A cancelled install has removed its temp dirs and written no state entry.
    New long-running steps must take the token and check it, not loop without
    one. The CLI passes tokens that nothing cancels.
  - *Per operation.* Build `Config` per operation (`Config::load` re-reads
    `config.toml` and the environment) and call `log::init` per operation; do
    not cache either across operations. `push.rs` owns a tokio runtime and
    blocks on it, so call it from a plain worker thread, not from inside an
    async task.
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
- `self_update::remove_root` — takes the ketch root apart by naming the
  directories and files ketch creates, then removes the root itself only if
  nothing else is left in it. Older `install.sh --install-dir ~/bin` derived
  the root as that directory's parent (`$HOME`); a `remove_dir_all` on the
  root would delete someone's home. Current `install.sh` keeps `--root`
  (default `~/.ketch`) independent of `--install-dir`. Uninstall still refuses
  to wipe named children when the root is `$HOME`, because a leftover tree
  from those older installers can still look like that. Anything left
  behind is reported, never removed.
- `hooks::allowed` — a manifest's `[hooks]` table is shell the manifest's
  author wrote, and `install.rs` runs it only when the origin is the user's
  own manifest directory. A registry or built-in manifest with hooks is
  refused before anything is placed, and skipped with a warning on uninstall
  so a package can always be removed. Add a new hook event through
  `hooks::Event`, not with a second `Command` in a command body.
- `changelog::sanitize` — drops escape sequences and bidi overrides from client
  prose before it is printed. A changelog and the registry's copy of a package
  file, shown as `ketch registry push`'s review diff, are the places ketch shows
  a whole file someone else wrote; an unfiltered one can rewrite the screen
  above it — including the review the user is about to approve.

Simplicity never removes one of these. If a change makes a guard unnecessary,
delete the guard deliberately and say why in the commit.

## Adding things

- **A package that inference gets wrong** → an entry in the registry, or
  `crates/ketch-core/src/builtin.toml` if it must work offline out of the
  box. `docs/MANIFESTS.md` is the schema; `docs/REGISTRY.md` is the
  folder-per-package layout.
- **A new archive format** → implement `Extractor` in
  `crates/ketch-core/src/extract/`, add it to the platform's list. Detection
  sniffs content; do not trust the extension.
- **A new package source** → implement `Source`. Prefer an external plugin
  (`crates/ketch-core/src/source/plugin.rs`) over a built-in one: it needs no
  recompile. The wire protocol is `docs/PLUGINS.md`; changing it means bumping
  `PROTOCOL_VERSION`.
- **A new command** → a variant in `cli.rs`, a thin body in `cmd/`, and the
  work itself in `install.rs` or a trait — in the core, made `pub` in
  `lib.rs` only as far as the command needs it.
- **A field in `ketch.lock`** → `crates/ketch-core/src/lockfile.rs`, and a row
  in `docs/LOCKFILE.md`. Anything a lockfile can say has to pass `validate`
  first: it is a file a colleague may have written.
- **A column in `stats.db`** → a new folder under
  `crates/ketch-core/migrations/`, never an edit to one already released: the
  migration is embedded in the binary and has already run on other people's
  machines. Then the `table!` block and the two structs in
  `crates/ketch-core/src/stats.rs`, which the `check_for_backend` attribute
  makes the compiler verify against the schema.
- **Recording something new that happened** → a variant on `stats::Action` and
  a call from wherever it becomes true, which for anything touching the install
  tree is `install.rs`. Keep it best effort: `stats::record` warns and returns,
  because a statistic is never worth failing the operation it describes.

## Releasing

Nobody types a version number. `scripts/release.sh` is the one place a release
version is decided, and **Actions → Bump and release** (`bump.yml`, pyrlyn/infra
`bump.yml`, `patch`/`minor`/`major`) is the only way to release — and the only
thing that creates a `v*` tag:

1. `scripts/release.sh <level> --local` raises the version, writes the
   changelog entry with git-cliff and makes one `chore: release vX.Y.Z`
   commit. Nothing is pushed to `main`.
2. bump pushes it to `release/bump-vX.Y.Z` and opens a pull request into
   `main` with `RELEASE_PLZ_TOKEN`, so the pull request's own CI runs and
   reports every required check of `protect-main`.
3. When they are all green, bump rebase-merges it with `GITHUB_TOKEN` (no
   ruleset bypass) and reads back the commit that landed on `main` (rebase
   gives it a new SHA; its tree must be the tested one).
4. Only then does bump tag that commit, create a draft release with the
   version's `CHANGELOG.md` entry, and dispatch `release.yml` on the tag.

Red checks, a timeout or a failed merge close the pull request and fail the
run: no tag, no release. `dry-run` opens the pull request, waits for the checks
and closes it. `just release [level]` starts the same workflow;
`--dry-run` prints the version and changes nothing, `--local` makes the version
commit without pushing.

The version is raised only when the version in `Cargo.toml` is already tagged.
A version already on `main` without a tag (v0.10.0, from a release-plz merge
whose run was cancelled) is not released by accident: bump fails on it unless
run with `release-untagged-head`, which releases `main` as it stands after its
required checks pass. release-plz no longer runs in CI; its release pull
request was a second way to release.

One rule follows from the changelog being derived from commits: a commit that
changes or removes existing CLI behavior is marked breaking — `feat!:`/`fix!:`
or a `BREAKING CHANGE:` footer — so the entry says so and the release is cut
with `level: minor`, not `patch`. Below 1.0 that marker is what flags a removed
command before it ships as a patch release. commitlint rejects a malformed subject outright via the opt-in
commit-msg hook (`just hooks`), and CI runs it on every non-draft pull request.
The commit-msg hook also prints a reminder, not a rejection, when the staged
diff touches `src/cli.rs` or `src/cmd/` and the message carries no breaking
marker.

`release.yml` is generated by [cargo-dist](https://github.com/axodotdev/cargo-dist)
from `dist-workspace.toml` and must never be edited by hand: change the config,
`.github/build-setup.yml` or `.github/build-check.yml`, then run
`just dist-generate`. That runs `dist generate` and then
`scripts/dist-generate.sh`'s patches, which dist has no setting for: the
signing secrets under ketch's names, the Notarise and Smoke test steps from
`build-check.yml` before each build uploads, and the aggregate `SHA256SUMS`
plus a download-size table before the release is created. CI fails when the
committed `release.yml` differs from what that produces.

The release build is `dispatch-releases` with `create-release = false`:
`release.yml` runs only when bump dispatches it with a `tag`, on the tagged
commit. It builds all five targets, and only when every one of them has built
and passed its smoke test does the `host` job upload the tarballs and
`SHA256SUMS` to bump's draft release and publish it. dist never tags. A failed
build leaves the tag and a draft (invisible to `ketch self upgrade` and
`install.sh`, which read `/releases/latest`) and opens a `release-failure`
issue; re-run it from the Actions tab. The version in `Cargo.toml` *is* the
tag, and `ketch self upgrade` measures itself against exactly that.

The macOS binaries are code-signed with a Developer ID Application
certificate, held in two repository secrets: `MACOS_CERTIFICATE`, the `.p12`
as base64, and `MACOS_CERTIFICATE_PWD`, its password. dist signs with them
(`macos-sign = true`); the identity step from `build-setup.yml` finds the
certificate's identity and fails the release when either secret is missing,
rather than letting it ship unsigned. A pull request never needs them: CI runs
`dist build` unsigned. The signature is not notarised yet: a tarball fetched
with `curl` carries no quarantine flag, so Gatekeeper never asks. The
`Notarise` step is ready but off until the App Store Connect key exists. To
turn it on, add three secrets: `APPSTORE_CONNECT_KEY` (the `.p8` as base64),
`APPSTORE_CONNECT_KEY_ID` and `APPSTORE_CONNECT_ISSUER_ID`. Then set the
repository variable `KETCH_NOTARIZE` to `true`. From then on, both macOS
binaries go through `xcrun notarytool submit --wait`, and the smoke test
requires `spctl` to report `source=Notarized Developer ID`. A missing secret
fails the release. A bare binary cannot be stapled, so Gatekeeper looks its
ticket up online.

After the release is published, dist's publish job `./tap` (`tap.yml`)
regenerates the Homebrew cask with `scripts/cask.sh` — version and both
checksums — and pushes it to `Casks/ketch.rb` in `pyrlyn/homebrew-tap`. That
push needs `HOMEBREW_TAP_TOKEN`, a token allowed to write to the tap
repository; the workflow's own token is scoped to this one and cannot. The
cask is a cask and not a formula because ketch lives in `~/.ketch`: a
formula's `post_install` runs sandboxed away from `$HOME`, while a cask's
install steps can be granted network access and one writable path under it,
which is all `ketch self install` needs. Homebrew keeps only the bootstrap
binary; the installed ketch is one ketch downloaded and verified itself,
exactly as with `install.sh`.

Seven things about that handoff are easy to break:

- **`RELEASE_PLZ_TOKEN` must be a PAT or GitHub App token**, not the default
  `GITHUB_TOKEN`: a pull request opened with that token starts no workflow, so
  the bump pull request would never get its required checks (and "Allow GitHub
  Actions to create pull requests" is off in pyrlyn). bump uses the PAT only to
  push the branch and open the pull request; it merges, tags, releases and
  dispatches with `GITHUB_TOKEN`, and a tag or release made with that token
  starts no workflow, which is why bump dispatches `release.yml` itself.
- **Merge the bump pull request with nothing but bump.** Only bump knows to
  tag the commit that lands (merge method: pyrlyn/.github
  `docs/pull-requests.md`). A bump pull request merged by hand leaves an
  untagged version on `main`.
- **release-plz reads the tags, not crates.io** (`git_only = true`). By
  default it asks the registry for the last released version, and `ketch` is
  not published there — so the lookup comes back empty, release-plz decides
  the package has never been released, and proposes the version already in
  `Cargo.toml`. No bump, no changelog entry, no release, and nothing fails:
  the pull request simply never appears. It is also why `feat:` needs
  `features_always_increment_minor`, since below 1.0 release-plz would
  otherwise send a feature to the patch.
- **The changelog has one writer.** `cliff.toml` is the format for
  `scripts/release.sh` (and release-plz's `changelog_config`, for a local
  preview). The tag name is part of it: `release-plz.toml` sets
  `v{{ version }}`, bump tags `v<version>` from `Cargo.toml`, and `cliff.toml` links each heading to
  `releases/tag/v<version>` — which `tests/release_changelog.rs` checks.
  Change one and change all three.
- **The certificate expires.** A Developer ID certificate lasts five years,
  and the day after, every release fails at the identity step. Replace both
  secrets with the renewed `.p12` and re-run the release.
- **The cask is generated.** Editing `Casks/ketch.rb` in the tap by hand lasts
  until the next release overwrites it; change `scripts/cask.sh` instead. CI
  runs `brew style` on its output on every gate run, because `tap.yml` runs
  the same check *after* the release has published — where a rejected cask
  leaves the tap a version behind and takes a re-run of the `tap` job to
  correct.

release-plz does not publish to crates.io (`publish = false`), does not create
the GitHub release (`git_release_enable = false`), and does not tag
(`git_tag_enable = false`); it no longer runs in CI at all. bump.yml owns the
tag and the release; `release.yml` attaches the assets.

Bumping the version by hand in an ordinary commit is what all of this exists
to stop: the version is written in one place and read as the tag, so a stray
bump is a version nobody released.

Asset names are load-bearing: `install.sh`, `install.ps1` and `ketch self
upgrade` all look for `ketch-<target>.tar.gz` and `SHA256SUMS`. Renaming
either strips the upgrade path from every copy already out there — which is
why `dist-workspace.toml` sets `.tar.gz` for Windows too. dist puts the binary
in one top-level `ketch-<target>/` directory; every reader finds it by
searching the tree, and a store install unwraps the single directory. CI runs
the same `dist build` on every gate run, so packaging breaks there, not
halfway through a release.

### macOS app releases

The app in `desktop/macos/` is released from this repository too, by
`.github/workflows/desktop-release.yml`, with a version of its own: tags are
`desktop-vX.Y.Z`, never `vX.Y.Z`, and the version is the workflow's input, not
`Cargo.toml`'s or `project.yml`'s.

**No app release is ever the latest release.** `install.sh`, `install.ps1`
and `ketch self upgrade` (the GitHub source's `/releases/latest` fast path)
all install whatever GitHub calls the latest release. An app release marked
latest would hand every CLI installer a release with no `ketch-<target>.tar.gz`
in it. So every `gh release create` in the workflow passes `--latest=false`
(`make_latest: false`), the feed release is a prerelease as well, and the last
step checks that `/releases/latest` did not move — restoring the CLI release
and failing if it did. Likewise the CLI's release tooling never takes a
`desktop-v*` tag for its own: `cliff.toml`'s `tag_pattern` is anchored
(`^v[0-9]`; git-cliff matches it anywhere in a tag name), `tests/crate-version.sh`
lists only `v[0-9]*` tags, release-plz matches `^v<semver>$` from
`git_tag_name`, `scripts/release.sh` looks up `refs/tags/v<version>` exactly,
`scripts/tap-release-version.sh` refuses a tag without a leading `v`,
`sync-docs.yml` skips non-`v` tag refs, and when ketch lists releases instead
of asking for the latest (`--pre`), a tag that is not a version never
outranks one that is (`select_release` in `src/source/mod.rs`). `tests/desktop-release.sh` (in
`just lint-shell`) checks all of it.

To cut one: Actions → desktop-release → Run workflow on `main` with the
version, or `gh workflow run desktop-release.yml --ref main -f version=X.Y.Z`.
The version must be plain `X.Y.Z` and above the last `desktop-v*` tag
(`scripts/desktop-version.sh`), because it is also `CFBundleVersion`, which
Sparkle compares. The run archives a universal Release build with the
hardened runtime, exports it for Developer ID (`desktop/macos/ExportOptions.plist`),
notarises and staples the app, builds the `.dmg` (`scripts/desktop-dmg.sh`,
hdiutil), signs, notarises and staples that, runs `spctl --assess` on both,
writes `Ketch-X.Y.Z.dmg.sha256`, and writes the Sparkle appcast
(`scripts/desktop-appcast.sh`). Only then does it create the tag and the
release, with release notes from `desktop/cliff.toml`, and replace
`appcast.xml` on the `desktop-appcast` release, the stable URL the app's
`SUFeedURL` names. A failed run creates nothing; re-run it. If it failed after
the versioned release was created but before the feed was replaced, upload
that release's `appcast.xml` to `desktop-appcast` with `gh release upload
--clobber` rather than re-running, since the version is then taken.

Secrets, all required; the first step names any that are missing and stops
before building:

- `MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PWD` — the same Developer ID
  Application `.p12` the CLI is signed with.
- `APPSTORE_CONNECT_KEY` (the `.p8` as base64), `APPSTORE_CONNECT_KEY_ID`,
  `APPSTORE_CONNECT_ISSUER_ID` — notarisation. Unlike the CLI's, it is not
  behind `KETCH_NOTARIZE`: an app is only ever released notarised.
- `SPARKLE_ED_PRIVATE_KEY` — the EdDSA key from Sparkle's `generate_keys -x`
  (the base64 seed). Its public half is `SUPublicEDKey` in
  `desktop/macos/Ketch/Info.plist`, still a placeholder that the workflow
  refuses; commit the real one first. The appcast is checked against the
  exported app's key before anything is published
  (`scripts/desktop-appcast-verify.swift`), because `generate_appcast` only
  warns on a mismatch. Losing or rotating this key strands every installed
  copy on its version.

`just macos-appcast` runs the disk-image and appcast scripts on a local build
with a throwaway key, as CI's `macos-app` job does. The ketch-ffi XCFramework
(R9) does not exist yet: the workflow's XCFramework step is off
(`XCFRAMEWORK: 'false'`, marked `TODO(R9)`), so a release made before R9
ships the app on `FakeKetchCore`.

## Before you call it done

1. `cargo nextest run`, `cargo clippy --all-targets`, `cargo fmt --check` all clean.
2. Non-trivial logic left a test behind that fails if the logic breaks.
3. You ran the actual binary against a `KETCH_ROOT` scratch tree if the change
   touches installation, linking, or the registry.
4. You reported what you did *not* do, if anything was skipped.
