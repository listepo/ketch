# The package registry

The registry is an ordinary GitHub repository. Every top-level folder is a
package, and holds one `ketch.json` describing it:

```
ketch-registry/
├── README.md          ← not a package: no ketch.json
├── fd/
│   └── ketch.json
├── jq/
│   └── ketch.json
└── ripgrep/
    └── ketch.json
```

The folder name *is* the package name — it is what `ketch install <name>`
matches. There is no index file to keep in step with the contents, and a
folder without a `ketch.json` is simply not a package, so the repository can
carry a README, a licence and CI config alongside the packages.

## A package file

```json
{
  "$schema": "https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/registry.schema.json",
  "source": "github:BurntSushi/ripgrep",
  "description": "Recursively search directories for a regex pattern",
  "homepage": "https://github.com/BurntSushi/ripgrep",
  "bin": [{ "name": "rg" }],
  "provides": ["rg"]
}
```

`source` is the only required field. The rest are the same fields a user
manifest in `~/.ketch/manifests/` takes — [MANIFESTS.md](MANIFESTS.md) is the
full schema, and `packages/schemas/src/builtin.ts` is a working example.

A registry entry has its own published JSON Schema, which differs from the
manifest one in exactly one way: `name` is optional, because the folder already
supplies it.

```
https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/registry.schema.json
```

`name` may be given, but it must equal the folder name — a package that
disagrees with its folder would be unreachable under the name the folder
advertises, so ketch refuses it rather than quietly indexing it twice. The
comparison is on normalized names: lowercased, with a trailing `.rs` or `.git`
dropped, so a folder called `ripgrep` and a `name` of `ripgrep.rs` agree.

## What ketch checks

A registry entry is code someone else wrote that ends up creating files on a
stranger's disk, so it is checked before it is trusted:

| Refused | Why |
| --- | --- |
| an unknown key (`binary` for `bin`) | a misspelt key that is silently ignored installs the wrong thing and complains nowhere |
| `name` that disagrees with the folder | the package would be unreachable under the name its folder advertises |
| a `name` or `bin.name` that is not a usable file name | `name` becomes a directory in the store and `bin.name` a link in `~/.ketch/bin`; `../../.zshrc` is not a binary |
| a `bin.path` or `extra_paths` entry containing `..` | paths are relative to the extracted payload and must stay inside it |
| a `bin` entry with neither `name` nor `path` | it describes nothing to link |
| a `provides` entry with whitespace | nobody can type it |
| a `strip_prefix` above 8 | each level is a directory listing of the payload, and no real archive nests wrappers that deep |
| a `source` that is not `scheme:id` or `owner/repo` | it becomes a URL, or the id handed to a plugin |

A folder that fails is reported and skipped, so one bad entry never takes the
rest of the registry down with it. `ketch update` also warns when two packages
claim the same name — each folder is valid alone, but only one of them would
ever resolve.

The same checks apply to `~/.ketch/manifests/*.json`, so a manifest that works
locally is one that can be contributed as-is.

## Using it

```bash
ketch update          # fetch the registry into ~/.ketch/registry
ketch search fd       # search it, alongside GitHub
ketch install fd      # install by name
ketch doctor          # shows which registry is in use and how many packages
```

`ketch update` downloads the repository's default branch as a tarball, unpacks
it to a staging directory, and only swaps it into `~/.ketch/registry` once it
parses — a truncated or malformed fetch leaves the working copy alone.

Nothing fetches the registry implicitly: a name that resolves today keeps
resolving offline tomorrow, and `ketch install owner/repo` never needs it at
all. Run `ketch update` when a package is missing or out of date.

## Precedence

A name is resolved against, in order:

1. `~/.ketch/manifests/<name>.json` — your own manifests
2. the fetched registry
3. the registry compiled into ketch (`packages/schemas/src/builtin.ts`)
4. inference from `owner/repo`

So a local manifest always wins, and a package curated in the registry beats
the older copy baked into whatever ketch build you happen to be running.

An explicit `owner/repo` still picks up a curated manifest when one exists:
`ketch install BurntSushi/ripgrep` links `rg`, because that is what the registry
entry says to do. Inference is the last resort, not the answer to any reference.

## Pointing at a different registry

```bash
export KETCH_REGISTRY=someone/their-registry
```

or in `~/.ketch/config.json`:

```json
{ "registry": "someone/their-registry" }
```

The default is `listepo/ketch-registry`. Only `owner/repo` is accepted; the
repository's default branch is what gets fetched.
