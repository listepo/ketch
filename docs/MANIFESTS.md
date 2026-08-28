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
| Your machine | `~/.ketch/manifests/<name>.json` | just you |
| The package registry | `<package>/ketch.json` | everyone — see [REGISTRY.md](REGISTRY.md) |
| Built into ketch | `packages/schemas/src/builtin.ts` | everyone, offline |

They are searched in that order, so a manifest of your own always wins.

Manifests are JSON, validated by a Zod schema in `@ketch/schemas` that also
generates the published JSON Schema. Point `$schema` at it and your editor will
complete and check the file as you type:

```
https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/manifest.schema.json
```

## The smallest one

```json
{
  "$schema": "https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/manifest.schema.json",
  "name": "ripgrep",
  "source": "github:BurntSushi/ripgrep"
}
```

Everything else has a default. In a registry package folder the name comes from
the folder and `name` may be left out; elsewhere it is required.

## A complete one

```json
{
  "$schema": "https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/manifest.schema.json",
  "name": "ripgrep",
  "source": "github:BurntSushi/ripgrep",
  "description": "Recursively search directories for a regex pattern",
  "homepage": "https://github.com/BurntSushi/ripgrep",
  "kind": "binary",
  "provides": ["rg"],
  "prerelease": false,
  "strip_prefix": 1,
  "notes": "Shell completions are under complete/ in the payload.",
  "bin": [{ "path": "*/rg", "name": "rg" }],
  "extra_paths": ["complete/rg.bash", "doc/rg.1"],
  "asset": {
    "include": ["*-apple-darwin.tar.gz"],
    "exclude": ["*-musl-*"],
    "target": {
      "macos-aarch64": "*-aarch64-apple-darwin.tar.gz",
      "macos-x86_64": "*-x86_64-apple-darwin.tar.gz"
    }
  }
}
```

## Fields

| Key | Type | Default | What it does |
| --- | --- | --- | --- |
| `$schema` | string | none | The published JSON Schema, so editors can check the file. Ignored by ketch itself. |
| `name` | string | the folder name, in a registry package | The install name. Becomes a directory in the store and the key in `state.json`. |
| `source` | string | — **required** | `owner/repo`, or `scheme:id` for a source plugin. |
| `description` | string | none | One line, shown by `ketch info` and `ketch search`. |
| `homepage` | string | none | A URL for humans. |
| `kind` | `auto` \| `binary` \| `app` | `auto` | What the payload is. See below. |
| `provides` | array of strings | `[]` | Other names this package answers to. `ketch install rg` works because ripgrep provides `rg`. |
| `prerelease` | boolean | `false` | Consider prereleases when resolving `latest`. |
| `strip_prefix` | integer 0–8 | unwrap one wrapper dir | Leading path components to drop when extracting. |
| `notes` | string | none | Printed after a successful install. |
| `bin` | array of objects | discover | Which executables to link, and under what names. |
| `extra_paths` | array of strings | `[]` | Man pages and completions, recorded for a future release to expose. |
| `asset` | object | platform picks | Narrows which release asset is chosen. |

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

```json
{
  "bin": [
    { "name": "rg" },
    { "path": "bin/tool" },
    { "path": "*/tool-*", "name": "tool" }
  ]
}
```

The first finds a file called `rg` anywhere in the payload; the second links
that exact path as `tool`; the third globs for it and links it as `tool`.

`path` is a glob (`*` and `?`) matched against the path relative to the payload
root. `name` is the file name of the symlink in `~/.ketch/bin`.

With no `bin` at all, ketch discovers executables itself: it looks up to four
levels deep, ignores documentation directories and bundle internals, and prefers
a `bin/` directory when the payload has one. A single executable whose name
plainly carries build metadata — `jq-macos-arm64` — is linked under the package
name instead.

### `strip_prefix`

Left out, ketch unwraps a single wrapper directory, which is what almost every
release tarball has. Give a number to unwrap exactly that many levels, or `0` to
keep the payload as it came. The ceiling is 8: each level costs a directory
listing, and no real archive nests wrappers that deep.

### `asset`

```json
{
  "asset": {
    "include": ["*-apple-darwin.tar.gz"],
    "exclude": ["*-musl-*"],
    "target": {
      "macos-aarch64": "*-aarch64-apple-darwin.tar.gz"
    }
  }
}
```

Precedence, in the order it is applied:

1. `exclude` drops an asset outright and nothing can bring it back.
2. A matching `target` entry for **this host** names the file outright, so it
   wins over both `include` and the platform's own scoring.
3. Otherwise `include` filters, and the platform scores what is left.

Target keys are `<os>-<arch>` for the machine ketch is running on:
`macos-aarch64` or `macos-x86_64`. There is no `universal` key — universal is a
property of an asset, not of a host.

Reach for this only when the platform's own scoring picks wrong. Run
`ketch info <pkg> --assets` first — it lists every asset with the score it got
and the reason, which is usually enough to see what the manifest needs to say.

## Several in one file

A file in `~/.ketch/manifests/` may hold one manifest, or several under a
`package` key:

```json
{
  "package": [
    { "name": "ripgrep", "source": "github:BurntSushi/ripgrep" },
    { "name": "fd", "source": "github:sharkdp/fd" }
  ]
}
```

Which shape a file has is decided from the parsed JSON, not from its text, so a
single manifest that merely mentions "package" in a `notes` or `description`
string is still read as one manifest.

## What ketch checks

The schema checks the shape. These are the values it cannot judge, and they are
enforced identically for every tier — user manifest, registry entry, built-in:

| Rule | Why |
| --- | --- |
| `name` must be usable verbatim as one path component | It becomes a directory in the store. A name that needed sanitising would install somewhere other than where it says. |
| every `bin.name` must be usable verbatim as one path component | It becomes a symlink in `~/.ketch/bin`. |
| every `bin.path` must stay inside the payload | No absolute paths, no `..`. |
| every `extra_paths` entry must stay inside the payload | Same. |
| each `bin` entry needs `name` or `path` | An entry with neither says nothing. |
| `provides` aliases must be non-empty and whitespace-free | An alias nobody can type is not an alias. |
| `strip_prefix` must be at most 8 | Each level is a directory listing, and no real archive nests wrappers that deep. |
| `source` must parse as `scheme:id` or `owner/repo` | It becomes a URL, or the id handed to a plugin. |

A manifest that fails any of these is refused rather than repaired. In the
registry — and in your own manifest directory — one bad file is skipped with a
warning and the rest still load.

## Trying one out

Drop it in `~/.ketch/manifests/<name>.json` and install:

```bash
ketch info <name> --verbose --assets    # which tier answered, and how each asset scored
ketch install <name> --verbose          # which asset was taken, and why
```

`ketch info --verbose` names the file the manifest came from, and `--assets`
lists every release asset with its score and the reason for it. `ketch install
--verbose` reports the asset it settled on, the archive format it found, and
what the signature check said.

To offer the package to everyone, send the same file to the registry as
`<name>/ketch.json` — see [REGISTRY.md](REGISTRY.md).
