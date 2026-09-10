//! `ketch config`: writing a package file.
//!
//! `create` is the questionnaire: it asks what each field of a `ketch.toml`
//! should say, assembles the manifest behind [`crate::wizard`], and writes the
//! file this project offers to the registry. This body is deliberately only
//! prompting, previewing and writing — every rule about what an answer means
//! lives in `wizard.rs`, where it is testable without a terminal.

use crate::cli::ConfigCommand;
use crate::config::{sanitize_component, Config};
use crate::error::{Error, Result};
use crate::model::{normalize_name, BinSpec, PackageKind, PackageRef};
use crate::registry::PACKAGE_FILE;
use crate::ui;
use crate::wizard::{self, Answers};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Run the selected `ketch config` operation with the active configuration.
pub fn run(cfg: &Config, command: ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Create { file, force, yes } => {
            // The questionnaire writes into the working tree, never the ketch
            // root, so the config every command receives plays no part here.
            let _ = cfg;
            create(file, force, yes)
        }
    }
}

/// Ask every question, then write what the answers add up to.
fn create(file: Option<PathBuf>, force: bool, yes: bool) -> Result<()> {
    let path = file.unwrap_or_else(|| PathBuf::from(PACKAGE_FILE));
    // Refuse before the first question: a questionnaire that asks everything
    // and then declines to write is a waste of the answers.
    if path.exists() && !force {
        return Err(Error::msg(format!(
            "{} already exists; pass --force to replace it",
            path.display()
        )));
    }
    let answers = ask_everything()?;
    let manifest = wizard::manifest(&answers)?;
    let body = wizard::render(&manifest);

    ui::note(&format!("{} will say:", path.display()));
    // The body ends with its own newline and `out` adds one; strip it or the
    // preview grows a blank line the written file does not have.
    ui::out(body.trim_end_matches('\n'));
    if !yes && !ui::confirm(&format!("write {}?", path.display()), false) {
        return Ok(());
    }
    write_manifest(&path, &body, force)?;
    ui::success("wrote", &path.display().to_string());
    ui::note(&format!(
        "try it with `ketch install {} --verbose`, offer it with `ketch registry push`",
        manifest.name
    ));
    Ok(())
}

/// Write a generated manifest, refusing a late overwrite unless forced.
fn write_manifest(path: &Path, body: &str, force: bool) -> Result<()> {
    if force {
        return std::fs::write(path, body).map_err(|e| Error::io(path, e));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| Error::io(path, e))?;
    file.write_all(body.as_bytes())
        .map_err(|e| Error::io(path, e))
}

/// The questionnaire, one field per question, in the order the schema
/// documents them.
fn ask_everything() -> Result<Answers> {
    let mut answers = Answers::default();
    answers.source = Some(ask_source()?);
    answers.name = ask_name()?;
    answers.description = ask_text("one-line description?");
    answers.homepage = ask_text("homepage URL?");
    answers.kind = ask_kind();
    answers.prerelease = ui::question("consider prereleases when resolving `latest`?", false);
    answers.strip_prefix = ask_strip_prefix();
    answers.provides = wizard::split_list(&ui::prompt(
        "other names this package answers to, comma-separated?",
        "",
    ));
    answers.notes = ask_text("notes to print after a successful install?");
    answers.bin = ask_bins(&answers.name);
    answers.extra_paths = wizard::split_list(&ui::prompt(
        "extra files to record, comma-separated (man pages, completions)?",
        "",
    ));
    answers.asset_include = wizard::split_list(&ui::prompt(
        "release assets must match one of, comma-separated globs?",
        "",
    ));
    answers.asset_exclude = wizard::split_list(&ui::prompt(
        "release assets must never match, comma-separated globs?",
        "",
    ));
    answers.asset_target = ask_asset_targets();
    Ok(answers)
}

/// The one field with no default and no way to guess. A typo is re-asked
/// rather than fatal, so one slip does not restart the whole questionnaire.
fn ask_source() -> Result<String> {
    loop {
        let answer = ui::prompt_required("source: `owner/repo`, or `scheme:id` for a plugin?")?;
        match PackageRef::try_from(answer) {
            Ok(reference) => return Ok(reference.to_string()),
            Err(complaint) => ui::warn(&complaint),
        }
    }
}

/// The install name. The default is the current directory spelled like a
/// package name; a name that could not be used verbatim as one path component
/// is re-asked, because `Manifest::validate` would refuse the manifest it
/// produces — the same test, applied at the prompt instead of after it.
fn ask_name() -> Result<String> {
    let default = default_name()?;
    loop {
        let answer = ui::prompt("package name?", &default);
        if sanitize_component(&answer) == answer {
            return Ok(answer);
        }
        ui::warn(&format!(
            "`{answer}` cannot be the package name: it would install somewhere \
             other than where it says"
        ));
    }
}

/// The current directory's file name, spelled the way package names are.
///
/// Sanitized as well as normalized: an empty answer returns the default, so a
/// default `ask_name` rejects would be offered back forever.
fn default_name() -> Result<String> {
    let folder = std::env::current_dir()
        .ok()
        .and_then(|cwd| cwd.file_name().map(|n| n.to_string_lossy().to_string()))
        .ok_or_else(|| {
            Error::msg("the current directory has no name a package name could come from")
        })?;
    Ok(sanitize_component(&normalize_name(&folder)))
}

fn ask_kind() -> PackageKind {
    loop {
        let answer = ui::prompt("kind of payload: auto, binary or app?", "auto");
        match wizard::parse_kind(&answer) {
            Some(kind) => return kind,
            None => ui::warn("kind is auto, binary or app"),
        }
    }
}

fn ask_strip_prefix() -> Option<usize> {
    loop {
        let answer = ui::prompt(
            "leading wrapper directories to strip when extracting (0-8)?",
            "0",
        );
        match wizard::parse_strip_prefix(&answer) {
            Ok(levels) => return levels,
            Err(e) => ui::warn(&e.to_string()),
        }
    }
}

/// `bin` entries until the user stops adding them. The link name defaults to
/// the file the path glob points at, else the package name — the natural
/// answer in both cases. An entry with neither a path nor a name says
/// nothing, so it is re-asked rather than recorded.
fn ask_bins(package: &str) -> Vec<BinSpec> {
    let mut bins = Vec::new();
    while ui::question("add a `bin` entry?", false) {
        let path = ask_text("path glob inside the payload?");
        let link_default = path
            .as_deref()
            .map(last_segment)
            .filter(|segment| !segment.is_empty())
            .unwrap_or(package);
        let name = ui::prompt("link name?", link_default);
        if path.is_none() && name.is_empty() {
            ui::warn("a `bin` entry needs a path, a name, or both");
            continue;
        }
        bins.push(BinSpec {
            path,
            name: (!name.is_empty()).then_some(name),
        });
    }
    bins
}

/// The file a path glob points at, without the directories in front of it.
fn last_segment(glob: &str) -> &str {
    glob.rsplit('/').next().unwrap_or(glob)
}

/// Per-target asset overrides, one `target=glob` pair per line, until an empty
/// line ends the loop.
fn ask_asset_targets() -> BTreeMap<String, String> {
    let mut overrides = BTreeMap::new();
    loop {
        let answer = ui::prompt(
            "asset override for one target, as `target=glob` (empty line to finish)?",
            "",
        );
        if answer.is_empty() {
            return overrides;
        }
        match answer.split_once('=') {
            Some((target, glob)) if !target.trim().is_empty() && !glob.trim().is_empty() => {
                overrides.insert(target.trim().to_string(), glob.trim().to_string());
            }
            _ => ui::warn("an override looks like `macos-aarch64=*-aarch64-apple-darwin.tar.gz`"),
        }
    }
}

/// An optional line: an empty answer means the field is simply absent.
fn ask_text(question: &str) -> Option<String> {
    let answer = ui::prompt(question, "");
    (!answer.is_empty()).then_some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_force_write_never_truncates_an_existing_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ketch.toml");
        std::fs::write(&path, "original\n").unwrap();

        assert!(write_manifest(&path, "replacement\n", false).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original\n");
    }
}
