# The lockfile

`ketch.lock` is one machine's set of tools, pinned to exact releases. Write it,
commit it next to your dotfiles, and any other machine can reproduce the same
set:

```bash
ketch lock              # write ./ketch.lock from what is installed
ketch lock --check      # has the tree drifted from it?
ketch sync              # install what the lockfile names, at those versions
```

It is not the lock ketch takes while it works — that one is a mutex over the
install tree, held for the length of a command and released at the end.

Despite the name it is a JSON file, validated by the same schema machinery as
every other data file ketch reads.

## What one looks like

```json
{
  "$schema": "https://raw.githubusercontent.com/listepo/ketch/main/packages/schemas/schemas/lockfile.schema.json",
  "version": 1,
  "package": [
    {
      "name": "fd",
      "source": "github:sharkdp/fd",
      "version": "10.2.0",
      "tag": "v10.2.0",
      "target": "macos-aarch64",
      "asset": "fd-v10.2.0-aarch64-apple-darwin.tar.gz",
      "sha256": "d3f0e1a5b3ee2b6d0a1a6d0dd6c72fd0e69f6bb7c5b3a2a94f0e5b9a2f8b8c11",
      "pinned": true
    },
    {
      "name": "ripgrep",
      "source": "github:BurntSushi/ripgrep",
      "version": "14.1.1",
      "tag": "14.1.1",
      "target": "macos-aarch64",
      "asset": "ripgrep-14.1.1-aarch64-apple-darwin.tar.gz",
      "sha256": "4cf9f2741e6c465ffdb7c26f38056a59e2a2544b51f7cc128ef28337eeae4d8e",
      "pinned": false
    }
  ]
}
```

Packages are written sorted by name, so the file is stable and its diffs are
readable. `ketch lock` writes it atomically — staged, fsynced, then renamed —
so an interrupted write cannot leave half a lockfile where a whole one used to
be.

| Key | What it is |
| --- | --- |
| `$schema` | the published JSON Schema, so editors can check the file |
| `version` | the lock format version, currently `1` |
| `package` | the entries, sorted by `name` |

Each entry:

| Key | Type | What it is |
| --- | --- | --- |
| `name` | string | the name it was installed under |
| `source` | string | `scheme:id` — the stable identity of the package |
| `version` | string | for humans; `tag` is what actually gets resolved |
| `tag` | string | the exact release `ketch sync` asks for |
| `target` | string | the machine this entry was captured on, as `<os>-<arch>` |
| `asset` | string | the release file that was taken **on that target** |
| `sha256` | string | of that file, 64 hex characters |
| `pinned` | boolean | whether the package was held at this version |

Every key is written on every entry, `pinned` included. Reading one back,
`pinned` defaults to `false` when absent, so a hand-written lockfile can leave
it out.

## What is reproducible, and what is not

**The tag is.** Every machine resolving the same tag gets the same release.
That is the point of writing one down.

**The asset and its hash are only reproducible on the same target.** A lock
written on Apple Silicon names an `aarch64` tarball that an Intel machine
cannot run. So:

- On a machine whose `target` matches, `sync` holds the download to the
  recorded `sha256` and refuses it before unpacking anything if it disagrees.
  A release replaced under a tag it already published is precisely what a
  lockfile exists to catch.
- On any other machine, `sync` picks the asset that fits the host and verifies
  it against the checksum the source publishes, as a normal install does.
  Pretending the recorded hash still applied would be a guarantee that quietly
  is not one.

## `ketch sync`

```bash
ketch sync                 # install what is missing or at the wrong version
ketch sync --dry-run       # show the plan, change nothing
ketch sync --prune         # also remove packages the lockfile does not name
ketch sync --file <FILE>   # a lockfile somewhere other than ./ketch.lock
ketch sync --jobs 1        # one package at a time instead of the configured width
```

The plan reads as a diff:

```
+ jq 1.7.1
~ ripgrep 14.1.0 -> 14.1.1
- httpie not in the lockfile
2 already match
```

Entries are matched against what is installed by `source`, not by `name`: a
package can be renamed upstream, or installed under an alias, and still be the
same thing to update.

A package the lockfile does not mention is **not** drift on its own — a
lockfile records what you want, not necessarily everything you have — so
`ketch lock --check` ignores extras and only `--prune` removes them. Pruning
can lose work, so it asks first unless you pass `--yes`.

`pinned` is restored after installing, so a package the lock recorded as held
comes back held rather than quietly upgradeable.

A missing lockfile is an error, not an empty one: `ketch sync` with nothing to
sync from is a mistake worth naming, not a no-op to report as success.

## What ketch checks

A lockfile is a file somebody else may have written; that is what sharing a
dotfiles repository means. So nothing in it is allowed to choose a filesystem
path. `sync` asks for a source at a tag and lets the ordinary manifest
resolution decide the install name, the binaries, and where they go — the lock
pins *which release*, never *where it lands*.

| Refused | Why |
| --- | --- |
| a `name` that is not usable verbatim as one path component | it is matched against installed packages and shown to you; a name that would have to be rewritten does not mean what it says |
| an empty `name` | there is nothing to match |
| the same package twice | only one of them could ever be installed |
| a `source` that is not `scheme:id` or `owner/repo` | it becomes a URL, or the id handed to a plugin |
| a `github:` source that is not a valid `owner/repo` | it becomes a URL |
| a `sha256` that is not 64 hex characters | it is compared against a real digest |
| an empty `tag` | there is nothing to resolve |
| an unknown key | a misspelt key that is silently ignored locks something other than what you wrote |
| a `version` newer than this ketch understands | upgrade with `ketch self update` |

One bad entry fails the whole file rather than being skipped. Unlike the
registry — where a partial answer beats none — a lockfile that installed most
of itself would not be a lock at all.

## Which name gets resolved

`sync` asks for the package by `name` first, because that is how it was found
originally, and which manifest tier answers decides what gets linked and under
what names. Resolving straight from `github:BurntSushi/ripgrep` would fall
through to inference and could expose different binaries than the registry
entry you actually installed.

The name is used only while it still means the same project. If it now resolves
to a different source, the `source` in the lockfile wins — a name that changed
hands must not quietly install something else.
