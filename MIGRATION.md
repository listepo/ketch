# TypeScript migration plan

The step-by-step plan for porting ketch from Rust to a strict-TypeScript
monorepo, written so **any AI agent can execute or resume any step without
prior conversation context**. Read this whole file before doing anything.
Branch: `ts-rewrite`. The Rust tree in `src/` + `tests/` is the executable
spec until Phase 10 removes it.

## How to resume (after a crash, usage limit, or fresh session)

1. `git -C /Users/listepo/GitHub/ketch log --oneline -8` and `git status --short`
   — compare against the **Status ledger** below. Work may exist that this file
   does not know about yet.
2. Uncommitted files in a step's scope mean a prior agent died mid-step:
   **verify them line-by-line against the Rust spec and finish them — never
   rewrite from scratch, never discard.** Prior runs left correct,
   carefully-reasoned drafts; fix what diverges, keep the rest.
3. Run the gates (below). If green, commit the step with a message explaining
   *why*, then tick the checkbox here in the same commit.
4. Engram memory (project `ketch`, topic `ketch/ts-rewrite-progress`) mirrors
   this ledger; update it when you update this file.
5. Never run two writers over the same package concurrently (including a
   second Claude session — check for one before starting).

## Status ledger

- [x] **Phase 0 — scaffold** (`d48c82f`, `43f0e84`): Moon workspace
  (`packages/schemas`, `packages/core`, `apps/cli`), mise pins (Node 26,
  Bun 1.3, Deno 2.9, pnpm 10), TS 7.0.2 strict + project references,
  oxlint linter, Biome formatter, Vitest 4, pnpm workspaces,
  Moon + Perry as root devDeps. AGENTS.md carries the toolchain table.
- [x] **Phase 1 — contracts** (`f897be8`): `@ketch/schemas` complete (every
  data file as Zod v4 + generated JSON Schemas in `packages/schemas/schemas/`;
  TOML became JSON, serde field names byte-for-byte; guards
  `sanitizeComponent`/`validateRepo`/`validateManifest` ported with tests).
  `@ketch/core` backbone: `model.ts` (full 955-line port), `errors.ts`
  (KetchError, Rust display strings pinned by test), `source/source.ts`,
  `platform/platform.ts`, `extract/extractor.ts` (`safeMemberPath`),
  `progress.ts`.
- [x] **Phase 2 — foundation** (`aee2d71`): `config.ts` (precedence: defaults <
  config.json < env < flags; empty-set token suppresses fallback; sync on
  purpose), `state.ts` (fsync-before-rename, stale lock reclaimed by rename,
  empty state.json is a loud error), LogLevel/LogFormat vocabulary.
- [x] **Phase 3 — core modules** (`eb6c97f`): extract (tar.gz/bz2/xz, zip,
  dmg/pkg via hdiutil, content sniffing), platform darwin (injectable tool
  runner), http + GitHub source, plugins + source registry, fetched registry +
  four-tier manifests, install pipeline (prepare/commit split, per-download
  staging dirs, async pool, request-order results, single-flight commit),
  lockfile, changelog, shell, pino log, selfupdate. Barrel namespaces
  `changelog`/`registry`/`selfupdate`/`shell` (flat names collide).
- [x] **Phase 4 — integration**: `tsc --build` exit 0 · vitest 238/238 ·
  oxlint exit 0 · biome clean.
- [x] **Phase 5 — the CLI** (`12f6da6`, `c968ea3`, `e02aee7`): `ui.ts` (the
  output choke point, the only caller of `log.record`, suspend-aware multi-bar,
  clack confirm), `cli.ts` + `main.ts` (commander with extra-typings, every
  command and flag from `cli.rs`, one error renderer), `cmd/{pkg,lock,query,
  system,shared}.ts`, `completions.ts` for bash/zsh/fish.
- [x] **Phase 6 — the e2e suite** (`e02aee7`): all 27 tests from
  `tests/install.rs`, claims intact, driving the real binary through a spawned
  process against a throwaway root, downloads served by the offline plugin
  fixture. **27/27 green on Node and on Bun.** The CLI's tests are inside
  `tsc --build` now, and test discovery is scoped to sources so the `dist/`
  copies are not collected.
- [ ] **Phase 7 — the site** (`apps/web`, `apps/docs`)
- [ ] **Phase 8 — docs rewrite**
- [ ] **Phase 9 — CI + release + install.sh**
- [ ] **Phase 10 — remove the Rust implementation**
- [ ] **Phase 11 — review + runtime matrix + Perry binary**
- [ ] **Phase 12 — push + PR** (only when the user says push)

Gates at `e02aee7`: `tsc --build` exit 0 · vitest **267/267** across 38 files ·
oxlint exit 0 (13 deliberate `no-await-in-loop` warnings, all sequential on
purpose) · `biome format` clean.

## The agent contract (applies to every step)

- Strict TS 7, ESM, `.ts` import specifiers, `node:` builtins only. The code
  must run on Node 26, Bun, and Deno, and compile under Perry
  (`pnpm exec perry build`). No `any`, no non-null `!`, no `console.*` outside
  `apps/cli/src/ui.ts` (oxlint enforces).
- Every file opens with a `/** … */` header saying what the module owns and
  why; comments explain *why*, never what — port the Rust comments' reasoning.
- Data files are JSON; every load validates through `@ketch/schemas`; every
  write stamps `$schema` via `schemaUrl(name)`.
- **Erasable syntax only.** Node runs TypeScript by stripping types, so no
  parameter properties, enums, namespaces or decorators — `erasableSyntaxOnly`
  enforces it. A dependency shipped as a CommonJS bundle needs
  `(await import("x")).default`, because Node cannot see named exports through
  one and Bun's interop will hide the failure from you.
- Errors: `KetchError` with the Rust display text. Guards from schemas throw
  plain `Error` — the CLI error renderer handles both.
- Tests: colocated `*.test.ts`, Vitest, names are sentences stating the claim.
  Port every Rust test in scope; add tests the port makes newly necessary.
- Trust boundaries survive by name: `safeMemberPath`, `sanitizeComponent`,
  `validateRepo`, changelog `sanitize`, manifest + lockfile validation.
- Gates, with **real exit codes** (`cmd; echo exit:$?` — never read a pipe's
  status): `pnpm exec tsc --build` · `pnpm exec vitest run` ·
  `pnpm exec oxlint .` · `pnpm exec biome format --write` then check.
  Dev loop runs on Bun (fastest); CI re-runs on Node.
- Models for delegation: Fable/Opus for thinking-heavy work (pipelines,
  parsers, security boundaries), Sonnet for well-specified ports, Haiku for
  mechanical steps. Escalate when "simple" turns out to need thought.
- One writer per package at a time. Do not edit a shared barrel while
  siblings run; list exports for the integrator instead.

## Phase 5 — the CLI (`apps/cli/src`)

Order matters: **5.1 first**, then 5.2–5.4 in parallel (they import ui.ts).

### 5.1 `ui.ts` — the output choke point *(spec: `src/ui.rs`, 463 lines)*
- `out()`/`table()` → stdout (data, pipeable). step/success/warn/error/note/
  debug → stderr, exact Rust prefixes/colors (picocolors; honor NO_COLOR,
  non-tty). `setQuiet`/`setVerbose` globals. `error()` = message + details +
  `hint:` line.
- Every status helper records to core `log` **before** the quiet check; ui is
  the ONLY caller of `record()`.
- Progress: single bar on tty (label truncated to 28, byte counts); `Bars`
  multi-bar group for parallel downloads with **suspend-print semantics**
  (status lines never interleave with live bars; per-download "fetched" lines
  suppressed inside a group); silent when quiet or not a tty. Hand-rolled
  ANSI redraw or ONE small dep.
- `confirm()` via @clack/prompts with --yes bypass and non-tty fallback.
- Port `jobs(cfg, flag)` from `cmd/pkg.rs` and ui.rs's truncate/bytes helpers.

### 5.2 `cli.ts` + `main.ts` *(spec: `src/cli.rs` 383, `src/main.rs` 103)*
- commander + @commander-js/extra-typings. EVERY command/alias/flag/help
  verbatim from cli.rs. Globals --root/--quiet/--verbose/--yes; --jobs where
  Rust has it. Version from package.json. `#!/usr/bin/env node` shebang.
- main.ts: Config from flags+env → `log.init` → ui wiring → dispatch →
  error rendering incl. hint and "the full log of this run is in …" note →
  Rust exit codes. cmd modules signatures come from `src/cmd/*.rs` pub fns.

### 5.3 `cmd/pkg.ts` + `cmd/lock.ts` *(spec: `src/cmd/pkg.rs` 363, `lock.rs` 228)*
- install: raw-arg dedupe → InstallRequests → `install.batch` (core) with the
  ui Bars factory + reporter → per-result reporting in request order → the
  "N of M packages failed" summary error. uninstall + confirmation. upgrade
  (pin skip). pin/unpin. lock write/--check; sync via requestFor + batch with
  pinned restore; --prune confirmation; --dry-run plan.
- ManifestResolver comes from `packages/core/src/manifest.ts`. Bodies stay
  thin — pipeline logic lives in core, call it.

### 5.4 `cmd/query.ts` + `cmd/system.ts` *(spec: `query.rs` 478, `system.rs` 409)*
- list/--names-only, info (--assets scored explanation), search, outdated.
- changelog: file-vs-release fallthrough EXACTLY as Rust (installed file
  first unless --release/--latest/exact-elsewhere; heading-or---file short
  circuit; release-notes fallback; warn + whole-file last resort; provenance
  via ui.step on stderr so stdout stays clean markdown).
- update (core `registry.update`), doctor (every system.rs check +
  `shell.pathCheck` + log check: path/level/format/size; --fix), path
  (status/install/uninstall/--shell/--all/--dry-run/--print), self
  (update/uninstall via core `selfupdate`).

## Phase 6 — e2e *(spec: `tests/support/mod.rs` 364, `tests/install.rs` 675)*

- `apps/cli/tests/support.ts`: Sandbox (temp KETCH_ROOT), in-code fixture
  archives (tool script + CHANGELOG), an OFFLINE `/bin/sh` source plugin
  speaking the wire protocol (docs/PLUGINS.md, `capabilities.download:true`)
  serving `file://` fixtures, publish/publishTool/publishNamed/withNotes,
  run/ok/fail spawning `[process.execPath, apps/cli/src/main.ts, …]` with
  KETCH_ROOT env (runtime-portable: node and bun both run .ts), log()/
  configure(json).
- Port ALL 27 tests from tests/install.rs, same claims. Suite skips cleanly
  while the CLI entry is missing; flip on once 5.x lands, then fix until
  green **running the real CLI**.

## Phase 7 — the site

- `apps/web`: Astro 7.2 + Tailwind 4.3 landing. Port `site/layouts/index.html`
  content (hero, install command `curl … | bash`, feature cards, command
  list, SEO: title/description/JSON-LD/OG tags, favicon.svg). Zero client JS
  except the copy buttons. Base path `/ketch/` (GitHub Pages project site).
- `apps/docs`: Docusaurus 3.10 classic, docs-only mode, sourcing the SAME
  Markdown from `docs/` (plugin `path: ../../docs`) so repo docs and site
  cannot drift; landing links to `/ketch/docs/`. Dark mode default respecting
  prefers-color-scheme.
- Build outputs compose: web at `/`, docs under `/docs/` (pages workflow
  merges `apps/web/dist` + `apps/docs/build`).

## Phase 8 — docs rewrite

- README.md: TS-first (install via installer script or npm later; monorepo
  dev commands: pnpm/moon; runtime support matrix incl. Perry; same feature
  sections, JSON examples everywhere TOML appeared).
- docs/MANIFESTS.md, REGISTRY.md, LOCKFILE.md, PLUGINS.md: JSON examples,
  `$schema` URLs, `ketch.json` naming. ROADMAP.md: refresh (Linux via
  Platform impl, npm publish, registry JSON migration note).
- AGENTS.md: full rewrite for the TS repo (layout table = monorepo, commands
  = pnpm/moon/vitest, conventions = this file's contract, trust boundaries
  unchanged by name, releasing = Perry binaries). Keep CLAUDE.md symlink.
- Registry repo (`listepo/ketch-registry`) still serves TOML — out of scope
  here; note the needed `ketch.json` migration in ROADMAP.

## Phase 9 — CI + release + installer

- `.github/workflows/ci.yml`: macOS runner; mise install; pnpm install;
  `moon ci` (typecheck/lint/format/test affected); full vitest on Node AND
  Bun; smoke the CLI under node/bun/deno (`ketch --help`, `doctor` against a
  scratch root); `perry build` the CLI and run the binary's --help.
- `.github/workflows/pages.yml`: build apps/web + apps/docs, merge, deploy.
- `.github/workflows/release.yml`: on tag — gates, then Perry-compile
  `ketch-aarch64-apple-darwin` + `ketch-x86_64-apple-darwin` tarballs +
  SHA256SUMS (same asset names as the Rust releases — `install.sh` and
  `selfupdate` depend on them). scripts/package.sh + release.sh adapted
  (version from apps/cli/package.json; release.sh bumps it + lockfile).
- install.sh: same flow, still downloads the tarball binary; version check
  now reads package.json field.

## Phase 10 — remove the Rust implementation

One commit: delete `src/`, `tests/`, `Cargo.toml`, `Cargo.lock`, `site/`
(Hugo), `.github/workflows` Rust legs, `rust-toolchain*` if any. AGENTS.md
loses its Rust sections (Phase 8 already rewrote it). Nothing else changes
in the same commit — reviewability.

## Phase 11 — verification

- Adversarial review workflow over the whole port (find → refute per finding);
  fix confirmed issues.
- Full gates; e2e on Node and Bun; `mise exec -- deno run` smoke; Perry
  binary built and driven through install/list/changelog/uninstall against a
  scratch KETCH_ROOT with the offline plugin.
- Update this ledger + engram; only then report done.

## Phase 12 — push + PR

Push `ts-rewrite`, PR against `main` describing the port, the stack, parity
status, and what deliberately changed (JSON data files, npm name, release
binary via Perry). **Only when the user asks.**
