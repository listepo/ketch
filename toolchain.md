# Toolchain

Программы проекта и прямые пакеты из манифестов.

## Программы

| Программа | Как ставить | Зачем здесь | Источник |
| --- | --- | --- | --- |
| mise | brew / curl, затем `mise install` | Пины версий инструментов | https://github.com/jdx/mise |
| cargo-cache | mise | `just cache` / `just cache-autoclean`; the shared cargo home fills up | https://github.com/matthiaskrgr/cargo-cache |
| node | mise | commitlint for `just lint-commits` and the commit-msg hook; node-based checks live in Just and CI only | https://github.com/nodejs/node |
| rustc | rustup / системный | Компилятор Rust | https://github.com/rust-lang/rust |
| cargo | вместе с rustc | Сборка и зависимости Rust | https://github.com/rust-lang/cargo |
| just | cargo install just / brew | Рецепты команд | https://github.com/casey/just |

## cargo

| Пакет | Где | Источник | Зачем здесь |
| --- | --- | --- | --- |
| assert_cmd | локально | https://crates.io/crates/assert_cmd | Зависимость Rust |
| assert_fs | локально | https://crates.io/crates/assert_fs | Зависимость Rust |
| bzip2 | локально | https://crates.io/crates/bzip2 | Зависимость Rust |
| clap | локально | https://crates.io/crates/clap | CLI |
| clap_complete | локально | https://crates.io/crates/clap_complete | Зависимость Rust |
| crossterm | локально | https://crates.io/crates/crossterm | Терминал |
| diesel | локально | https://crates.io/crates/diesel | SQLite ORM |
| diesel_migrations | локально | https://crates.io/crates/diesel_migrations | Миграции SQLite |
| dirs | локально | https://crates.io/crates/dirs | Зависимость Rust |
| flate2 | локально | https://crates.io/crates/flate2 | Зависимость Rust |
| hex | локально | https://crates.io/crates/hex | Зависимость Rust |
| indicatif | локально | https://crates.io/crates/indicatif | Зависимость Rust |
| libsqlite3-sys | локально | https://crates.io/crates/libsqlite3-sys | `bundled` compiles SQLite into the binary. Linking the system one would make ketch's single-binary promise depend on what the host happens to ship, and the release builds both macOS architectures where that answer differs. |
| lzma-rs | локально | https://crates.io/crates/lzma-rs | Зависимость Rust |
| octocrab | локально | https://crates.io/crates/octocrab | `ketch registry push` talks to GitHub through octocrab rather than the ureq client the sources use: it needs forks, refs, contents and pull requests, which octocrab already types and paginates. It is async, hence tokio for a runtime to block on; everything else in ketch stays synchronous. |
| predicates | локально | https://crates.io/crates/predicates | Зависимость Rust |
| pretty_assertions | локально | https://crates.io/crates/pretty_assertions | Зависимость Rust |
| ratatui | локально | https://crates.io/crates/ratatui | TUI |
| semver | локально | https://crates.io/crates/semver | Зависимость Rust |
| serde | локально | https://crates.io/crates/serde | Сериализация |
| serde_json | локально | https://crates.io/crates/serde_json | JSON |
| sha2 | локально | https://crates.io/crates/sha2 | Зависимость Rust |
| tar | локально | https://crates.io/crates/tar | Зависимость Rust |
| tempfile | локально | https://crates.io/crates/tempfile | Зависимость Rust |
| thiserror | локально | https://crates.io/crates/thiserror | Ошибки |
| tokio | локально | https://crates.io/crates/tokio | Асинхронность |
| toml | локально | https://crates.io/crates/toml | Конфиг |
| trycmd | локально | https://crates.io/crates/trycmd | Зависимость Rust |
| ureq | локально | https://crates.io/crates/ureq | Зависимость Rust |
| walkdir | локально | https://crates.io/crates/walkdir | Зависимость Rust |
| zip | локально | https://crates.io/crates/zip | Зависимость Rust |

## npm / pnpm

| Пакет | Где | Источник | Зачем здесь |
| --- | --- | --- | --- |
| @commitlint/cli | локально | https://www.npmjs.com/package/@commitlint/cli | Сообщения коммитов |
| @commitlint/config-conventional | локально | https://www.npmjs.com/package/@commitlint/config-conventional | Правила коммитов |
