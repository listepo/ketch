# ketch fuzz targets

[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz) (libFuzzer) targets for
the places ketch reads bytes somebody else wrote. This directory is its own
Cargo workspace, so `cargo build`, `cargo nextest run` and `just check` never
compile it, and it needs a nightly toolchain that the rest of the project does
not use.

## Setup

```bash
mise install                        # cargo-fuzz, pinned in mise.toml
rustup toolchain install nightly    # used only as `cargo +nightly fuzz`
```

`mise.toml` keeps pinning the stable Rust every build uses; nightly is a
rustup toolchain selected per command with `+nightly`. libFuzzer runs on macOS
and Linux; Windows is not a target.

## Running

```bash
just fuzz                     # list the targets
just fuzz archive-extract     # seed the corpora, run one target for 60 s
just fuzz all 300             # every target in turn, 300 s each
cargo +nightly fuzz build     # build every target
```

`fuzz/seed.sh` fills `fuzz/corpus/<target>/` from `tests/fixtures`, the root
`ketch.toml`, `src/builtin.toml`, the code blocks in `docs/`, the example
plugin, and small archives it makes itself. `corpus/`, `artifacts/` and
`target/` are ignored.

## Targets

| Target | Drives | Fails when |
| --- | --- | --- |
| `cli-argv` | `Cli::try_parse_from` over argv drawn from the clap tree, and error rendering | clap's debug asserts or a panic |
| `package-spec` | `PackageSpec::parse` and its label | a panic |
| `manifest-toml` | `manifest::parse_registry`, which runs `Manifest::validate` | a panic |
| `lockfile` | `ketch.lock` parse and validation | a panic |
| `state` | `State::load_path` on arbitrary `state.json` bytes | a panic |
| `checksum-file` | `parse_checksum_file`, `parse_digest` | a key with a directory, or a value that is not lowercase SHA-256 |
| `archive-extract` | tar.gz, tar.xz, tar.bz2, tar and zip through `extract_auto` | anything written outside the destination, or a symlink leaving it |
| `extra-paths` | `extra::classify` on a path or a `{ kind, shell, section }` table | a classified path that is not a safe payload member |
| `plugin-protocol` | every JSON reply a `ketch-source-*` plugin sends | a panic |
| `hook-line` | a hook script on its way to `sh -c` | the script split or changed |
| `printable` | `ui::printable` | a control, bidi or zero-width character surviving, or a second pass changing the text |

## How the targets reach ketch

ketch is a binary. `src/lib.rs` is empty unless the build sets
`cfg(fuzzing)`, which `cargo fuzz` does; then it declares the same modules as
`src/main.rs` and exposes `ketch::fuzzing`, the entry points above. The few
private functions a target needs have a `#[cfg(fuzzing)]` wrapper beside
them. A module added to `main.rs` has to be added to `lib.rs` too.

## A crash

libFuzzer writes the input to `fuzz/artifacts/<target>/`. Minimise it with
`cargo +nightly fuzz tmin <target> <file>`, turn it into a regression test in
`tests/` or beside the module, and fix it in its own pull request.
