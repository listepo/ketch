# Commands

Every ketch command in one place: what it does, the form it takes, and one
working example. `PKG` below is an installed name, an alias, or
`owner/repo` — most commands that take one also accept `@version` on the end
(`sharkdp/fd@v10.2.0`). Global flags work everywhere: `--root <DIR>` points at
a different ketch tree, `-v/--verbose` shows what ketch is doing, `-q/--quiet`
prints only errors and requested data, and `--no-color` disables colour.

## Install and remove

### `ketch install [PKG]... [options]`

Install one or more packages. Inference picks the release asset matching the
host (architecture, OS, libc); a manifest or `--asset` says what to do instead.

```bash
ketch install BurntSushi/ripgrep  # any repo that publishes releases
ketch install rg                  # or a name the registry knows
ketch install sharkdp/fd@v10.2.0  # or an exact version
ketch install --path ./mytool     # a local binary, archive, symlink, or .app
```

Options: `--path <PATH>` installs a local file (equivalent to
`local:<PATH>`); `--name <NAME>` sets the installed name for a single
package; `--force/-f` reinstalls the requested version even when present;
`--pre` considers prereleases; `--no-link` unpacks without linking onto PATH;
`--require-checksum` refuses releases with no published checksum;
`--asset <NAME>` takes one release file by exact name (asks for confirmation,
since the platform check is skipped); `-j/--jobs <N>` and `-y/--yes` control
parallelism and prompts. Aliased as `ketch i`.

### `ketch uninstall <NAME>...`

Remove installed packages. Names resolve like `install` (installed name,
binary, or `owner/repo`); a typo stops the command before anything is removed.

```bash
ketch uninstall rg
ketch uninstall fd ripgrep --yes  # skip the confirmation
```

Aliased as `ketch remove` and `ketch rm`.

### `ketch upgrade [NAME]...`

Upgrade installed packages to their latest release. Empty means every
unpinned package. Shows a `from -> to` table, asks, then installs.

```bash
ketch upgrade              # everything unpinned
ketch upgrade ripgrep fd   # just these
ketch upgrade --dry-run    # show the plan, change nothing
```

Options: `--pre` considers prereleases; `--force` upgrades pinned packages
too; `-j/--jobs <N>`, `-y/--yes`. Pinned and `local:` packages are skipped
unless forced.

### `ketch rollback <PKG> [--to <VERSION>]`

Switch a package back to a version still on disk. Without `--to`, restores the
previous retained version.

```bash
ketch rollback ripgrep
ketch rollback rg --to 14.1.0
```

### `ketch prune [NAME]... [--keep <N>]`

Remove retained old versions according to the retention policy. Empty means
every installed package. `--keep <N>` updates the stored policy.

```bash
ketch prune
ketch prune ripgrep --keep 2
```

### `ketch pin <NAME>...` / `ketch unpin <NAME>...`

Hold a package at its current version (`pin`), or release the hold (`unpin`).
Pinned packages are skipped by `upgrade` unless `--force` is passed.

```bash
ketch pin ripgrep
ketch upgrade --force        # upgrades pinned packages too
ketch unpin ripgrep
```

### `ketch link <NAME>...` / `ketch unlink <NAME>...`

Re-create (`link`) or remove (`unlink`) the bin-dir links for an installed
package. `unlink` keeps the payload installed; `upgrade` preserves the linked
state.

```bash
ketch unlink ripgrep   # keep it, take it off PATH
ketch link ripgrep     # put it back
```
## Inspect

### `ketch list [--json] [--names-only]`

Show installed packages as a `package / version / source` table.

```bash
ketch list
ketch list --names-only   # one name per line, for scripts
ketch list --json         # JSON, for scripts
```

Aliased as `ketch ls`.

### `ketch outdated [--json] [--pre]`

Show installed packages that have a newer release. A source that cannot be
reached is a warning, not a failure — unless nothing could be checked at all.

```bash
ketch outdated
ketch outdated --json   # {"status","outdated","failed","unreachable"}
```

### `ketch info <PKG> [--assets] [--json]`

Show details about a package, installed or not: source, description, latest
and installed versions, links, retention, verification.

```bash
ketch info BurntSushi/ripgrep
ketch info rg --assets   # every release asset with its score and why
```

Aliased as `ketch show`.

### `ketch why <PKG> [--json]`

Explain how a package would be resolved, without installing it: which manifest
tier answered, which release and asset were picked, and why.

```bash
ketch why sharkdp/fd@v10.2.0
ketch why rg --json
```

### `ketch changelog <PKG> [--latest] [--file] [--release]`

Show what changed: the changelog file the package ships, or the release notes
published with the release. Prefers the file on disk for the installed
version; falls back to the notes.

```bash
ketch changelog ripgrep            # installed version
ketch changelog ripgrep --latest   # newest release instead
ketch changelog ripgrep --file     # only the shipped file
ketch changelog ripgrep --release  # only the published notes
```

### `ketch search <QUERY>... [-n <LIMIT>]`

Search GitHub (and searching plugins) for installable repositories.

```bash
ketch search fuzzy finder
ketch search ripgrep -n 5
```

## Reproduce

### `ketch lock [--file <FILE>] [--check]`

Write `./ketch.lock` from what is installed — the reproducible record of the
machine's tools, pinned to exact releases. See [LOCKFILE.md](LOCKFILE.md).

```bash
ketch lock              # write ./ketch.lock
ketch lock --check      # has the tree drifted from it?
ketch lock -f tools.lock
```

### `ketch sync [--file <FILE>] [--prune] [--dry-run]`

Install everything `ketch.lock` names, at the versions it names. A package the
lockfile does not mention is left alone unless `--prune` is passed (which asks
first, since it can lose work).

```bash
ketch sync                 # missing or drifted packages only
ketch sync --dry-run       # show the +/~/− plan, change nothing
ketch sync --prune --yes   # also remove extras, without asking
```

## History

### `ketch history [PKG] [-n <LIMIT>] [--json]`

Show what was installed, upgraded and removed, newest first. Omit the package
for the whole tree in one timeline.

```bash
ketch history
ketch history ripgrep -n 10
```

### `ketch stats [--json]`

Summarise everything ketch has recorded in `stats.db`.

```bash
ketch stats
ketch stats --json
```

## Registry and sources

### `ketch update`

Refresh the package registry into `~/.ketch/registry`. (For installed
packages, see `upgrade`.)

```bash
ketch update
```

Runs automatically at the start of `install` and `upgrade` unless
`auto_update` is `false` in `~/.ketch/config.toml`
(`KETCH_AUTO_UPDATE=false`).

### `ketch registry validate [DIR] [--fixture <DIR>] [--changed <NAME>] [--json]`

Validate a registry tree the way CI will: every `ketch.toml` parsed and
checked, plus name collisions. `--fixture` offline-installs entries against
local files as extra proof.

```bash
ketch registry validate
ketch registry validate ./ketch-registry --fixture ./fixtures
```

### `ketch registry status [--json]`

Show the local registry copy's age and source, without fetching.

```bash
ketch registry status
```

### `ketch registry push [--file <FILE>] [--registry <REPO>] [--dry-run] [--yes]`

Compare this project's `ketch.toml` with the registry's copy and open a pull
request with it. Opens straight away for a new package; shows a diff and asks
for an update; reports `unchanged` and opens nothing when identical. See
[REGISTRY.md](REGISTRY.md).

```bash
KETCH_GITHUB_TOKEN=... ketch registry push
ketch registry push --dry-run
```

### `ketch plugin list [--json]` / `ketch plugin dir`

Show discovered source plugins (`ketch-source-<scheme>` in `~/.ketch/plugins`
and on `PATH`), or print the plugins directory. See [PLUGINS.md](PLUGINS.md).

```bash
ketch plugin list
ketch plugin dir
```

## Environment

### `ketch doctor [--fix] [--json]`

Check the environment and the install tree: version, PATH setup, platform
checks, log, registry age, store against `state.json`. Exits non-zero when a
check fails.

```bash
ketch doctor
ketch doctor --fix    # repair what can be repaired (the PATH setup)
```

### `ketch path [install|uninstall|status]`

Put the ketch bin directory on PATH. Bare `ketch path` shows the status
table; that is the default subcommand.

```bash
ketch path                  # bin dir, PATH state, per-shell table
ketch path install          # edit shell startup files (asks per shell)
ketch path install --print  # print the line to add by hand
ketch path install --all    # act on every known shell
ketch path uninstall        # take the block back out again
```

### `ketch config create [--file <FILE>] [--force] [--yes]`

Write a package config (`ketch.toml`) by answering questions — source, name,
bin entries, asset patterns — then preview and write the file. Answers can be
piped on stdin, one per line. See [MANIFESTS.md](MANIFESTS.md).

```bash
ketch config create
ketch config create --file ./ketch.toml --yes
```

### `ketch completions <SHELL> [--install]`

Print a shell completion script, or install it into the shell's user
completion directory with `--install`.

```bash
ketch completions zsh > _ketch
ketch completions bash --install
```

## ketch itself

### `ketch self install [--force] [--link-dir <DIR>]`

Install this release of ketch as a package, into the store and bin dir. What
the curl/PowerShell/Homebrew installers run.

```bash
ketch self install
```

### `ketch self upgrade [--dry-run] [--force] [--yes]`

Upgrade ketch to the latest release. Aliased as `ketch self update`.

```bash
ketch self upgrade
ketch self upgrade --dry-run
```

### `ketch self version`

Print the running version and where it lives (target, root, binary, PATH).

```bash
ketch self version
```

### `ketch self uninstall [--keep-packages] [--dry-run] [--yes]`

Remove ketch and everything it installed, permanently. Lists what it is about
to delete and asks first. `--keep-packages` removes only ketch.

```bash
ketch self uninstall --dry-run
ketch self uninstall --keep-packages
```

