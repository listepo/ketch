# Contributing

Repository prose (commits, PRs, docs, comments) is English. Read
[`AGENTS.md`](AGENTS.md) before changing anything — layout, conventions, and
trust boundaries.

## Checks

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

The repository is a Cargo workspace: the `ketch` binary at the root (the
command line: `src/main.rs`, `src/cli.rs`, `src/cmd/`, `src/complete.rs`,
`src/man.rs`, `src/self_docs.rs`) and the `ketch-core` library in `crates/ketch-core`
(everything else). Unit tests sit beside the module they test, in either
crate; `tests/` drives the real binary. `AGENTS.md` says which crate a change
belongs in.

CI (`ci.yml`) runs on pushes to `main`, on pull requests that are not drafts,
and on `workflow_dispatch`. Before merging a branch, dispatch the gate on that
ref:

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
[pyrlyn.github.io/ketch/docs](https://pyrlyn.github.io/ketch/docs/) is
generated from those files — edit the Markdown here, not the published HTML.

To add a package to the registry, put a `ketch.toml` at the package repo root
and run `ketch registry push` (see [`docs/REGISTRY.md`](docs/REGISTRY.md)).
