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
the host's own `CHANGELOG.md` is written for it by release-plz. The host is
also a client of itself: `ketch self install` records it in `state.json` as
the package `ketch`, and the root `ketch.toml` is its manifest.

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

The Justfile wraps the same commands with `--locked`: `just fmt`, `just clippy`
(or `just lint`), `just test`, and `just check` runs the whole CI gate.

Commitlint checks commit messages against the conventional-commit format:

```bash
just deps             # one-time: mise install + npm ci
just hooks            # opt in to the commit-msg hook
just lint-commits     # commitlint over fixture messages; part of just check
```

The commit-msg hook is opt-in via `just hooks`: commitlint then runs on every
`git commit`, rejecting a malformed subject outright and letting merge and
revert subjects through.

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

CI runs `fmt --check`, `clippy -D warnings` and `test` on macOS. All three must
pass before a change is done.

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
| `src/source/` | where releases come from: GitHub built in, plugins external |
| `src/extract/` | archive formats, selected by sniffing content not file names |
| `src/platform/` | OS-specific placement, linking, trust checks |
| `src/shell.rs` | putting the bin dir on PATH in bash, zsh and fish |
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
| `scripts/package.sh` | the release tarball, shared by CI and the release workflow |
| `release-plz.toml` | what the release pull request bumps, and what it does not publish |
| `scripts/release.sh` | the same version bump and pull request, by hand |
| `plan.md` | what is being built next, and what each piece would take |
| `scripts/cask.sh` | the Homebrew cask, generated into `listepo/homebrew-tap` on release |
| `install.sh` | the `curl | bash` installer; only bootstraps `ketch self install` |

The rule that keeps `cmd/` thin: anything touching the install tree belongs in
`install.rs`, `state.rs`, or a trait implementation, so the same logic serves
every command. If you are about to write install logic inside a command, you
are in the wrong file.

`src/shell.rs` is the one module that writes outside the ketch root, and it
does so only when asked: `ketch path install`, `ketch doctor --fix` and
`ketch self uninstall`, which takes the block back out of every startup file
that has one rather than only the shell running now. It edits
a shell startup file between two markers, so the block can be found again,
rewritten when the root moves, and removed without guessing which line was
ketch's. It follows a symlinked startup file to its target before writing,
because that file is very often a link into a dotfiles repository.

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
  nothing else is left in it. `install.sh --install-dir ~/bin` makes the root
  that directory's parent, so a `remove_dir_all` on the root is a way to delete
  someone's home directory. Anything left behind is reported, never removed.
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

Nothing is typed to cut a release. release-plz keeps one pull request up to
date on every merge to `main`, holding the next version and the `CHANGELOG.md`
entry for it, both derived from the conventional commits since the last tag —
so `feat:` moves the minor, `fix:` the patch, and `docs:`/`chore:` move
nothing. Merging that pull request is the release.

One rule follows from that derivation: a commit that changes or removes
existing CLI behavior is marked breaking — `feat!:`/`fix!:` or a
`BREAKING CHANGE:` footer — so the bump lands on the minor, not the patch.
Below 1.0 that marker is all that keeps a removed command from shipping as a
patch release. commitlint rejects a malformed subject outright — CI runs the
same check over a pull request's commits — and the commit-msg hook prints a
reminder, not a rejection, when the staged diff touches `src/cli.rs` or
`src/cmd/` and the message carries no breaking marker.

`release.yml` runs on every merge to `main` and asks one question first: does
`v<version from Cargo.toml>` already exist as a tag? If it does, that version
has shipped and the run stops there in seconds. If it does not, this merge is a
release: the whole gate runs again, both macOS architectures are built and
signed, the tarballs and an aggregate `SHA256SUMS` go up on a **draft** release
— which has no tag — and the last step publishes that draft, which is what
creates the tag, at the commit that was built.

That ordering is the point. A tag exists if and only if a release finished, so
`ketch self update` and `install.sh` can never find a tag whose binaries are
still building or never arrived; a failed run leaves a draft to re-run or
delete, and main simply stays untagged. It also removes the old mismatch
hazard: the version in `Cargo.toml` *is* the tag, derived rather than compared,
and `ketch self update` measures itself against exactly that.

A re-run is `workflow_dispatch` with `force` — the one case the tag check would
otherwise skip, such as a `tap` job that failed after the release published.

The binaries are code-signed with a Developer ID Application certificate,
held in two repository secrets: `MACOS_CERTIFICATE`, the `.p12` as base64, and
`MACOS_CERTIFICATE_PWD`, its password. `release.yml` imports it into a
throwaway keychain, hands the identity to `scripts/package.sh` as
`KETCH_SIGN_IDENTITY`, and deletes the keychain afterwards. CI runs the same
script without an identity and packs the binary unsigned, so a pull request
never needs the certificate. A release with either secret missing fails rather
than ships unsigned. The signature is not notarised: a tarball fetched with
`curl` carries no quarantine flag, so Gatekeeper never asks, and notarisation
would need an App Store Connect key that does not exist yet.

After the release is published, the `tap` job regenerates the Homebrew cask
with `scripts/cask.sh` — version and both checksums — and pushes it to
`Casks/ketch.rb` in `listepo/homebrew-tap`. That push needs
`HOMEBREW_TAP_TOKEN`, a token allowed to write to the tap repository; the
workflow's own token is scoped to this one and cannot. The cask is a cask and
not a formula because ketch lives in `~/.ketch`: a formula's `post_install`
runs sandboxed away from `$HOME`, while a cask's install steps can be granted
network access and one writable path under it, which is all `ketch self
install` needs. Homebrew keeps only the bootstrap binary; the installed ketch
is one ketch downloaded and verified itself, exactly as with `install.sh`.

Six things about that handoff are easy to break:

- **`RELEASE_PLZ_TOKEN` must be a PAT or GitHub App token**, not the default
  `GITHUB_TOKEN`, which cannot start another workflow run — so the release
  pull request it opens would never have CI run on it, and merging that pull
  request is what publishes ketch. `release-plz.yml` fails on the missing
  secret rather than letting that happen quietly.
- **release-plz must not propose a version while one is being published.**
  A release pull request is written against the last released version, and for
  the minutes between the merge and the tag there is none. `release-plz.yml`
  checks for the tag and leaves the pull request alone until it exists; the
  next commit after that opens it.
- **release-plz reads the tags, not crates.io** (`git_only = true`). By
  default it asks the registry for the last released version, and `ketch` is
  not published there — so the lookup comes back empty, release-plz decides
  the package has never been released, and proposes the version already in
  `Cargo.toml`. No bump, no changelog entry, no release, and nothing fails:
  the pull request simply never appears. It is also why `feat:` needs
  `features_always_increment_minor`, since below 1.0 release-plz would
  otherwise send a feature to the patch.
- **The tag name is a contract.** `release-plz.toml` sets `v{{ version }}`,
  `release.yml` derives the same string from `Cargo.toml`, and the changelog
  links to `releases/tag/v<version>` — which `tests/release_changelog.rs`
  checks. Change one and change all three. That last link is a changelog
  postprocessor: from the second release on, release-plz would point the
  heading at the diff against the previous tag instead of at the release
  holding the tarballs.
- **The certificate expires.** A Developer ID certificate lasts five years,
  and the day after, every release fails at the import step. Replace both
  secrets with the renewed `.p12` and re-run with `force`.
- **The cask is generated.** Editing `Casks/ketch.rb` in the tap by hand lasts
  until the next release overwrites it; change `scripts/cask.sh` instead. CI
  runs `brew style` on its output on every pull request, because the `tap` job
  runs the same check *after* the release has published — where a rejected
  cask leaves the tap a version behind and takes another release to correct.

release-plz does not publish to crates.io (`publish = false`), does not create
the GitHub release (`git_release_enable = false`), and does not tag: only its
`release-pr` command is ever run. `release.yml` owns all three, because it is
what builds and attaches the assets.

`scripts/release.sh 0.2.0` still works and does the same job by hand — bump on
a branch, pull request, merge to release — for cutting a specific version
without waiting for the bot. Close release-plz's pull request if you use it, or
the two will propose different versions. Either way `CHANGELOG.md` has to gain
the entry for the new version, because `tests/release_changelog.rs` requires
one for whatever `Cargo.toml` says and `release.yml` publishes that section as
the release notes.

Bumping the version by hand in an ordinary commit is what both of these exist
to stop: the version is written in one place and read as the tag, so a stray
bump publishes a release.

Asset names are load-bearing: `install.sh` and `ketch self update` both look for
`ketch-<target>.tar.gz` and `SHA256SUMS`. Renaming either strips the upgrade
path from every copy already out there. CI runs the same `scripts/package.sh` on
every pull request so packaging breaks there, not halfway through a release.

## Before you call it done

1. `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check` all clean.
2. Non-trivial logic left a test behind that fails if the logic breaks.
3. You ran the actual binary against a `KETCH_ROOT` scratch tree if the change
   touches installation, linking, or the registry.
4. You reported what you did *not* do, if anything was skipped.
