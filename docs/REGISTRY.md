# The package registry

The registry is an ordinary GitHub repository. Every top-level folder is a
package, and holds one `ketch.toml` describing it:

```
ketch-registry/
├── README.md          ← not a package: no ketch.toml
├── fd/
│   └── ketch.toml
├── jq/
│   └── ketch.toml
└── ripgrep/
    └── ketch.toml
```

The folder name *is* the package name — it is what `ketch install <name>`
matches. There is no index file to keep in step with the contents, and a
folder without a `ketch.toml` is simply not a package, so the repository can
carry a README, a licence and CI config alongside the packages.

## A package file

```toml
# ripgrep/ketch.toml
source      = "github:BurntSushi/ripgrep"
description = "Recursively search directories for a regex pattern"
homepage    = "https://github.com/BurntSushi/ripgrep"
bin         = [{ name = "rg" }]
provides    = ["rg"]
```

`source` is the only required field. The rest are the same fields a user
manifest in `~/.ketch/manifests/` takes — [MANIFESTS.md](MANIFESTS.md) is the
full schema, and `src/builtin.toml` is a working example of each.

`name` may be given, but it must equal the folder name — a package that
disagrees with its folder would be unreachable under the name the folder
advertises, so ketch refuses it rather than quietly indexing it twice.

## What ketch checks

A registry entry is code someone else wrote that ends up creating files on a
stranger's disk, so it is checked before it is trusted:

| Refused | Why |
| --- | --- |
| an unknown key (`binary = …` for `bin = …`) | a misspelt key that is silently ignored installs the wrong thing and complains nowhere |
| a `local:` source | a registry entry must name a release anyone can fetch, not a path on one machine's disk |
| `name` that disagrees with the folder | the package would be unreachable under the name its folder advertises |
| a `name` or `bin.name` that is not a usable file name | `name` becomes a directory in the store and `bin.name` a link in `~/.ketch/bin`; `../../.zshrc` is not a binary |
| a `bin.path` or `extra_paths` entry containing `..` | paths are relative to the extracted payload and must stay inside it |
| a `bin` entry with neither `name` nor `path` | it describes nothing to link |
| a `provides` entry with whitespace | nobody can type it |
| a `strip_prefix` above 8 | each level is a directory listing of the payload, and no real archive nests wrappers that deep |

A folder that fails is reported and skipped on a machine that already has the
registry, so one bad entry never takes the rest of the copy down with it.
`ketch update` also warns when two packages claim the same name — each folder
is valid alone, but only one of them would ever resolve. Clients stay
best-effort about an already-published registry: they warn and skip, they do
not refuse the whole tree.

Before merge, the same problems are fatal. `ketch registry validate` turns
parse errors, validation errors, and name or alias collisions into a failing
run so they cannot land.

[`Manifest::validate`](MANIFESTS.md) also applies to `~/.ketch/manifests/*.toml`.
Registry entries must additionally pass the `local:` and folder-name rules in
the table above — rules user manifests never see — so a manifest that installs
locally is not necessarily one that can be contributed as-is.

## Using it

```bash
ketch update             # fetch the registry into ~/.ketch/registry
ketch registry status    # age and source of the local copy; no network
ketch search fd          # search it, alongside GitHub
ketch install fd         # install by name
ketch doctor             # registry line includes package count, age, and revision
```

Nothing fetches the registry implicitly: a name that resolves today keeps
resolving offline tomorrow, and `ketch install owner/repo` never needs it at
all. `ketch update` is the only refresh. `ketch registry status` and `ketch
doctor` read the local copy and `registry.meta.toml` (source revision and
fetch time, stored under the ketch root beside the manifests) and never
open a network connection.

## Author and maintainer workflow

The exact validation command, run from the registry repository root (or any
tree laid out the same way):

```bash
ketch registry validate .
```

That is what must pass before merge: every `ketch.toml` parsed, checked
against its folder name, refused when `source` is `local:`, passed through
[`Manifest::validate`](MANIFESTS.md), with name and alias collisions failing
the run.

To also offline-install the entries a pull request changes, put one local
asset per package in a fixture directory — a file named after the package,
or a folder of that name holding exactly one file — and name the packages:

```bash
ketch registry validate . --fixture ./ci/fixtures --changed ripgrep --changed fd
```

Without `--changed`, every package that has a fixture is installed. `--changed`
without `--fixture` is an error. The install runs in a throwaway ketch root
from the fixture file, so a `github:` source in the entry never reaches the
network. A missing binary, a bad `bin.path`, or an archive that will not
unpack fails the same way as a collision.

### Compatibility

- **Registry CI / `ketch registry validate`** is fail-closed. Invalid or
  colliding entries do not merge.
- **Clients** (`ketch update`, lookup, install by name) stay best-effort
  about an already-published bad registry: they warn, skip the broken folder,
  and keep resolving the rest. They never fetch as a side effect of status
  or doctor.
- A fixture install is extra proof for changed entries. It does not replace
  the tree-wide collision scan.

The `ketch-registry` repository still needs a workflow that runs this command
on every pull request.

`registry push` runs those same per-package checks on the file it is about to
send. It does not scan the rest of the registry for name collisions — only CI
and `registry validate` do that. It does not offline-install against a fixture.

## Contributing a package

A `ketch.toml` at the root of a project is the same file its registry folder
would hold, so contributing it is one command, run from that root:

```bash
KETCH_GITHUB_TOKEN=... ketch registry push            # fetch, compare, then a pull request
ketch registry push --yes                             # answer the question in advance
ketch registry push --dry-run                         # what would be pushed, offline
ketch registry push --file path/to/ketch.toml         # a file somewhere else
ketch registry push --registry someone/their-registry # a registry other than the default
```

After the file passes, `registry push` fetches the registry's current copy
of `<name>/ketch.toml` from the registry's default branch, and what happens
next depends on what that holds:

| Registry's copy | What `registry push` does |
| --- | --- |
| does not exist | opens the pull request straight away |
| is byte for byte the same | reports `unchanged` and opens nothing |
| differs | prints a unified diff — its copy as the old side, the local file as the new — and opens the update pull request only after yes/no |

It then puts the file at `<name>/ketch.toml` on a branch called `ketch/<name>` and opens a pull
request against the registry's default branch. With push access to the
registry the branch is made there; without it, on a fork of the registry
under your account, created if needed. Pushing again updates the same branch,
and joins the pull request that is already open rather than opening a second.

`--yes` answers the question in advance — for scripts and CI, where there is
no one to ask, or for when you already know what the diff says. `--dry-run`
shows what would be pushed without touching the network.

`name` may be left out of the file, in which case the folder it sits in names
the package — the same rule the registry applies. The file goes up exactly as
written, comments included.

The token is the one every other command uses (`KETCH_GITHUB_TOKEN`, or
`github_token` in `config.toml`); it needs the `repo` scope, or for a
fine-grained token, contents and pull requests on the registry — and, if a
fork is involved, permission to create one.

## Precedence

A name is resolved against, in order:

1. `~/.ketch/manifests/<name>.toml` — your own manifests
2. the fetched registry
3. the registry compiled into the binary (`src/builtin.toml`)
4. inference from `owner/repo`

So a local manifest always wins, and a package curated in the registry beats
the older copy baked into whatever ketch build you happen to be running.

## Pointing at a different registry

```bash
export KETCH_REGISTRY=someone/their-registry
```

or in `~/.ketch/config.toml`:

```toml
registry = "someone/their-registry"
```

The default is `listepo/ketch-registry`. Only `owner/repo` is accepted; the
repository's default branch is what gets fetched.
