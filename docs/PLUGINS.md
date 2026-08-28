# Source plugins

GitHub is built in. Anything else — GitLab, Gitea, an internal artifact server —
can be added as a plugin, without recompiling ketch and without a ketch release.

A plugin is an executable named `ketch-source-<scheme>`. ketch runs it with a
subcommand and reads one JSON document from its stdout. That is the whole
contract, so a plugin can be written in any language.

## Discovery

ketch looks in `~/.ketch/plugins` first, then every directory on `PATH`. The
first `ketch-source-<name>` found wins, so a copy in the plugins directory
shadows one on `PATH`; shadowing is decided by the executable's file name, not
its path. Within a directory, candidates are taken in sorted order, so what
`ketch plugin list` shows is stable from run to run. Anything that is not
executable is passed over.

`ketch plugin list` shows what was found and `ketch plugin dir` prints the
directory.

Every discovered plugin is asked for its capabilities on every ketch command, so
`capabilities` must be fast and must not touch the network.

## Subcommands

```text
capabilities                     -> {"protocol":1,"scheme":"gitlab","download":false,"search":true}
describe <id>                    -> a SourceInfo object, or null
releases <id> --limit N [--prerelease]
                                 -> [ Release, ... ]
search <query> --limit N         -> [ SourceInfo, ... ]
download <url> <dest>            -> only when capabilities.download is true
```

`--limit` is always passed to `releases` and `search`; `--prerelease` is added
to `releases` only when prereleases are wanted.

`KETCH_PROTOCOL_VERSION` is set in the environment for each call.

Exit `0` with the JSON document on stdout. Anything else is a failure, and
whatever the plugin wrote to stderr is shown to the user. The document must be
valid UTF-8; output that is not is refused by name rather than mangled.

### `capabilities`

```json
{ "protocol": 1, "scheme": "gitlab", "download": false, "search": true }
```

| Field | Required | Meaning |
| --- | --- | --- |
| `protocol` | yes | Must equal ketch's `PROTOCOL_VERSION`, currently `1`. A mismatch is reported and the plugin is ignored. |
| `scheme` | yes | The prefix users type: `gitlab:group/project`. ASCII letters, digits, `-` and `_` only. |
| `download` | no | `true` if the plugin fetches assets itself. Defaults to `false`. |
| `search` | no | `true` if `search` is implemented. Defaults to `false`. |

A plugin whose `capabilities` does not parse, reports another protocol, or names
an unusable scheme is skipped with a warning; every other source keeps working.

### `releases`

Newest first. Drafts must be excluded and prereleases included only when
`--prerelease` is passed — ketch re-applies both filters anyway, so a plugin
that ignores its flags cannot change what gets installed.

```json
[
  {
    "version": "1.2.3",
    "tag": "v1.2.3",
    "prerelease": false,
    "draft": false,
    "published_at": "2026-01-15T10:00:00Z",
    "notes": "release notes",
    "assets": [
      {
        "name": "tool-1.2.3-aarch64-apple-darwin.tar.gz",
        "url": "https://example.invalid/download/tool.tar.gz",
        "size": 2400000,
        "content_type": "application/gzip",
        "digest": { "algo": "sha256", "hex": "e3b0c442..." },
        "headers": { "PRIVATE-TOKEN": "..." }
      }
    ]
  }
]
```

Only `version`, `tag` and each asset's `name` and `url` are required; every
other field defaults — `prerelease` and `draft` to `false`, `size` to `0`,
`assets` and `headers` to empty, the rest to null.

Asset names matter more than anything else here: ketch picks which asset to
install by scoring the **name** against the host platform. Report the names the
project actually publishes.

`digest` is what lets ketch verify the download without extra requests. Supply
it whenever the source knows it. `algo` is the lowercase algorithm name,
currently always `sha256`.

### `describe` and `search`

Both return `SourceInfo`:

```json
{
  "id": "group/project",
  "name": "project",
  "description": "one line",
  "homepage": "https://example.invalid",
  "stars": 1200,
  "license": "MIT",
  "archived": false
}
```

Only `id` and `name` are required. `describe` returns one object or `null`.
`search` returns an array, empty if the plugin does not search — and ketch does
not call it at all unless `capabilities.search` was `true`.

### `download`

Only called when `capabilities.download` is `true`. Write the asset to `<dest>`
and exit `0`. A plugin that exits `0` without writing the file is reported as
having done so; nothing is unpacked.

When `download` is `false`, ketch fetches `asset.url` itself, sending
`asset.headers` and **no ketch credentials** — a plugin's URLs never receive the
user's GitHub token. Put whatever credentials your assets need in `headers`.

Either way ketch hashes the file that actually lands on disk. A plugin does not
get to assert the checksum of its own download.

## What ketch enforces

A plugin is a third-party executable run on the user's behalf, so:

- **stdin is closed.** A plugin that tries to prompt gets EOF rather than the
  user's terminal.
- **output is capped** at 8 MiB per stream.
- **there is a 30-second deadline** per subcommand, after which the process is
  killed with `SIGKILL`.
- **a broken plugin is a warning, not a failure.** Discovery reports it and
  every other source keeps working.

None of this is optional politeness: discovery probes every plugin before
anything else runs, so one that hangs or floods a pipe would take down every
ketch command rather than just its own.

## A minimal example

```sh
#!/bin/sh
# ketch-source-example — put in ~/.ketch/plugins and chmod +x
set -eu
case "$1" in
  capabilities)
    printf '{"protocol":1,"scheme":"example","search":false}\n'
    ;;
  describe)
    printf '{"id":"%s","name":"%s"}\n' "$2" "${2##*/}"
    ;;
  releases)
    printf '[{"version":"1.0.0","tag":"v1.0.0","assets":[{"name":"tool-1.0.0-aarch64-apple-darwin.tar.gz","url":"https://example.invalid/tool.tar.gz"}]}]\n'
    ;;
  *)
    echo "unsupported subcommand: $1" >&2
    exit 1
    ;;
esac
```

Then:

```bash
ketch plugin list
ketch install example:some/project
```

The end-to-end suite in `apps/cli/tests/` drives a plugin of exactly this shape
— `capabilities`, `describe`, `releases` and `download` in a few lines of
`/bin/sh` — which is why the whole suite runs offline.

## Changing the protocol

The message shapes live in `packages/schemas/src/plugin.ts` and are what both
ketch and its tests validate against. Changing any of them means bumping
`PROTOCOL_VERSION` there: a plugin reporting a version ketch does not speak is
ignored with a warning, which is the whole point of the field.
