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

When a release ships several binaries sharing the package's name and no
manifest names one, only one of them is linked: the one `--bin <NAME>` names;
failing that, the binary named exactly like the package; failing that, a choice
remembered from last time (in `state.json`, or during `ketch sync` in
`ketch.lock`), or a numbered pick in a terminal. Binaries with other names are
linked as usual. With `--yes` or without a terminal there is no pick, and the
install fails with the candidates listed — see the `bin` section of
[MANIFESTS.md](MANIFESTS.md).

`--bin <NAME>` (single package) answers that question up front, so it works
without a terminal: `ketch install owner/rtok --bin rtok-cli`. It is stored in
`state.json` like a pick and reused on later upgrades. It fails when no binary in
the release has that name, and when the package's manifest already has a `bin`
(change that instead).

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
too; `--bin <NAME>` (exactly one package) picks which binary to link when the
new release ships several sharing the package's name, as on `install`;
`-j/--jobs <N>`, `-y/--yes`. Pinned and `local:` packages are skipped unless
forced.

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

### `ketch list`

`ketch list [local|remote] [--json] [--names-only]`, aliased as `ketch ls`.

Show what is installed, what the registry offers, or both in one table with
the newest version of each.

| Command | Shows | Network |
| --- | --- | --- |
| `ketch list local` | Installed packages, from the install record | Never |
| `ketch list remote` | Packages the registry offers, with their latest version | Needed; fails without it |
| `ketch list` | Both, one row per package, sorted by name | Used for `latest`; without it, the local part |

"The registry" here is the local copy `ketch update` fetches, plus your own
manifests in `~/.ketch/manifests` (yours win when a name is in both). The few
packages compiled into ketch as a fallback are not listed; run `ketch update`
to see the registry they come from.

**Columns.**

- `package` — the name to install, upgrade or uninstall it by.
- `installed` — the installed version. `(pinned)` means `ketch pin` holds it;
  `(+N retained)` means N earlier versions are kept for `ketch rollback`.
  Empty for a package that is not installed.
- `latest` — the newest release of the package's source. `(update available)`
  follows it when that is an update for the installed version. `?` means the
  source did not answer. Empty for a package installed from a local path,
  which has no upstream to ask.
- `source` — where the package comes from. For an installed package, that is
  where it was installed from, and where its next version will come from.
- `description` (`remote` only) — the registry's one-line summary, cut to the
  terminal width (`COLUMNS` sets another width; nothing is cut when the output
  is piped).

**Markers.** In `ketch list`, an installed package has `*` in the first column.
On a colour terminal the marker is `●`, the name is bold, and
`(update available)` is yellow. `CLICOLOR_FORCE=1` asks for colour even into a
pipe; `--no-color` and `NO_COLOR` turn it off.

**Update available** is decided the way `ketch outdated` decides it. `latest` is
the newest release the package's source reports, from the same lookup
`ketch outdated` makes: stable releases only, unless `prerelease = true` is set
in `config.toml` or in the package's manifest. That version is compared with
the installed one, and only a strictly newer version counts; a release with
the installed tag never does. A pinned package is never offered an update.

**Pinned packages** are listed with both versions and `(pinned)`, get no
`(update available)`, and are left out of the footer. `ketch unpin` lets them
move again.

**Packages from outside the registry** — installed from `owner/repo` or
another `scheme:id` reference — are listed too, and their `latest` comes from
their own source. One installed from a local path (`ketch install --path`) is
listed with an empty `latest`.

Under the table, `N updates available: ketch upgrade <names>` names every
package with an update, as the command that takes them all.

**Offline, and packages that do not answer.** Latest versions are looked up in
parallel (`jobs` at a time, 4 by default), with a counter on stderr while they
load. A source that fails — rate-limited, gone, unreachable — shows `?` in
`latest`, and one line under the table names each such package; every other row
is still printed and the command still succeeds. When no source answers at all,
the network is taken to be missing: `ketch list` prints the `ketch list local`
table followed by `latest: offline` and exits 0, while `ketch list remote`,
which has nothing to show without it, exits non-zero.

Answers are cached for 10 minutes in `~/.ketch/cache/latest.json`, per source
and per prerelease setting, so listing twice in a row asks GitHub once. Only
answers are cached, never failures. Delete the file to ask again sooner;
`ketch outdated` and `ketch upgrade` never read it.

**`--json`** prints, for each mode:

- `local` — an array of `{"name", "installed", "pinned", "retained", "source"}`,
  with `retained` the list of versions kept for rollback.
- `remote` — an array of `{"name", "latest", "description", "source"}`, with
  `latest` `null` for a source that did not answer.
- no mode — `{"packages": [...], "unreachable": [...]}`. Each package is
  `{"name", "installed", "latest", "update_available", "pinned", "source"}`;
  `installed` is `null` when it is not installed and `latest` is `null` when
  it is unknown. `unreachable` names the packages whose `latest` is unknown.
  Offline, `packages` holds only installed packages.

**`--names-only`** prints the names alone, one per line, for each mode; it never
touches the network.

**Changed in 0.7.** `ketch list` used to show installed packages only, and its
`--json` was an array of install records; it now shows everything, in the
shape above. Scripts that want the installed packages should call
`ketch list local`. `ketch list --installed` is a hidden alias of
`ketch list local`, kept for one release.

```text
$ ketch list local
package  installed        source
fd       v10.4.2          github:sharkdp/fd
ripgrep  14.1.1           github:BurntSushi/ripgrep
rtok     v0.9.0 (pinned)  github:pyrlyn/rtok
```

```text
$ ketch list remote
package  latest   description
cox      v0.1.0   Modular terminal coding agent
dunnage  v0.1.0   Shrink Cargo target directories without slowing builds
ketch    v0.6.1   Catch releases straight from GitHub
ripgrep  15.2.0   Recursively search directories for a regex pattern
rtok     v0.10.0  Reduce the context AI coding agents must carry
runa     ?        Run AI models locally (GGUF via llama.cpp) or through the OpenAI and Anthropic APIs
? means the latest release could not be checked: runa
```

```text
$ ketch list
   package  installed        latest                      source
   cox                       v0.1.0                      github:pyrlyn/cox
   dunnage                   v0.1.0                      github:listepo/dunnage
*  fd       v10.4.2          v10.5.0 (update available)  github:sharkdp/fd
   ketch                     v0.6.1                      github:pyrlyn/ketch
*  ripgrep  14.1.1           15.2.0 (update available)   github:BurntSushi/ripgrep
*  rtok     v0.9.0 (pinned)  v0.10.0                     github:pyrlyn/rtok
   runa                      ?                           github:pyrlyn/runa
2 updates available: ketch upgrade fd ripgrep
? means the latest release could not be checked: runa
```

Without a network:

```text
$ ketch list
package  installed        source
fd       v10.4.2          github:sharkdp/fd
ripgrep  14.1.1           github:BurntSushi/ripgrep
rtok     v0.9.0 (pinned)  github:pyrlyn/rtok
latest: offline
$ ketch list remote
     error could not reach any package source to check the latest versions; `ketch list local` works offline
```

```bash
ketch list local --json      # installed packages, for scripts
ketch list --names-only      # every package name, one per line
ketch list remote --json     # the registry with latest versions
```

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

Validate a registry tree the way the pre-push hook does: every `ketch.toml` parsed and
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
check fails. On Windows it also warns about user PATH entries that name a ketch
bin dir whose folder is gone — this root's, or any `.ketch\bin`.

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
bin entries, asset patterns — then preview and write the file. A package that
links binaries must name the command it puts on PATH, so the first `bin` entry
is asked for rather than offered; its name defaults to the package name.
Answers can be piped on stdin, one per line. See [MANIFESTS.md](MANIFESTS.md).

```bash
ketch config create
ketch config create --file ./ketch.toml --yes
```

### `ketch config reset [--yes]`

Write `config.toml` in the ketch root with the compiled defaults. Asks first
unless `--yes`; backs the existing file up beside itself as
`config.toml.bak-<unix-seconds>` unless it is missing or already matches a
sibling backup.

```bash
ketch config reset
ketch config reset --yes
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

Upgrade ketch to the latest release. Aliased as `ketch self update`. A ketch
installed with mise and never `self install`ed is mise's to upgrade: this
refuses, and names `mise upgrade`.

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
to delete and asks first. `--keep-packages` removes only ketch. A ketch
installed with mise also asks whether to run `mise unuse -g` for its own copy;
`--yes` answers that too.

On Windows it also removes what ketch wrote to the registry. Today that is
only the bin dir in the user PATH (`HKCU\Environment\Path`), written by
`install.ps1` or `ketch path install`. The entry is matched however it is
spelled: case, quotes, `/` or `\`, a trailing separator, or an 8.3 short
name. `--keep-packages` leaves it, as it leaves the shell blocks, because the
packages still in the bin dir need it. ketch never registers itself in Apps &
Features, so there is no entry there to remove.

```bash
ketch self uninstall --dry-run
ketch self uninstall --keep-packages
```

