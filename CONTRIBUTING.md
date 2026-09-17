# Contributing

Repository prose (commits, PRs, docs, comments) is English. Read
[`AGENTS.md`](AGENTS.md) before changing anything — layout, conventions, and
trust boundaries.

## Checks

```bash
cargo test
cargo clippy --all-targets
cargo fmt --check
```

CI (`ci.yml`) runs on pushes to `main` and on `workflow_dispatch` — not on pull
request events. Before merging a branch, dispatch the gate on that ref:

```bash
gh workflow run ci.yml --ref <branch>
```

Commits follow [conventional commits](https://www.conventionalcommits.org). A
change that alters or removes existing CLI behavior is `feat!:` / `fix!:` or
carries a `BREAKING CHANGE:` footer so release-plz bumps the minor.

## Docs

User-facing guides: [`docs/MANIFESTS.md`](docs/MANIFESTS.md),
[`docs/REGISTRY.md`](docs/REGISTRY.md), [`docs/PLUGINS.md`](docs/PLUGINS.md),
[`docs/LOCKFILE.md`](docs/LOCKFILE.md). The site at
[listepo.github.io/ketch/docs](https://listepo.github.io/ketch/docs/) is
generated from those files — edit the Markdown here, not the published HTML.

To add a package to the registry, put a `ketch.toml` at the package repo root
and run `ketch registry push` (see [`docs/REGISTRY.md`](docs/REGISTRY.md)).
