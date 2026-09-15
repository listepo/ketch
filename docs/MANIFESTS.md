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

## Writing one

`ketch config create` writes one for you, so the schema below is a reference
to read rather than a form to memorize. It asks what each field should say —
source, name, `bin` entries, asset patterns and the rest — showing the default
so an empty answer keeps it, then previews the assembled file before writing
it.

```bash
ketch config create    # asks, previews, writes ./ketch.toml
```

`--file` writes somewhere else, `--force` replaces a file that is already
there, and `--yes` writes it without the final confirmation. The answers can
be piped to stdin instead — one per line, an empty line taking the default —
so the questionnaire works from a script as well as a terminal. From there,
offering the file to everyone is a registry pull request — see
[REGISTRY.md](REGISTRY.md).

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
"linux-x86_64" = "*-x86_64-unknown-linux-gnu.tar.gz"
"linux-aarch64" = "*-aarch64-unknown-linux-gnu.tar.gz"
"windows-x86_64" = "*-x86_64-pc-windows-msvc.zip"
"windows-aarch64" = "*-aarch64-pc-windows-msvc.zip"
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
| `extra_paths` | list of strings or tables | empty | Man pages and completions to link into user directories. |
| `asset` | table | platform picks | Narrows which release asset is chosen. |
| `trust` | table | none | Whose signature a release must carry. See below. |

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

### `extra_paths`

Each entry is a path relative to the extracted payload, as a string or a table.

```toml
extra_paths = [
  "complete/rg.bash",
  "doc/rg.1",
  { path = "misc/custom", kind = "completion", shell = "bash" },
]
```

A string is classified by these rules, and refused if both or neither apply:

- **Completion** — a path component is `complete`, `completions`, `completion`,
  `bash-completion`, or `site-functions`, and the file name is `*.bash`,
  `*.zsh`, `*.fish`, `*.ps1`, `*.elv`, a zsh `_name` with no other dots, or
  extensionless under `bash-completion` / `site-functions`.
- **Man page** — a path component is `man`, `manN` (N is a man section), `doc`,
  or `docs`, and the file name is `name.N` or `name.N.gz` where N is `1`–`9`
  optionally followed by lowercase letters (`1`, `8`, `1p`).

`complete/rg.1` matches both families and is refused. A file at the payload
root such as `rg.1` is refused. Set `kind` instead of guessing.

A table must set `kind` (`man` or `completion`). `shell` is required for a
completion whose file name does not name a shell. `section` is required for a
man page whose file name is not `*.N`. Paths are still confined to the payload.

ketch links classified files into the user man root and completion directories
`ketch doctor` reports. Destinations are recorded in `state.json` like binaries,
so uninstall and relink use the same ownership proof.

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

Target keys are `<os>-<arch>` for the machine ketch is running on, for example
`macos-aarch64`, `macos-x86_64`, `linux-x86_64`, `linux-aarch64`,
`windows-x86_64`, or `windows-aarch64`. There is no `universal` key — universal
is a property of an asset, not of a host.

Reach for this only when the platform's own scoring picks wrong. Run
`ketch info <pkg> --assets` first — it lists every asset with the score it got
and the reason, which is usually enough to see what the manifest needs to say.

### `trust`

A checksum proves the download is the file the release lists. A signature
proves who published it. `trust` names that publisher, and ketch refuses a
release that does not carry their signature.

```toml
[trust]
verifier = "sigstore"
issuer = "https://token.actions.githubusercontent.com"
repository = "BurntSushi/ripgrep"
```

| Key | Applies to | What it does |
| --- | --- | --- |
| `verifier` | all — **required** | `sigstore`, `minisign` or `gpg`. |
| `mode` | all | `require` (the default) refuses the install when the signature cannot be verified. `warn` installs anyway, says so, and records the package as checksum-only. |
| `signature` | all | The sidecar's file name; `{file}` stands for the signed file's name. Defaults: `{file}.sigstore.json`, `{file}.minisig`, `{file}.asc`. |
| `signed` | all | A glob naming a checksum list (`SHA256SUMS`, `*checksums.txt`) that the signature covers instead of the asset. The list must name the asset with the hash ketch downloaded. |
| `issuer` | `sigstore` — **required** | The OIDC issuer of the signing certificate: `https://token.actions.githubusercontent.com` for GitHub Actions. |
| `identity` | `sigstore` | The certificate's exact identity: a workflow URL such as `https://github.com/o/r/.github/workflows/release.yml@refs/tags/v1.2.3`, or an email. |
| `repository` | `sigstore` | `owner/repo`: any workflow in that GitHub repository. `identity`, `repository` or both is required — an issuer alone admits anyone it issues to. |
| `public_key` | `minisign`, `gpg` — **required** | minisign: the key line, or the whole `minisign.pub`. gpg: the armored public key block. |
| `fingerprint` | `gpg` — **required** | The primary key's full fingerprint. The key block must have it. |

What each verifier accepts:

- **sigstore** — a Sigstore bundle, as `cosign sign-blob --bundle` and GitHub
  artifact attestations write. It is checked offline against Sigstore's
  public-good trusted root, which ketch carries: the certificate chains to
  Sigstore's CA and names the pinned identity, the signature covers the file,
  and the Rekor transparency log signed the entry. A bare `.sig` with a
  certificate, or an `.intoto.jsonl` without its log entry, cannot be checked
  offline and is refused.
- **minisign** — the prehashed signatures `minisign -S` makes. Legacy ones are
  refused.
- **gpg** — a detached signature, armored or binary, by the key or one of its
  signing subkeys. No keyring is read: a key imported for something else is
  not this publisher's authorisation. Revoked keys, expired signatures and
  signatures made after the key expired are refused.

The sidecar and any checksum list are release assets like the one installed:
a release that does not publish them fails the install. What was verified is
recorded in `state.json` — the verifier, the pinned identity, the sidecar's
name and SHA-256, and for sigstore the Rekor log index — and `ketch info`
shows it. Nothing the signature file says about itself is printed. A package
without `trust` shows as `checksum` when the release published a matching
checksum, and `first use` when it published none.

## What ketch checks

Serde checks the shape. These are the values it cannot judge:

| Rule | Why |
| --- | --- |
| `name` must be usable verbatim as one path component | It becomes a directory in the store. A name that needed sanitising would install somewhere other than where it says. |
| every `bin.name` must be usable verbatim as one path component | It becomes a symlink in `~/.ketch/bin`. |
| every `bin.path` must stay inside the payload | No absolute paths, no `..`. |
| every `extra_paths` entry must stay inside the payload | Same. |
| every `extra_paths` entry must be a man page or a completion | Classified from `kind` or the path rules below. Ambiguous entries are refused. |
| each `bin` entry needs `name` or `path` | An entry with neither says nothing. |
| `provides` aliases must be non-empty and whitespace-free | An alias nobody can type is not an alias. |
| `strip_prefix` must be at most 8 | Each level is a directory listing, and no real archive nests wrappers that deep. |
| `trust.signature` and `trust.signed` must be usable file names, and `signature` knows only `{file}` and must name another file | They name files downloaded beside the asset. |
| `trust` keys must belong to the chosen `verifier` | A `fingerprint` on a minisign policy would read as a check and be none. |
| sigstore needs an `https` `issuer` and an `identity` or `repository` | An issuer alone admits anyone it issues certificates to. |
| a minisign `public_key` must parse | A key that cannot be read cannot be pinned. |
| a gpg `public_key` must parse, verify its own self-signatures, not be revoked, and have the pinned `fingerprint` | The fingerprint is the pin; the key block alone could be anyone's. |

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
