# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/listepo/ketch/releases/tag/v0.1.0) - 2026-09-07

### Added

- install ketch as a package, push manifests to the registry, publish a Homebrew cask
- add optional ratatui progress UI

### Fixed

- build the octocrab client inside its runtime

### Other

- sign the release binaries with a Developer ID certificate
- 📝 Add docstrings to `stats-sqlite` ([#10](https://github.com/listepo/ketch/pull/10))
- Version history in SQLite, cargo-cache via mise, release-plz ([#9](https://github.com/listepo/ketch/pull/9))
- expand delivery roadmap
- add assertion tooling
- remove task runner links
- plan ratatui tui support
- compare Rust task runners
- add cargo cache aliases
- add Rust CLI testing plan
- Revert "Rewrite ketch in TypeScript, still shipping one native binary ([#7](https://github.com/listepo/ketch/pull/7))"
- Close the ledger: the port is merged ([#8](https://github.com/listepo/ketch/pull/8))
- Add fallow, configured strict, gating only what a change introduces
- Tick Phase 11: the review is adjudicated and the runtimes check out
- Stream a tar to disk instead of holding it all in memory
- Test the checksum requirement, which nothing was exercising
- Let the platform be heard when it declines to remove a link
- Say when the log could not be opened
- Stop deleting downloads that arrived intact
- Stop leaking a listener per zip member, and test the zip guards
- Keep the tag an upgrade approved, whatever shape it is
- Refuse a tar that was cut off in transit
- Stop refusing ordinary tarballs, and stop offering to delete node
- Say how to re-test the Perry blocker
- Run the end-to-end suite against the packaged binary
- Tick Phase 10 off the ledger
- Keep compiler intermediates out of the repository
- Remove the Rust implementation
- Tick Phase 9 off the ledger
- Port CI, packaging and the release script off the Rust toolchain
- Decompress xz with the OS instead of WebAssembly
- Give Moon the config layout 2.5 actually reads
- Tick the documentation off the ledger
- Give the site back its own colours
- Fix what writing the documentation found
- Rewrite the documentation for the TypeScript tree
- Tick the site off the ledger
- Keep the gates honest about the site
- Rebuild the website on Astro and Docusaurus
- Correct the ledger to what is actually committed
- Port the command layer and the end-to-end suite
- Keep the sources runnable on Node, not just on Bun
- Write the migration plan any agent can resume from
- Port the terminal layer and the e2e harness
- Keep the old registry until the new one is in place
- Port every core module to TypeScript
- Port config, state and the log vocabulary to TypeScript
- Port the data schemas and the core type contracts
- Record the rewrite toolchain and agent policy in AGENTS.md
- Scaffold the TypeScript monorepo
- Stage every download in a directory of its own
- Install in parallel by default, and keep a log
- Read a package's changelog, from the file or the release
- Add `ketch.lock`, and `ketch sync` to reproduce a machine from one
- Add `ketch path` to put the bin dir on PATH, and let doctor fix it
- Add `scripts/release.sh` to open a release pull request
- Harden archive extraction and install/update safety
- stop the headline forcing a page wider than a phone
- shorten the descriptions that were rendering truncated
- emit JSON-LD as an object, and a description that fits
- assertions that survive minification, and drop a deprecated config key
- landing page, published docs, README
- CI packaging, release workflow, and end-to-end tests
- Package registry, validation, bug fixes and docs
- types, traits, config, http, cli, state
