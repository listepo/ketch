# AGENTS.md

Notes for coding agents working in this repository. Humans are welcome to read
it too — nothing here is agent-specific except the framing.

## What ketch is

A single-binary CLI package manager that installs command-line tools and macOS
apps straight from GitHub releases. No taps, no formulae, no build step: it
downloads what a project already ships, verifies it, unpacks it into a store,
and links it onto `PATH`.

It is a strict-TypeScript monorepo that compiles to one native binary. The
binary is the product; the runtime is an implementation detail nobody installing
ketch has to know about.

### Host app, client app

Two words used throughout this file and the code, because "app" alone is
ambiguous in a package manager:

- **host app** — ketch itself: this repository, the binary a release publishes,
  the thing being changed. Its own version, release process and `~/.ketch` tree
  are the host's.
- **client app** — anything ketch installs and manages: ripgrep, a `.app`
  bundle, whatever a `Manifest` names. It is written by someone else, so
  everything about it — asset names, archive members, `CHANGELOG.md`, release
  notes — is untrusted input, not ketch's own data.

Where the distinction matters most: `ketch self update` upgrades the host,
`ketch upgrade` upgrades clients; `scripts/release.sh` releases the host,
`ketch.lock` pins clients; `packages/core/src/changelog.ts` reads a client's
changelog, while the host's history is git.

macOS is the only implemented platform. `packages/core/src/platform/platform.ts`
gates it in `hostPlatform()` — anything but `darwin` gets a clear error before
the backend is even imported — so a Linux backend means implementing the
`Platform` interface and adding one branch there. Nothing above it changes.

## Commands

```bash
pnpm install                     # once, or after a dependency changes
pnpm run typecheck               # tsc --build across the project references
pnpm run lint                    # oxlint
pnpm run format                  # biome format --write
pnpm run format:check            # the same, without writing
pnpm run test                    # vitest: unit and end-to-end, no network
pnpm run check                   # typecheck, lint, format:check, test
pnpm exec moon ci                # only the tasks your change affects
```

Run the CLI from source against a throwaway tree instead of your real
`~/.ketch`:

```bash
KETCH_ROOT=/tmp/ketch-scratch node apps/cli/src/main.ts doctor
```

Node runs the TypeScript directly by stripping types, so there is no build step
in the loop — `bun` and `deno run -A` run the same entry point unchanged.

Read a gate's **real exit code** (`cmd; echo exit:$?`); never read a pipe's
status. CI runs the same gates on macOS, and all of them must pass before a
change is done.

### Toolchain

Pinned in `mise.toml` and `package.json`. Check them rather than trusting this
table — it is a summary, they are the source.

| Tool | Version | Role |
| --- | --- | --- |
| TypeScript | 7 (native compiler) | strict everywhere; project references |
| Node.js | 26 | canonical runtime; CI runs the suite here |
| Bun | 1.3 | fastest runtime: the dev/agent loop runs on it, and it compiles the released binary |
| Deno | 2 | supported runtime, smoke-tested |
| Perry (`@perryts/perry`) | 0.5 | the intended release compiler; blocked upstream, see Releasing |
| pnpm | 10 | dependency management (workspaces in `pnpm-workspace.yaml`) |
| Moon | 2.5 | task runner; inherited tasks in `.moon/tasks/all.yml` |
| Vitest | 4 | the whole suite; colocated `*.test.ts` |
| oxlint | 1.80 | the linter (no ESLint) |
| Biome | 2.5 | the formatter (no Prettier) |
| Zod | 4 | schemas; `z.toJSONSchema` emits the published JSON Schemas |
| pino | 10 | the log file (JSON Lines native; pino-pretty for text) |

`mise install` gives a developer, an agent and CI the same versions.

## Layout

| Path | Owns |
| --- | --- |
| `apps/cli/src/main.ts` | the entry point: run the command, render the failure, choose the exit code — nothing else |
| `apps/cli/src/cli.ts` | the commander surface and the one `preAction` bootstrap, kept separate so `cmd/` takes its args directly |
| `apps/cli/src/cmd/` | thin command bodies: arguments, output, confirmations |
| `apps/cli/src/ui.ts` | all terminal output, and the only caller of `log.record` |
| `apps/cli/src/completions.ts` | bash, zsh and fish completions, generated from the surface itself |
| `apps/cli/tests/` | end-to-end tests that drive the real CLI |
| `packages/core/src/install.ts` | the install/uninstall/relink pipeline every command shares |
| `packages/core/src/config.ts` | the effective config: defaults, `config.json`, environment, flags |
| `packages/core/src/source/` | where releases come from: GitHub built in, plugins external |
| `packages/core/src/extract/` | archive formats, selected by sniffing content not file names |
| `packages/core/src/platform/` | OS-specific placement, linking, trust checks |
| `packages/core/src/shell.ts` | putting the bin dir on PATH in bash, zsh and fish |
| `packages/core/src/registry.ts` | the fetched package registry (see `docs/REGISTRY.md`) |
| `packages/core/src/manifest.ts` | resolving a name to a `Manifest` across four tiers |
| `packages/core/src/model.ts` | every type that crosses a module boundary |
| `packages/core/src/state.ts` | the installed-package record and the process lock |
| `packages/core/src/log.ts` | the log file, in text or JSON Lines |
| `packages/core/src/changelog.ts` | finding and slicing a client app's changelog |
| `packages/core/src/lockfile.ts` | `ketch.lock`: what is installed, pinned to exact releases |
| `packages/schemas/src/` | every on-disk shape as a Zod schema, plus the guards those schemas need |
| `packages/schemas/schemas/` | the generated JSON Schemas, committed and published |
| `docs/` | the documentation, and the only copy of it |
| `apps/web/` | the landing page: Astro and Tailwind, deployed at the site root |
| `apps/docs/` | the docs site: Docusaurus, its pages generated from `docs/` |
| `scripts/package.sh` | the release tarball, shared by CI and the release workflow |
| `scripts/release.sh` | the version bump and the release pull request |

The dependency direction is one-way and worth keeping that way: `@ketch/schemas`
imports nothing of ketch's, `@ketch/core` imports schemas, `apps/cli` imports
both. Shape validation and the value checks a shape cannot express live in
schemas so every consumer validates identically; domain behaviour — version
ordering, asset scoring, install state — lives in core.

The rule that keeps `cmd/` thin: anything touching the install tree belongs in
`install.ts`, `state.ts`, or an interface implementation, so the same logic
serves every command. If you are about to write install logic inside a command,
you are in the wrong file.

`shell.ts` is the one module that writes outside the ketch root, and it does so
only when asked: `ketch path install` and `ketch doctor --fix`. It edits a shell
startup file between two markers, so the block can be found again, rewritten
when the root moves, and removed without guessing which line was ketch's. It
follows a symlinked startup file to its target before writing, because that file
is very often a link into a dotfiles repository.

## Conventions

These are observed throughout; match them rather than introducing your own.

- **Every file opens with a `/** … */` header** saying what the module owns and
  why it exists separately. Every exported item has a doc comment.
- **Comments explain *why*, never *what*.** The code already says what it does.
  A comment earns its place by recording a decision, a constraint, or a
  failure that motivated the shape of the code.
- **All output goes through `ui`.** There is no `console.*` outside
  `apps/cli/src/ui.ts`, and oxlint enforces it. Data goes to stdout via
  `ui.out`/`ui.table`; progress, warnings and errors go to stderr, so output can
  be piped. `ui.ts` is also the only caller of `log.record`, so a new command
  cannot forget to be logged, and a status line written any other way is
  invisible to whoever reads the log afterwards.
- **Core cannot reach the terminal.** Anything in `packages/core` that needs to
  say something takes a sink — a `warn` callback, a `ProgressSink`, an
  `InstallReporter` — and the CLI supplies one. This is what keeps the pipeline
  testable and the output in one place.
- **Errors are `KetchError`**, built with `KetchError.msg`/`io`/`parse` or a
  variant object. Its display text and exit codes are pinned by tests. Guards in
  `@ketch/schemas` throw plain `Error`, because schemas know nothing of core;
  the CLI's error renderer handles both.
- **No `any`, no non-null `!`, no `enum`, no parameter properties, no
  namespaces, no decorators.** `erasableSyntaxOnly` and oxlint enforce it:
  Node runs TypeScript by stripping types, so syntax that compiles to runtime
  code makes the CLI unrunnable on the canonical runtime.
- **`node:` builtins only.** No Bun, Deno or Perry specific API anywhere. Run
  tests on Bun because it is fastest, never because something only works there.
  A dependency shipped as a CommonJS bundle needs
  `(await import("x")).default` — Node cannot see named exports through one, and
  Bun's interop will hide the failure from you.
- **Data files are JSON**, every load validates through `@ketch/schemas`, and
  every file ketch writes stamps `$schema` via `schemaUrl(name)`. Field names
  are byte-for-byte what the earlier Rust build wrote, so a `state.json` left by
  an older install is read unchanged — renaming one is a breaking change to
  somebody's machine, not a refactor.
- **Tests live in a colocated `*.test.ts`** beside the file they test, and are
  named as sentences: `latest_prefers_highest_stable` reads in TS as
  `it("prefers the highest stable release")`. A test name should read as the
  claim it proves.
- **`apps/cli/tests/` is the exception**, and only for what a unit test cannot
  reach: the pipeline end to end, through the real CLI in a spawned process.
  `tests/support.ts` builds a throwaway root, fixture archives and a `/bin/sh`
  source plugin that serves them, so the suite stays offline. Add a case there
  when a bug could pass every unit test in the tree — most of them could.
- **Best-effort where a partial answer beats no answer.** A broken plugin, an
  unreadable manifest or one unreachable source is warned about and skipped,
  never fatal. A malformed *built-in* registry is a ketch bug and does fail.

## Trust boundaries

Most of what ketch handles was written by someone else: GitHub API responses,
release asset names and bytes, archive member paths, registry `ketch.json`
files, and source-plugin subprocess output — everything about a client app, in
other words. Anything from those reaching a filesystem path, a URL, a process
or the user's terminal is a trust boundary.

Reuse the guards that exist rather than writing new ones. They live in
`packages/schemas/src/util.ts` unless noted, and core re-exports them:

- `safeMemberPath` — rejects archive entries that escape the destination
  (`..`, absolute, Windows drive/stream syntax).
- `sanitizeComponent` — makes a string usable as one path component. To
  *reject* rather than rewrite, ask whether it changes the value; that is what
  `validateManifest` does, because a package that installs somewhere other than
  where it says is worse than one that refuses to install.
- `validateManifest` — the single guard every manifest tier passes through
  (registry, user manifests, built-in). Add new checks there, not at a caller.
- `validateLockfile` — the same for `ketch.lock`, except that one bad entry
  fails the whole file: a lockfile that installed most of itself would not be a
  lock at all.
- `validateRepo` — anything that becomes `github.com/owner/repo`.
- `sanitize` in `packages/core/src/changelog.ts` — drops escape sequences and
  bidi overrides from client prose before it is printed. A changelog is the one
  place ketch shows a whole file someone else wrote; an unfiltered one can
  rewrite the screen above it.
- `runPlugin` in `packages/core/src/source/plugin.ts` — closed stdin, an 8 MiB
  cap per stream, a 30-second deadline, and strict UTF-8 decoding. Discovery
  probes every plugin before anything else runs, so one that hangs or floods a
  pipe would take down every command.

Every schema is a `strictObject`: a misspelt key is refused, never ignored. A
package that installs the wrong thing and complains nowhere is the failure this
prevents. Keep it that way.

Simplicity never removes one of these. If a change makes a guard unnecessary,
delete the guard deliberately and say why in the commit.

## Adding things

- **A package that inference gets wrong** → an entry in the registry, or
  `packages/schemas/src/builtin.ts` if it must work offline out of the box.
  `docs/MANIFESTS.md` is the schema; `docs/REGISTRY.md` is the folder-per-package
  layout.
- **A new archive format** → implement `Extractor` in
  `packages/core/src/extract/`, add it to the platform's list. Detection sniffs
  content; do not trust the extension.
- **A new package source** → implement `Source`. Prefer an external plugin
  (`packages/core/src/source/plugin.ts`) over a built-in one: it needs no
  release. The wire protocol is `docs/PLUGINS.md`; changing it means bumping
  `PROTOCOL_VERSION` in `packages/schemas/src/plugin.ts`.
- **A new command** → a `.command()` in `cli.ts`, a thin body in `cmd/`, and the
  work itself in `install.ts` or an interface.
- **A field in any data file** → the Zod schema in `packages/schemas/src/`,
  then `pnpm --filter @ketch/schemas run generate` to regenerate
  `packages/schemas/schemas/*.schema.json`, and commit the result. A lockfile
  field also wants a row in `docs/LOCKFILE.md`: anything a lockfile can say has
  to pass `validateLockfile` first, because it is a file a colleague may have
  written.

## Releasing

The version is written in two files: `apps/cli/package.json`, and the `VERSION`
constant in `apps/cli/src/cli.ts` — which repeats it because `resolveJsonModule`
is off across the workspace. `version.test.ts` asserts the two agree, and that
assertion is not optional housekeeping: `ketch self update` compares the running
version against the release tag, so a build that disagrees with itself breaks
upgrades for everyone already installed.

```bash
scripts/release.sh 0.2.0          # --dry-run to see it first
```

Run it on a clean default branch that is level with its remote. It rewrites both
files on a `release/v0.2.0` branch, pushes it, and opens a pull request listing
the commits since the last tag and giving the tag command to publish. It refuses
a version that goes backwards, one that is already current, and one written with
a leading `v`, and it re-reads what it wrote before pushing, so a bad rewrite
fails with nothing published. `pnpm-lock.yaml` records the workspace member by
path rather than by version, so a bump does not touch it.

Merging the pull request does not release anything. Tagging the merge commit
does:

```bash
git tag v0.2.0 && git push origin v0.2.0
```

The release workflow then re-runs the whole gate, refuses a tag that disagrees
with the declared version, and publishes the tarballs with an aggregate
`SHA256SUMS`.

`scripts/package.sh` compiles one target with `bun build --compile` and tars
the result. Bun cross-compiles, so either slice builds on either architecture;
the release workflow still runs one runner per macOS architecture, because that
is what lets it execute the binary it just built before publishing it.

Asset names are load-bearing: `install.sh` and `ketch self update` both look for
`ketch-<target>.tar.gz`, with the binary named `ketch` at the archive root, and
an aggregate `SHA256SUMS`. Renaming any of them strips the upgrade path from
every copy already out there. CI runs the same `scripts/package.sh` on every
pull request so packaging breaks before a tag is pushed, not after.

Bumping the version by hand is what `scripts/release.sh` exists to stop: the
version is written in two files and checked in two more places, and all of them
must agree.

### Why Bun compiles the binary and not Perry

Perry is the intended compiler, and the reason this codebase is written to
erasable-syntax and `node:`-builtins-only rules. It cannot do the job yet: as of
0.5.1220 its prebuilt macOS `libperry_stdlib.a` references `js_ext_http_client_*`
symbols the published package never defines, so linking fails for any program
that calls `fetch`. It is not something ketch is doing wrong — a five-line
`await fetch("https://example.com")` reproduces it, while the same program using
only `node:fs` links and runs. Downloading is ketch's whole job, so the release
path uses Bun until that is fixed upstream; `scripts/package.sh` is the one
place to change back.

`@perryts/perry` stays a devDependency on purpose, so checking whether upstream
has fixed it is one command rather than a setup:

```bash
printf 'const r = await fetch("https://example.com");\nconsole.log(r.status);\n' > /tmp/probe.ts
pnpm exec perry compile /tmp/probe.ts -o /tmp/probe
```

When that links, try `perry compile apps/cli/src/main.ts`. It will also want
`perry.compilePackages` in `apps/cli/package.json` listing every dependency that
reaches the binary; Perry names the missing ones itself, so the list is grown by
re-running until it stops.

Nothing else about the port depends on which compiler wins. The portability
rules stay as they are: they are what keeps that switch a one-line change.

## Before you call it done

1. `pnpm run typecheck`, `pnpm run lint`, `pnpm run format:check` and
   `pnpm run test` all clean, for every package you touched, read from their
   real exit codes.
2. Non-trivial logic left a test behind that fails if the logic breaks.
3. You ran the actual CLI against a `KETCH_ROOT` scratch tree if the change
   touches installation, linking, or the registry.
4. You regenerated `packages/schemas/schemas/` if you changed a schema, and
   committed the result.
5. You reported what you did *not* do, if anything was skipped.

## Delegating work to agents

Match the model to the thinking the task needs: **Fable 5 or Opus 5** for
work that requires real reasoning (pipeline concurrency, parsers, security
boundaries, architecture); **Sonnet** for straightforward well-specified
jobs; **Haiku** for mechanical ones. When a "simple" task turns out to need
thinking, escalate the model rather than accepting a shallow result.

One writer per package at a time — including a second session you did not
start. Do not edit a shared barrel while siblings are running; list the exports
you need for whoever integrates instead.
