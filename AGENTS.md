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
- **English for repository files.** Commits, pull request titles and bodies,
  comments, docs, and user-facing strings in this repository are written in
  English. Do not leave non-English prose in tracked files.
- If a directory above this repository contains an `AGENTS.md` or
  `CLAUDE.md`, follow it too. If it conflicts with this file, ask the creator.
- **Config files.** A config file this project owns has a schema generated from its types (Rust: `schemars`), committed and checked by a drift test, and one module owns all config loading, validation and editing. A config file another program owns (an agent host's or an editor's) gets no schema from us: check only our own entry in it and leave the rest byte-for-byte, comments included.

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
`ketch.lock` pins clients; `src/changelog.rs` reads a client's changelog, while
the host's own `CHANGELOG.md` is written for it by git-cliff. The host is
also a client of itself: `ketch self install` records it in `state.json` as
the package `ketch`, and the root `ketch.toml` is its manifest.

macOS, Linux and Windows each have a `Platform` backend in `src/platform/`.
`host()` selects it. Another OS still means implementing that trait — nothing
above it changes. End-to-end tests are gated with `#[cfg(target_os = "...")]`
so `cargo test` on a host runs that OS's suite. CI runs that suite on macOS,
Linux and Windows.

## Commands

```bash
cargo nextest run                # unit tests and the end-to-end suite; no network
cargo nextest run -E 'binary(install)'  # just the end-to-end suite
cargo clippy --all-targets       # must be clean
cargo fmt                        # must be clean
cargo build                      # debug binary at target/debug/ketch
```

The Justfile wraps the same commands with `--locked`: `just fmt`, `just clippy`
(or `just lint`), `just test`, and `just check` runs what CI runs on this
host — format, clippy, `cargo nextest run --all-targets`, commitlint fixtures, shell
syntax on `install.sh` and the release scripts, whether `release.yml` is what
`dist generate` produces, `dist build` for the host target, and on macOS
`brew style` on the generated cask. Cross-target
builds and the Linux/Windows jobs are CI-only.

`just test` ends with a lossless `dunnage` cleanup of this checkout's cargo
`target/` dirs (compress + dedupe, never deletes); a machine without `dunnage`
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

Testing a Rust CLI application requires a combination of unit tests for
internal business logic and integration tests to verify end-to-end binary
execution, argument parsing, and output formatting. Prefer these crates for
the integration layer:

- `assert_cmd` executes the compiled CLI binary and runs assertions against
  exit codes, stdout, and stderr.
- `predicates` composes boolean assertions for output matching, including
  string containment and regular expressions.
- `assert_fs` automates setup, tear-down, and verification of temporary files
  and directories.
- `trycmd` orchestrates snapshot testing with plain-text or Markdown files so
  lengthy or complex CLI output doubles as documentation and test assertions.
- `rstest` expresses related cases as parameterized tests and fixtures without
  duplicating setup.
- `insta` records reviewed snapshots for stable structured values or output;
  use its redactions for volatile values rather than weakening the assertion.
- `pretty_assertions` makes equality failures readable; import its `assert_eq`
  and `assert_ne` macros in unit tests that compare non-trivial values.

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
commitlint runs on. `mise install` fetches both, and the Rust toolchain is
deliberately not pinned because CI builds on the runner's default stable.

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

## Task runner choice

Choose **Just** when you want a fast, lightweight, and simple command alias
tool that feels like `make` without the baggage, or when the repository manages
multiple languages alongside Rust.

Choose **cargo-make** when you need complex CI/CD build pipelines,
cross-platform conditional flows, automated crate installations, or built-in
scripting extensions such as duckscript tailored specifically for Rust.

For this repository, prefer **Just** if a task runner is introduced: the
project combines Rust with shell and site tooling, and its current commands
are simple aliases. Use cargo-make instead only when the workflow grows into
conditional, multi-stage Rust automation.

## Layout

| Path | Owns |
| --- | --- |
| `src/main.rs` | argument parsing, config construction, dispatch — nothing else |
| `src/cli.rs` | the clap surface, kept separate so `cmd/` takes its args directly |
| `src/cmd/` | thin command bodies: arguments, output, confirmations |
| `src/install.rs` | the install/uninstall/relink pipeline every command shares |
| `src/resolve.rs` | side-effect-free resolution trace shared by install and `ketch why` |
| `src/source/` | where releases come from: GitHub built in, plugins external |
| `src/extract/` | archive formats, selected by sniffing content not file names |
| `src/platform/` | OS-specific placement, linking, trust checks (`macos.rs`, `linux.rs`, `windows.rs`) |
| `src/shell.rs` | putting the bin dir on PATH in bash, zsh and fish, and on Windows the user environment |
| `src/registry.rs` | the fetched package registry (see `docs/REGISTRY.md`) |
| `src/manifest.rs` | resolving a name to a `Manifest` across four tiers |
| `src/model.rs` | every type that crosses a module boundary |
| `src/state.rs` | the installed-package record and the process lock |
| `src/stats.rs` | `stats.db`: the history of what was installed, in SQLite |
| `src/log.rs` | the log file, in text or JSON Lines |
| `src/changelog.rs` | finding and slicing a client app's changelog |
| `src/lockfile.rs` | `ketch.lock`: what is installed, pinned to exact releases |
| `src/push.rs` | `ketch registry push`: a project's `ketch.toml` as a registry pull request, via octocrab |
| `src/self_update.rs` | `ketch self`: installing, updating and removing the host as a package |
| `ketch.toml` | the host's own package file, what `ketch registry push` sends |
| `src/ui.rs` | all terminal output |
| `tests/` | end-to-end tests that drive the real binary |
| `dist-workspace.toml` | what cargo-dist builds, signs and publishes; the source of `release.yml` |
| `scripts/dist-generate.sh` | `dist generate` plus the patches to `release.yml` dist has no setting for |
| `.github/build-setup.yml`, `.github/build-check.yml` | steps dist splices into each release build: before it, and before upload |
| `.github/workflows/release.yml` | generated by dist; builds every target, then tags and publishes the release |
| `.github/workflows/bump.yml` | the one-click release: verify, bump, commit, dispatch |
| `.github/workflows/verify.yml` | the gate a release is cut behind, shared by `bump.yml` and `release-plz.yml` |
| `.github/workflows/tap.yml` | dist's publish job: the Homebrew cask, pushed to the tap |
| `scripts/release.sh` | the one place a release version is decided; `just release` |
| `release-plz.toml` | what the release pull request bumps, and what it does not publish |
| `cliff.toml` | the `CHANGELOG.md` entry format, for release-plz and `scripts/release.sh` alike |
| `plan.md` | what is being built next, and what each piece would take |
| `scripts/cask.sh` | the Homebrew cask, generated into `listepo/homebrew-tap` on release |
| `install.sh` | the `curl | bash` installer for macOS and Linux; only bootstraps `ketch self install` |
| `install.ps1` | the `irm | iex` installer for Windows; same bootstrap as `install.sh` |
| `.github/dependabot.yml` | weekly `chore(deps)` pull requests for cargo, npm and GitHub Actions; not `mise.toml` |

The rule that keeps `cmd/` thin: anything touching the install tree belongs in
`install.rs`, `state.rs`, or a trait implementation, so the same logic serves
every command. If you are about to write install logic inside a command, you
are in the wrong file.

Several things write outside the ketch root. `ketch self install`, the bootstrap
installers and the Homebrew cask each place a bootstrap binary outside it.
`src/platform/` links `.app` bundles into `/Applications` (or
`KETCH_APPS_DIR`), and man pages and completions into the user directories
`ketch doctor` reports; those destinations are recorded in state so uninstall
can take them back. `src/shell.rs` edits shell startup files and the user PATH
only when asked: `ketch path install`, `ketch doctor --fix` and `ketch self
uninstall`, which takes the block back out of every startup file that has one
rather than only the shell running now, and on Windows takes the bin dir out of
the user PATH. It edits a shell startup file between two
markers, so the block can be found again, rewritten when the root moves, and
removed without guessing which line was ketch's. It follows a symlinked
startup file to its target before writing, because that file is very often a
link into a dotfiles repository. On Windows `ketch path install` writes
`HKCU\Environment\Path` via `[Environment]::SetEnvironmentVariable` so a new
terminal sees it without a logoff; `setx` is not used, because it truncates.

## Conventions

These are observed throughout; match them rather than introducing your own.

- **Every file opens with a `//!` header** saying what the module owns and why
  it exists separately. Every public item has a doc comment.
- **A generated file says so in its first lines**, and the generator writes
  that header, not a person or a second script: `ketch lock` for `ketch.lock`,
  `site/sync-docs.py` for `site/content/docs/`, `scripts/cask.sh` for the
  tap's `Casks/ketch.rb`. To change such a file, change its generator.
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
- `self_update::remove_root` — takes the ketch root apart by naming the
  directories and files ketch creates, then removes the root itself only if
  nothing else is left in it. Older `install.sh --install-dir ~/bin` derived
  the root as that directory's parent (`$HOME`); a `remove_dir_all` on the
  root would delete someone's home. Current `install.sh` keeps `--root`
  (default `~/.ketch`) independent of `--install-dir`. Uninstall still refuses
  to wipe named children when the root is `$HOME`, because a leftover tree
  from those older installers can still look like that. Anything left
  behind is reported, never removed.
- `changelog::sanitize` — drops escape sequences and bidi overrides from client
  prose before it is printed. A changelog and the registry's copy of a package
  file, shown as `ketch registry push`'s review diff, are the places ketch shows
  a whole file someone else wrote; an unfiltered one can rewrite the screen
  above it — including the review the user is about to approve.

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
- **A column in `stats.db`** → a new folder under `migrations/`, never an edit
  to one already released: the migration is embedded in the binary and has
  already run on other people's machines. Then the `table!` block and the two
  structs in `src/stats.rs`, which the `check_for_backend` attribute makes the
  compiler verify against the schema.
- **Recording something new that happened** → a variant on `stats::Action` and
  a call from wherever it becomes true, which for anything touching the install
  tree is `install.rs`. Keep it best effort: `stats::record` warns and returns,
  because a statistic is never worth failing the operation it describes.

## Releasing

Nobody types a version number. `scripts/release.sh` is the one place a release
version is decided, and there are three ways to run it:

- **release-plz** keeps one `chore: release vX.Y.Z` pull request up to date on
  every push to `main`, holding the next version and its `CHANGELOG.md` entry,
  both derived from the conventional commits since the last tag — so `feat:`
  moves the minor, `fix:` the patch, and `docs:`/`chore:` move nothing.
  Merging it runs `verify.yml` on the merge commit and then
  `scripts/release.sh patch --no-bump`, which dispatches `release.yml` for the
  version the pull request wrote.
- **`bump.yml`** (Actions → Bump and release, `patch`/`minor`/`major`) runs
  `verify.yml`, then `scripts/release.sh <level>`: it raises the version, writes
  the changelog entry with git-cliff, pushes one `chore: release vX.Y.Z` commit
  straight to `main` and dispatches `release.yml`.
- **`just release [level]`** does the same from a clean, up-to-date `main`.
  `--dry-run` prints the version and changes nothing; `--local` makes the
  version commit without pushing or dispatching.

The version is raised only when the version in `Cargo.toml` is already tagged,
so a version a merged release pull request wrote is released as it stands.
Close release-plz's pull request if you release another way, or it will
propose a version that has already shipped on its next update.

One rule follows from deriving the version from commits: a commit that changes
or removes existing CLI behavior is marked breaking — `feat!:`/`fix!:` or a
`BREAKING CHANGE:` footer — so the bump lands on the minor, not the patch.
Below 1.0 that marker is all that keeps a removed command from shipping as a
patch release. commitlint rejects a malformed subject outright via the opt-in
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

The release itself is `dispatch-releases`: `release.yml` runs only when
dispatched with a `tag`. It builds all five targets, and only when every one
of them has built and passed its smoke test does the `host` job create the tag
and the GitHub release, at the commit that was built, with that version's
`CHANGELOG.md` section as the notes. A tag exists if and only if a release
finished, so `ketch self upgrade` and `install.sh` can never find a tag whose
binaries are still building or never arrived; a failed run creates nothing,
and is re-run from the Actions tab. The version in `Cargo.toml` *is* the tag,
and `ketch self upgrade` measures itself against exactly that.

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
checksums — and pushes it to `Casks/ketch.rb` in `listepo/homebrew-tap`. That
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
  `GITHUB_TOKEN`, which cannot start another workflow run — so the release
  pull request it opens would never have CI run on it. `release-plz.yml`
  fails on the missing secret rather than letting that happen quietly. The
  other direction is on purpose: the version commit `bump.yml` pushes with
  `GITHUB_TOKEN` starts neither `ci.yml` nor `release-plz.yml`, and dispatch
  is the one event that token may start, which is how it reaches `release.yml`.
- **Only the merge of release-plz's own pull request is a release there.**
  `release-plz.yml`'s gate wants a line that *is* the `chore: release vX.Y.Z`
  title and a pull request from a `release-plz-` branch behind the commit:
  `just release` pushes a commit with the same subject straight to `main` and
  dispatches the release itself, and a second dispatch would race the first.
- **release-plz must not propose a version while one is being published.**
  A release pull request is written against the last released version, and for
  the minutes between the dispatch and the tag there is none.
  `release-plz.yml` checks for the tag and leaves the pull request alone until
  it exists; the next commit after that updates it.
- **release-plz reads the tags, not crates.io** (`git_only = true`). By
  default it asks the registry for the last released version, and `ketch` is
  not published there — so the lookup comes back empty, release-plz decides
  the package has never been released, and proposes the version already in
  `Cargo.toml`. No bump, no changelog entry, no release, and nothing fails:
  the pull request simply never appears. It is also why `feat:` needs
  `features_always_increment_minor`, since below 1.0 release-plz would
  otherwise send a feature to the patch.
- **The changelog has one writer.** `cliff.toml` is the format for both
  release-plz (`changelog_config`) and `scripts/release.sh`, so an entry reads
  the same whichever way the release was cut. The tag name is part of it:
  `release-plz.toml` sets `v{{ version }}`, `scripts/release.sh` dispatches
  `v<version>`, and `cliff.toml` links each heading to
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
the GitHub release (`git_release_enable = false`), and does not tag: only its
`release-pr` command is ever run. `release.yml` owns all three, because it is
what builds and attaches the assets.

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

## Before you call it done

1. `cargo nextest run`, `cargo clippy --all-targets`, `cargo fmt --check` all clean.
2. Non-trivial logic left a test behind that fails if the logic breaks.
3. You ran the actual binary against a `KETCH_ROOT` scratch tree if the change
   touches installation, linking, or the registry.
4. You reported what you did *not* do, if anything was skipped.
