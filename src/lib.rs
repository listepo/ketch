//! The fuzzing face of ketch: its modules as a library, for `fuzz/` only.
//!
//! ketch is a binary, and a cargo-fuzz target can only link a library. This
//! file exists only under `cfg(fuzzing)`, which `cargo fuzz` sets and nothing
//! else does, so every stable build — `cargo build`, `just check`, the release
//! — sees an empty library and `main.rs` stays the one crate root that ships.
//! Under fuzzing it declares the same modules `main.rs` does, compiling them a
//! second time as this library, and `fuzzing` hands the targets the few
//! entry points they drive.
//!
//! A module added to `main.rs` belongs here too; the nightly fuzz build fails
//! on a `crate::` path this list does not have, and nothing else notices.
#![cfg(fuzzing)]
// The modules are compiled for a handful of entry points, so most of what they
// define is unused from here; that says nothing about the binary.
#![allow(dead_code, unused_imports)]

mod bin_choice;
mod changelog;
mod cli;
mod cmd;
mod config;
mod diff;
mod error;
mod extra;
mod extract;
mod hooks;
mod http;
mod install;
mod listing;
mod lockfile;
mod log;
mod manifest;
mod model;
mod platform;
mod process;
mod push;
mod registry;
mod resolve;
mod self_update;
mod shell;
mod source;
mod state;
mod stats;
mod trust;
#[cfg(feature = "tui")]
mod tui;
mod ui;
mod wizard;

/// Entry points for the targets in `fuzz/fuzz_targets/`, one per trust
/// boundary. Each takes plain data, returns nothing, and panics only when an
/// invariant the code promises does not hold: an `Err` is a correct answer to
/// hostile input, a panic or a path outside its root is a finding.
pub mod fuzzing {
    use crate::cli::Cli;
    use crate::extract::archive::{
        TarBz2Extractor, TarExtractor, TarGzExtractor, TarXzExtractor, ZipExtractor,
    };
    use crate::extract::Extractor;
    use crate::model::{CompletionShell, ExtraKind, ExtraPath, ExtraPathSpec, PackageSpec};
    use clap::{Command, CommandFactory, Parser};
    use std::ffi::OsString;
    use std::path::{Component, Path, PathBuf};
    use std::sync::OnceLock;

    /// Every word of the clap tree — subcommands, aliases, flags, possible
    /// values — so `cli_argv` can build argv that gets past the first word.
    pub fn cli_vocab() -> Vec<String> {
        let cmd = Cli::command();
        // Clap's own consistency checks: conflicts, groups, duplicate flags.
        cmd.clone().debug_assert();
        let mut words: Vec<String> = ["--", "-", "--help", "-h", "--version", "-V", "help"]
            .map(String::from)
            .to_vec();
        walk(&cmd, &mut words);
        words.sort();
        words.dedup();
        words
    }

    fn walk(cmd: &Command, words: &mut Vec<String>) {
        for arg in cmd.get_arguments() {
            if let Some(long) = arg.get_long() {
                words.push(format!("--{long}"));
            }
            if let Some(short) = arg.get_short() {
                words.push(format!("-{short}"));
            }
            for value in arg.get_possible_values() {
                words.push(value.get_name().to_string());
            }
        }
        for sub in cmd.get_subcommands() {
            words.push(sub.get_name().to_string());
            words.extend(sub.get_all_aliases().map(String::from));
            walk(sub, words);
        }
    }

    /// Parse argv the way `main` does, and render the error a failed parse
    /// would print.
    pub fn cli_argv(argv: Vec<OsString>) {
        let argv = std::iter::once(OsString::from("ketch")).chain(argv);
        if let Err(e) = Cli::try_parse_from(argv) {
            let _ = e.render().to_string();
        }
    }

    /// `name[@version]`, `owner/repo`, `scheme:id` — whatever a user types.
    pub fn package_spec(input: &str) {
        let spec = PackageSpec::parse(input);
        let _ = spec.label();
    }

    /// A registry file: one manifest or a `[[package]]` array, each through
    /// `Manifest::validate`.
    pub fn manifest_toml(text: &str) {
        let _ = crate::manifest::fuzz_parse_registry(text);
    }

    /// `ketch.lock`: TOML, then every check `sync` relies on.
    pub fn lockfile(text: &str) {
        let _ = crate::lockfile::fuzz_parse(text);
    }

    /// `state.json` through the loader ketch runs on every start, which reads a
    /// path; one scratch file is rewritten per input.
    pub fn state(bytes: &[u8]) {
        static FILE: OnceLock<tempfile::TempDir> = OnceLock::new();
        let Some(dir) = scratch(&FILE) else { return };
        let path = dir.path().join("state.json");
        if std::fs::write(&path, bytes).is_ok() {
            let _ = crate::state::State::load_path(&path);
        }
    }

    fn scratch(cell: &'static OnceLock<tempfile::TempDir>) -> Option<&'static tempfile::TempDir> {
        if cell.get().is_none() {
            let _ = cell.set(tempfile::tempdir().ok()?);
        }
        cell.get()
    }

    /// A release's `SHA256SUMS` body and GitHub's `sha256:<hex>` asset digest.
    pub fn checksum_file(text: &str) {
        for (name, hex) in crate::source::github::parse_checksum_file(text) {
            assert!(
                !name.contains('/'),
                "checksum key keeps a directory: {name:?}"
            );
            assert!(
                hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
                "checksum value is not lowercase sha256 hex: {hex:?}"
            );
        }
        if let Some(hex) = crate::source::github::fuzz_parse_digest(text) {
            assert_eq!(hex.len(), 64, "digest is not sha256: {hex:?}");
        }
    }

    /// Unpack arbitrary bytes with the archive extractors every platform
    /// shares, then prove nothing landed outside the destination and no
    /// symlink the archive made points out of it.
    pub fn archive_extract(bytes: &[u8]) {
        let Ok(work) = tempfile::tempdir() else {
            return;
        };
        let Ok(work_path) = dunce::canonicalize(work.path()) else {
            return;
        };
        let src = work_path.join("payload");
        let dest = work_path.join("dest");
        if std::fs::write(&src, bytes).is_err() {
            return;
        }
        let extractors: Vec<Box<dyn Extractor>> = vec![
            Box::new(TarGzExtractor),
            Box::new(TarXzExtractor),
            Box::new(TarBz2Extractor),
            Box::new(TarExtractor),
            Box::new(ZipExtractor),
        ];
        let _ = crate::extract::extract_auto(&src, &dest, &extractors);

        if let Ok(entries) = std::fs::read_dir(&work_path) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                assert!(
                    name == "payload" || name == "dest",
                    "extraction wrote {:?} outside its destination",
                    entry.path()
                );
            }
        }
        for entry in walkdir::WalkDir::new(&dest).into_iter().flatten() {
            if !entry.path_is_symlink() {
                continue;
            }
            let Ok(target) = std::fs::read_link(entry.path()) else {
                continue;
            };
            let parent = entry.path().parent().unwrap_or(&dest);
            let resolved = lexical_join(parent, &target);
            assert!(
                resolved.starts_with(&dest),
                "symlink {:?} -> {:?} leaves the destination",
                entry.path(),
                target
            );
        }
    }

    /// `base.join(rel)` with `..` and `.` folded away without touching disk.
    fn lexical_join(base: &Path, rel: &Path) -> PathBuf {
        let mut out = if rel.is_absolute() {
            PathBuf::new()
        } else {
            base.to_path_buf()
        };
        for component in rel.components() {
            match component {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    }

    /// One `extra_paths` entry: a bare path, or a table naming kind, shell and
    /// section. A classified entry must name a relative path that stays inside
    /// the payload.
    pub fn extra_path(path: String, spec: Option<(bool, Option<u8>, Option<String>)>) {
        let entry = match spec {
            None => ExtraPath::Path(path),
            Some((man, shell, section)) => ExtraPath::Spec(ExtraPathSpec {
                path,
                kind: if man {
                    ExtraKind::Man
                } else {
                    ExtraKind::Completion
                },
                shell: shell
                    .map(|i| CompletionShell::ALL[usize::from(i) % CompletionShell::ALL.len()]),
                section,
            }),
        };
        let Ok(classified) = crate::extra::classify(&entry) else {
            return;
        };
        let rel = Path::new(&classified.rel_path);
        assert!(
            crate::extract::safe_member_path(rel).is_ok(),
            "classified extra path {:?} is not a safe payload member",
            classified.rel_path
        );
        assert!(
            !rel.components().any(|c| matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )),
            "classified extra path {:?} escapes the payload",
            classified.rel_path
        );
    }

    /// Every JSON body a `ketch-source-*` plugin answers with.
    pub fn plugin_protocol(body: &str) {
        crate::source::plugin::fuzz_parse(body);
    }

    /// A hook's script on its way to the shell. On Unix it must reach `sh -c`
    /// as exactly one argument, whatever it contains. A NUL cannot be in any
    /// argument: `std` records the argument as unusable and the spawn fails
    /// with an error, so that script is only checked for staying one argument.
    pub fn hook_line(script: &str) {
        let cmd = crate::hooks::fuzz_shell(script);
        #[cfg(not(windows))]
        {
            let args: Vec<_> = cmd.get_args().collect();
            assert_eq!(args.len(), 2, "hook script split into {args:?}");
            if !script.contains('\0') {
                assert_eq!(args[1], script, "hook script changed on its way to sh");
            }
        }
        #[cfg(windows)]
        let _ = cmd;
    }

    /// Somebody else's text on its way to the terminal: nothing that can move
    /// the cursor, drive the terminal or reorder the line survives.
    pub fn printable(text: &str) {
        let out = crate::ui::printable(text);
        for c in out.chars() {
            assert!(
                c == '\n' || c == '\t' || !c.is_control(),
                "control character {c:?} survived printable"
            );
            assert!(
                !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200b}'..='\u{200f}'),
                "invisible or bidi character {c:?} survived printable"
            );
        }
        // Filtering is idempotent: a second pass has nothing left to take.
        assert_eq!(crate::ui::printable(&out), out);
    }
}
