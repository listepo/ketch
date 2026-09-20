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

## `brew upgrade` does not upgrade ketch itself

Homebrew keeps only the bootstrap binary; the ketch it installed is one ketch
downloaded and verified itself. Hand over explicitly after the cask updates:

```bash
brew upgrade --cask ketch
ketch self upgrade
```

`ketch self update` is kept as an alias, so existing scripts keep working.

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

The `Notarise` step in `release.yml` only runs when the repository variable
`KETCH_NOTARIZE` is `true`, and then fails rather than ships unsigned: the
three secrets must exist — `APPSTORE_CONNECT_KEY` (the `.p8`, base64),
`APPSTORE_CONNECT_KEY_ID`, `APPSTORE_CONNECT_ISSUER_ID`. The smoke test needs
`spctl` to report `source=Notarized Developer ID`; a bare Mach-O binary cannot
be stapled, so Gatekeeper looks its ticket up online. See `AGENTS.md`
Releasing and `tests/release-yml-notarize.sh`.