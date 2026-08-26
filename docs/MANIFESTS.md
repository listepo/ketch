# Manifests

A manifest is the config for one package: where it comes from, which release
asset to take, and what to expose once it is unpacked.

You rarely need one. ketch infers a manifest from `owner/repo` alone, and that
is enough for most projects. Write one when inference gets it wrong — an
unusually named asset, a binary that should be linked under a different name, an
`.app` bundle, a short alias worth remembering.

The same schema is used in three places:

| Where | File | Applies to |
| --- | --- | --- |
| Your machine | `~/.ketch/manifests/<name>.toml` | just you |
| The package registry | `<package>/ketch.toml` | everyone — see [REGISTRY.md](REGISTRY.md) |
| Built into ketch | `src/builtin.toml` | everyone, offline |

They are searched in that order, so a manifest of your own always wins.

## The smallest one

```toml
source = "github:BurntSushi/ripgrep"
```

Everything else has a default. In a registry package folder the name comes from
the folder; elsewhere `name` is required.

## A complete one

```toml
name = "ripgrep"
source = "github:BurntSushi/ripgrep"
description = "Recursively search directories for a regex pattern"
homepage = "https://github.com/BurntSushi/ripgrep"
kind = "binary"
provides = ["rg"]
prerelease = false
strip_prefix = 1
notes = "Shell completions are under complete/ in the payload."

bin = [{ path = "*/rg", name = "rg" }]
extra_paths = ["complete/rg.bash", "doc/rg.1"]

[asset]
include = ["*-apple-darwin.tar.gz"]
exclude = ["*-musl-*"]

[asset.target]
"macos-aarch64" = "*-aarch64-apple-darwin.tar.gz"
"macos-x86_64" = "*-x86_64-apple-darwin.tar.gz"
```

## Fields

| Key | Type | Default | What it does |
| --- | --- | --- | --- |
| `name` | string | the folder name, in a registry package | The install name. Becomes a directory in the store and the key in `state.json`. |
| `source` | string | — **required** | `owner/repo`, or `scheme:id` for a source plugin. |
| `description` | string | none | One line, shown by `ketch info` and `ketch search`. |
| `homepage` | string | none | A URL for humans. |
| `kind` | `auto` \| `binary` \| `app` | `auto` | What the payload is. See below. |
| `provides` | list of strings | empty | Other names this package answers to. `ketch install rg` works because ripgrep provides `rg`. |
| `prerelease` | bool | `false` | Consider prereleases when resolving `latest`. |
| `strip_prefix` | integer 0–8 | unwrap one wrapper dir | Leading path components to drop when extracting. |
| `notes` | string | none | Printed after a successful install. |
| `bin` | list of tables | discover | Which executables to link, and under what names. |
| `extra_paths` | list of strings | empty | Man pages and completions, recorded for a future release to expose. |
| `asset` | table | platform picks | Narrows which release asset is chosen. |

Unknown keys are an error, not a warning. A misspelt key that is silently
ignored gives you a package that installs the wrong thing and says nothing.

### `kind`

- `auto` — look at the payload. An `.app` bundle makes it an app; anything else
  is treated as binaries.
- `binary` — link executables onto `PATH`, and never place an `.app`.
- `app` — place the `.app` bundle and do **not** scatter its executables across
  `PATH`.

### `bin`

Each entry needs `path`, `name`, or both.

```toml
bin = [
  { name = "rg" },                    # find a file called `rg` in the payload
  { path = "bin/tool" },              # link this exact path, as `tool`
  { path = "*/tool-*", name = "tool" }, # glob it, link it as `tool`
]
```

`path` is a glob (`*` and `?`) matched against the path relative to the payload
root. `name` is the file name of the symlink in `~/.ketch/bin`.

With no `bin` at all, ketch discovers executables itself: it looks up to four
levels deep, ignores documentation directories and bundle internals, and prefers
a `bin/` directory when the payload has one. A single executable whose name
plainly carries build metadata — `jq-macos-arm64` — is linked under the package
name instead.

### `asset`

```toml
[asset]
include = ["*-apple-darwin.tar.gz"]   # must match at least one
exclude = ["*-musl-*"]                # must match none

[asset.target]
"macos-aarch64" = "*-aarch64-apple-darwin.tar.gz"
```

Precedence, in the order it is applied:

1. `exclude` drops an asset outright and nothing can bring it back.
2. A matching `asset.target` entry for **this host** names the file outright, so
   it wins over both `include` and the platform's own scoring.
3. Otherwise `include` filters, and the platform scores what is left.

Target keys are `<os>-<arch>` for the machine ketch is running on:
`macos-aarch64` or `macos-x86_64`. There is no `universal` key — universal is a
property of an asset, not of a host.

Reach for this only when the platform's own scoring picks wrong. Run
`ketch info <pkg> --assets` first — it lists every asset with the score it got
and the reason, which is usually enough to see what the manifest needs to say.

## What ketch checks

Serde checks the shape. These are the values it cannot judge:

| Rule | Why |
| --- | --- |
| `name` must be usable verbatim as one path component | It becomes a directory in the store. A name that needed sanitising would install somewhere other than where it says. |
| every `bin.name` must be usable verbatim as one path component | It becomes a symlink in `~/.ketch/bin`. |
| every `bin.path` must stay inside the payload | No absolute paths, no `..`. |
| every `extra_paths` entry must stay inside the payload | Same. |
| each `bin` entry needs `name` or `path` | An entry with neither says nothing. |
| `provides` aliases must be non-empty and whitespace-free | An alias nobody can type is not an alias. |
| `strip_prefix` must be at most 8 | Each level is a directory listing, and no real archive nests wrappers that deep. |

A manifest that fails any of these is refused rather than repaired. In the
registry, one bad package is skipped with a warning and the rest still load.

## Trying one out

Drop it in `~/.ketch/manifests/<name>.toml` and install:

```bash
ketch install <name> --verbose
```

`--verbose` reports which tier the manifest came from, which asset was picked
and why, and what got linked. To offer it to everyone, send the same file to the
registry as `<name>/ketch.toml` — see [REGISTRY.md](REGISTRY.md).
