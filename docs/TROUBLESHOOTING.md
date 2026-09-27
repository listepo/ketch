# Troubleshooting

Failures ketch already explains, collected in one place so the fix does not
have to be searched out of the command that reported it.

## A Windows executable is locked mid-upgrade

Another process is running from a file ketch is about to replace. ketch lists
the processes, asks whether to stop them, and on yes sends TERM then KILL
(`taskkill /F` on Windows); the current ketch pid is never offered. Declining
leaves them running and replacement continues as before — on Windows the busy
binary is renamed aside first, so the new one can still land.

```text
$ ketch upgrade
testtool 1.0.0 -> 2.0.0
stop 1 process using testtool? [y/N]
```

## An install stops at "ships several binaries sharing its name"

The release holds more than one executable that answers to the package's name
(`rtok-cli` and `rtok-hook` for a package called `rtok`), none of them is named
exactly like the package, the manifest has no `bin` entry, and there was no
terminal to ask in — piped, in CI, or run with `--yes`. Nothing is installed.

```text
$ ketch install owner/rtok --yes
error: `rtok` ships several binaries sharing its name (rtok-cli, rtok-hook) and none is named `rtok`; ...
```

Either run the same command in a terminal without `--yes` and pick one from the
list — the choice is remembered in `state.json` and reused on every upgrade —
or name the binary in your own manifest, which wins over the registry:

```toml
# ~/.ketch/manifests/rtok.toml
name = "rtok"
source = "github:owner/rtok"
bin = [{ name = "rtok-cli" }]
```

The same choice is made on macOS, Linux and Windows; see the `bin` section of
[MANIFESTS.md](MANIFESTS.md) for the order.

## `ketch.exe.old` is still in the bin dir

Windows will not delete the file that backs a running image. `self upgrade`
renames the running `ketch.exe` to `ketch.exe.old`, and the delete of that
aside fails until this process exits. The next `ketch self upgrade` or
`ketch self install` removes it. `ketch doctor` names the file when it is
still there; delete it by hand once nothing is running from it.

A swap that fails with `Access is denied (os error 5)` on `ketch.exe` itself
is a short lock, usually antivirus scanning the new file. The rename and the
copy are retried for under a second. A lock that outlasts those retries
restores the previous binary.

If the new binary or the process listing never finishes, ketch stops that
process and says so, including its pid when the process had been created.
A spawn that never returns has no pid to stop; the message says the wait
was ended. When that happens to the version check after a swap, the previous
binary is kept.

## `brew upgrade` does not upgrade ketch itself

Homebrew keeps only the bootstrap binary; the ketch it installed is one ketch
downloaded and verified itself. Hand over explicitly after the cask updates:

```bash
brew upgrade --cask ketch
ketch self upgrade
```

`ketch self update` is kept as an alias, so existing scripts keep working.

## `ketch self upgrade` says ketch is managed by mise

A ketch installed with `mise use -g github:listepo/ketch` lives in mise's
install tree, under a directory named for its version. Rewriting that binary
would leave mise reporting a version that is no longer on disk, so ketch
declines. Either let mise upgrade it:

```bash
mise upgrade
```

or hand ketch over to itself once, after which `ketch self upgrade` works and
the mise copy is only a bootstrap:

```bash
ketch self install
```

## A registry entry collides or will not validate

Name and alias collisions are fatal in `ketch registry validate` (so they
cannot land) and warnings in `ketch update` (so one bad folder never takes a
working copy down with it). From the registry checkout root:

```bash
ketch registry validate .
ketch registry validate . --fixture ./ci/fixtures --changed ripgrep --changed fd
```

The `ketch-registry` repository carries no CI by the owner's choice (plan.md
F2), so validate locally and keep the pre-push hook in
[REGISTRY.md](REGISTRY.md) installed.

## Notarisation fails on a release run

The `Notarise` step in `release.yml` (from `.github/build-check.yml`) only runs when the repository variable
`KETCH_NOTARIZE` is `true`, and then fails rather than ships unsigned: the
three secrets must exist — `APPSTORE_CONNECT_KEY` (the `.p8`, base64),
`APPSTORE_CONNECT_KEY_ID`, `APPSTORE_CONNECT_ISSUER_ID`. The smoke test needs
`spctl` to report `source=Notarized Developer ID`; a bare Mach-O binary cannot
be stapled, so Gatekeeper looks its ticket up online. See `AGENTS.md`
Releasing and `tests/release-workflows.sh`.