//! Which binary a package links when its manifest names none.
//!
//! Inference links every executable it discovers, which is right until a
//! release ships several that share the package's name — `rtok` beside
//! `rtok-hook`. Then one of them is the command and the rest are helpers, and
//! nothing in the payload says which. Discovery order cannot say either: it is
//! a sort over file names, and `rtok-hook.exe` sorts ahead of `rtok.exe`
//! while `rtok` sorts ahead of `rtok-hook` (B64). So the decision lives here,
//! once, with no knowledge of the OS: the exact package name, else the choice
//! remembered from last time, else the user's answer, else an error that says
//! how to name the binary in a manifest. Only the family members that lose
//! are dropped; executables with other names are linked as they always were.

use crate::error::{Error, Result};

/// How a binary came to be chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Picked {
    /// Its name is the package's name.
    Exact,
    /// It was chosen for this package before, and state remembered it.
    Remembered,
    /// The user picked it just now.
    Asked,
}

/// Compare-ready form of an executable's file name: lowercase, without the
/// extension Windows needs to run it, so `RTOK.exe` and `rtok` are one name.
pub fn stem(file_name: &str) -> String {
    without_extension(file_name, &[".exe", ".cmd", ".bat"]).to_ascii_lowercase()
}

/// The name a `bin` entry records for `file_name`, case kept: without
/// `.exe`, which Windows adds back when it links, so one entry finds
/// `rtok.exe` there and `rtok` everywhere else. A `.cmd` or `.bat` keeps its
/// extension, because linked under a bare name Windows would call it `.exe`.
pub fn command_name(file_name: &str) -> &str {
    without_extension(file_name, &[".exe"])
}

/// `file_name` without the first of `extensions` it ends with, ignoring case.
/// A name that is nothing but the extension is kept whole.
fn without_extension<'a>(file_name: &'a str, extensions: &[&str]) -> &'a str {
    let lower = file_name.to_ascii_lowercase();
    for ext in extensions {
        if lower.ends_with(ext) && lower.len() > ext.len() {
            // ASCII case folding keeps byte offsets, so this is a boundary.
            return &file_name[..file_name.len() - ext.len()];
        }
    }
    file_name
}

/// True when `file_name` belongs to the package's family: its stem is the
/// package name, or the package name followed by `-`, `_` or `.`.
///
/// The separator is required so that `go` and `gofmt`, which are two
/// commands rather than a command and its helper, keep being linked together.
pub fn shares_name(package: &str, file_name: &str) -> bool {
    let package = package.to_ascii_lowercase();
    let stem = stem(file_name);
    match stem.strip_prefix(&package) {
        Some("") => true,
        Some(rest) => rest.starts_with(['-', '_', '.']),
        None => false,
    }
}

/// Indices of the discovered files that compete for the package's name, when
/// there are at least two of them. Fewer means there is nothing to choose.
pub fn contenders(package: &str, files: &[String]) -> Vec<usize> {
    let family: Vec<usize> = files
        .iter()
        .enumerate()
        .filter(|(_, f)| shares_name(package, f))
        .map(|(i, _)| i)
        .collect();
    if family.len() < 2 {
        Vec::new()
    } else {
        family
    }
}

/// Choose one of `candidates` for `package`, in the fixed order: the exact
/// package name, the remembered choice, the answer `ask` returns (`None` when
/// it cannot ask), and otherwise an error naming every candidate.
///
/// `manifest_hint` is where the error tells the user to write `bin`.
pub fn choose(
    package: &str,
    candidates: &[String],
    remembered: Option<&str>,
    ask: &mut dyn FnMut(&[String]) -> Option<usize>,
    manifest_hint: &str,
) -> Result<(usize, Picked)> {
    let package_stem = package.to_ascii_lowercase();
    if let Some(i) = candidates.iter().position(|c| stem(c) == package_stem) {
        return Ok((i, Picked::Exact));
    }
    if let Some(wanted) = remembered.map(stem) {
        if let Some(i) = candidates.iter().position(|c| stem(c) == wanted) {
            return Ok((i, Picked::Remembered));
        }
    }
    if let Some(i) = ask(candidates).filter(|&i| i < candidates.len()) {
        return Ok((i, Picked::Asked));
    }
    Err(ambiguous(package, candidates, manifest_hint))
}

/// The error for a choice nobody could make: what the candidates are, and the
/// two ways to make it — a terminal to answer in, or a `bin` entry.
fn ambiguous(package: &str, candidates: &[String], manifest_hint: &str) -> Error {
    let first = candidates.first().map(|c| stem(c)).unwrap_or_default();
    Error::msg(format!(
        "`{package}` ships several binaries sharing its name ({}) and none is \
         named `{package}`; run the command in a terminal to pick one, or name \
         it in a manifest: `bin = [{{ name = \"{first}\" }}]` in {manifest_hint}",
        candidates.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn never(_: &[String]) -> Option<usize> {
        panic!("the prompt must not be reached")
    }

    #[test]
    fn the_exact_name_wins_in_windows_sort_order() {
        // B64: sorted, `rtok-hook.exe` comes first on Windows.
        let found = names(&["rtok-hook.exe", "rtok.exe"]);
        let picked = choose("rtok", &found, None, &mut never, "m.toml").unwrap();
        assert_eq!(picked, (1, Picked::Exact));
    }

    #[test]
    fn the_exact_name_wins_in_unix_sort_order() {
        let found = names(&["rtok", "rtok-hook"]);
        let picked = choose("rtok", &found, None, &mut never, "m.toml").unwrap();
        assert_eq!(picked, (0, Picked::Exact));
    }

    #[test]
    fn the_exact_name_ignores_case_and_the_exe_extension() {
        let found = names(&["Rtok-Hook.EXE", "RTOK.exe"]);
        let picked = choose("rtok", &found, None, &mut never, "m.toml").unwrap();
        assert_eq!(picked, (1, Picked::Exact));
    }

    #[test]
    fn the_exact_name_beats_a_remembered_choice() {
        let found = names(&["rtok", "rtok-hook"]);
        let picked = choose("rtok", &found, Some("rtok-hook"), &mut never, "m.toml").unwrap();
        assert_eq!(picked, (0, Picked::Exact));
    }

    #[test]
    fn a_remembered_choice_is_reused_without_asking() {
        let found = names(&["rtok-cli.exe", "rtok-hook.exe"]);
        let picked = choose("rtok", &found, Some("rtok-cli"), &mut never, "m.toml").unwrap();
        assert_eq!(picked, (0, Picked::Remembered));
    }

    #[test]
    fn a_remembered_choice_that_is_gone_falls_through_to_asking() {
        let found = names(&["rtok-cli", "rtok-hook"]);
        let mut offered = Vec::new();
        let picked = choose(
            "rtok",
            &found,
            Some("rtok-old"),
            &mut |c: &[String]| {
                offered = c.to_vec();
                Some(1)
            },
            "m.toml",
        )
        .unwrap();
        assert_eq!(picked, (1, Picked::Asked));
        assert_eq!(offered, found);
    }

    #[test]
    fn an_answer_out_of_range_is_not_a_choice() {
        let found = names(&["rtok-cli", "rtok-hook"]);
        let err = choose("rtok", &found, None, &mut |_: &[String]| Some(7), "m.toml");
        assert!(err.is_err());
    }

    #[test]
    fn without_a_prompt_the_error_lists_candidates_and_how_to_set_bin() {
        let found = names(&["rtok-cli", "rtok-hook"]);
        let err = choose(
            "rtok",
            &found,
            None,
            &mut |_: &[String]| None,
            "~/m/rtok.toml",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("rtok-cli"), "{err}");
        assert!(err.contains("rtok-hook"), "{err}");
        assert!(err.contains("bin = [{ name = \"rtok-cli\" }]"), "{err}");
        assert!(err.contains("~/m/rtok.toml"), "{err}");
    }

    #[test]
    fn contenders_need_two_files_sharing_the_package_name() {
        let found = names(&["go", "gofmt"]);
        assert!(contenders("go", &found).is_empty());
        let found = names(&["rg"]);
        assert!(contenders("ripgrep", &found).is_empty());
        let found = names(&["helper", "rtok", "rtok-hook"]);
        assert_eq!(contenders("rtok", &found), vec![1, 2]);
    }

    #[test]
    fn stems_drop_only_a_windows_run_extension() {
        assert_eq!(stem("rtok.EXE"), "rtok");
        assert_eq!(stem("tool.cmd"), "tool");
        assert_eq!(stem("tool.sh"), "tool.sh");
        assert_eq!(stem(".exe"), ".exe");
        assert_eq!(command_name("Rtok-Cli.EXE"), "Rtok-Cli");
        assert_eq!(command_name("rtok"), "rtok");
        assert_eq!(command_name("rtok.cmd"), "rtok.cmd");
    }
}
