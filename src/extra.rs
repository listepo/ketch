//! Man pages and shell completions declared in `extra_paths`.
//!
//! Classification is the trust boundary: a payload path becomes a man page
//! or a completion only from explicit metadata or the documented path rules.
//! Ambiguous entries are refused rather than guessed. Destination directories
//! come from the platform; this module only maps a classified entry onto them.

use crate::cli::Cli;
use crate::error::{Error, Result};
use crate::model::{
    ClassifiedExtra, CompletionShell, ExtraKind, ExtraPath, ExtraPathSpec, LinkRole,
};
use clap::CommandFactory;
use std::path::{Path, PathBuf};

/// One extra file ready to place: a payload-relative source and a destination
/// the platform already named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraPlacement {
    /// Path relative to the extracted payload / store prefix.
    pub rel_path: String,
    /// Absolute path ketch will create and record.
    pub dest: PathBuf,
    /// Recorded on the `LinkRecord` so uninstall and `binaries()` can tell
    /// this apart from a PATH link.
    pub role: LinkRole,
}

/// Classify every extra path and resolve user destinations. Does not touch disk.
pub fn plan(
    extras: &[ExtraPath],
    man_root: &Path,
    completion_dir: impl Fn(CompletionShell) -> PathBuf,
) -> Result<Vec<ExtraPlacement>> {
    let mut out = Vec::with_capacity(extras.len());
    let mut dests = Vec::new();
    for entry in extras {
        let classified = classify(entry)?;
        let dest = destination(&classified, man_root, &completion_dir)?;
        if dests.iter().any(|seen| seen == &dest) {
            return Err(Error::msg(format!(
                "multiple extra_paths want to create {}",
                dest.display()
            )));
        }
        dests.push(dest.clone());
        out.push(ExtraPlacement {
            rel_path: classified.rel_path,
            dest,
            role: match classified.kind {
                ExtraKind::Man => LinkRole::Man,
                ExtraKind::Completion => LinkRole::Completion,
            },
        });
    }
    Ok(out)
}

/// Classify one extra_paths entry. Used by `Manifest::validate` and [`plan`].
pub fn classify(entry: &ExtraPath) -> Result<ClassifiedExtra> {
    match entry {
        ExtraPath::Path(path) => classify_path(path),
        ExtraPath::Spec(spec) => classify_spec(spec),
    }
}

fn classify_path(path: &str) -> Result<ClassifiedExtra> {
    crate::extract::safe_member_path(Path::new(path))
        .map_err(|_| Error::msg(format!("extra path `{path}` must stay inside the package")))?;
    let components = path_components(path);
    let basename = file_name(path)?;
    let man = looks_like_man(&components, basename);
    let completion = looks_like_completion(&components, basename);
    match (man, completion) {
        (Some(section), None) => Ok(ClassifiedExtra {
            rel_path: path.to_string(),
            kind: ExtraKind::Man,
            shell: None,
            section: Some(section.to_string()),
        }),
        (None, Some(shell)) => Ok(ClassifiedExtra {
            rel_path: path.to_string(),
            kind: ExtraKind::Completion,
            shell: Some(shell),
            section: None,
        }),
        (Some(_), Some(_)) => Err(Error::msg(format!(
            "extra path `{path}` looks like both a man page and a completion; \
             set `kind` explicitly"
        ))),
        (None, None) => Err(Error::msg(format!(
            "extra path `{path}` is not a man page or a completion; \
             use `{{ path = \"{path}\", kind = \"man\" }}` or \
             `{{ path = \"{path}\", kind = \"completion\", shell = \"...\" }}`"
        ))),
    }
}

fn classify_spec(spec: &ExtraPathSpec) -> Result<ClassifiedExtra> {
    crate::extract::safe_member_path(Path::new(&spec.path)).map_err(|_| {
        Error::msg(format!(
            "extra path `{}` must stay inside the package",
            spec.path
        ))
    })?;
    let basename = file_name(&spec.path)?;
    let components = path_components(&spec.path);
    match spec.kind {
        ExtraKind::Man => {
            let section = match spec.section.as_deref() {
                Some(section) if man_section_ok(section) => section.to_string(),
                Some(section) => {
                    return Err(Error::msg(format!(
                        "extra path `{}` has unusable man section `{section}`",
                        spec.path
                    )))
                }
                None => man_section_from_basename(basename)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        Error::msg(format!(
                            "extra path `{}` needs `section` (for example `1`)",
                            spec.path
                        ))
                    })?,
            };
            Ok(ClassifiedExtra {
                rel_path: spec.path.clone(),
                kind: ExtraKind::Man,
                shell: None,
                section: Some(section),
            })
        }
        ExtraKind::Completion => {
            let shell = match spec.shell {
                Some(shell) => shell,
                None => completion_shell_from_basename(basename, &components).ok_or_else(|| {
                    Error::msg(format!(
                        "extra path `{}` needs `shell` (bash, zsh, fish, elvish, powershell)",
                        spec.path
                    ))
                })?,
            };
            Ok(ClassifiedExtra {
                rel_path: spec.path.clone(),
                kind: ExtraKind::Completion,
                shell: Some(shell),
                section: None,
            })
        }
    }
}

fn destination(
    classified: &ClassifiedExtra,
    man_root: &Path,
    completion_dir: &impl Fn(CompletionShell) -> PathBuf,
) -> Result<PathBuf> {
    let basename = file_name(&classified.rel_path)?;
    match classified.kind {
        ExtraKind::Man => {
            let section = classified.section.as_deref().ok_or_else(|| {
                Error::msg(format!(
                    "extra path `{}` is a man page without a section",
                    classified.rel_path
                ))
            })?;
            Ok(man_root.join(format!("man{section}")).join(basename))
        }
        ExtraKind::Completion => {
            let shell = classified.shell.ok_or_else(|| {
                Error::msg(format!(
                    "extra path `{}` is a completion without a shell",
                    classified.rel_path
                ))
            })?;
            Ok(completion_dir(shell).join(completion_dest_name(basename, shell)))
        }
    }
}

fn completion_dest_name(basename: &str, shell: CompletionShell) -> String {
    match shell {
        CompletionShell::Bash if basename.ends_with(".bash") => basename
            .strip_suffix(".bash")
            .unwrap_or(basename)
            .to_string(),
        CompletionShell::Zsh if !basename.starts_with('_') => format!("_{basename}"),
        _ => basename.to_string(),
    }
}

/// Join `rel` under `root`, refusing anything that is not a regular file
/// still inside `root`.
pub fn resolve_under(root: &Path, rel: &str) -> Result<PathBuf> {
    let safe = crate::extract::safe_member_path(Path::new(rel))
        .map_err(|_| Error::msg(format!("extra path `{rel}` must stay inside the package")))?;
    let target = root.join(&safe);
    if !target.starts_with(root) {
        return Err(Error::msg(format!(
            "extra path `{rel}` must stay inside the package"
        )));
    }
    if !target.is_file() {
        return Err(Error::msg(format!(
            "extra path `{rel}` is not a file in the payload"
        )));
    }
    Ok(target)
}

/// Write ketch's own man page and completion scripts into `prefix`.
///
/// Generated under the store so the links that follow have the same ownership
/// proof as binaries: they point at files ketch placed.
pub fn write_ketch_docs(prefix: &Path) -> Result<Vec<ExtraPath>> {
    let man_rel = "share/man/man1/ketch.1";
    let man_path = prefix.join(man_rel);
    if let Some(parent) = man_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(&man_path, render_manpage()).map_err(|e| Error::io(&man_path, e))?;

    let mut extras = vec![ExtraPath::Path(man_rel.to_string())];
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    for shell in CompletionShell::ALL {
        let rel = generated_completion_rel(shell);
        let path = prefix.join(&rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut buf = Vec::new();
        clap_complete::generate(shell.to_clap(), &mut command, &name, &mut buf);
        std::fs::write(&path, buf).map_err(|e| Error::io(&path, e))?;
        extras.push(ExtraPath::Spec(ExtraPathSpec {
            path: rel,
            kind: ExtraKind::Completion,
            shell: Some(shell),
            section: None,
        }));
    }
    Ok(extras)
}

fn generated_completion_rel(shell: CompletionShell) -> String {
    match shell {
        CompletionShell::Bash => "share/ketch/completions/ketch".to_string(),
        CompletionShell::Zsh => "share/ketch/completions/_ketch".to_string(),
        CompletionShell::Fish => "share/ketch/completions/ketch.fish".to_string(),
        CompletionShell::Elvish => "share/ketch/completions/ketch.elv".to_string(),
        CompletionShell::Powershell => "share/ketch/completions/ketch.ps1".to_string(),
    }
}

fn render_manpage() -> String {
    let cmd = Cli::command();
    let name = cmd.get_name();
    let about = cmd
        .get_about()
        .or_else(|| cmd.get_long_about())
        .map(|s| s.to_string())
        .unwrap_or_else(|| env!("CARGO_PKG_DESCRIPTION").to_string());
    let mut out = String::new();
    out.push_str(".\\\" Generated by ketch from its own CLI. Do not edit.\n");
    out.push_str(".TH KETCH 1\n");
    out.push_str(".SH NAME\n");
    out.push_str(&format!("{} \\- {}\n", name, roff(&about)));
    out.push_str(".SH SYNOPSIS\n");
    out.push_str(".B ketch\n");
    out.push_str("[\\fIOPTIONS\\fR] <\\fICOMMAND\\fR>\n");
    out.push_str(".SH DESCRIPTION\n");
    out.push_str(&roff(&about));
    out.push('\n');
    out.push_str(".SH COMMANDS\n");
    let mut subcommands: Vec<_> = cmd.get_subcommands().collect();
    subcommands.sort_by_key(|c| c.get_name());
    for sub in subcommands {
        if sub.is_hide_set() {
            continue;
        }
        let about = sub.get_about().map(|s| s.to_string()).unwrap_or_default();
        out.push_str(".TP\n");
        out.push_str(&format!(".B {}\n", sub.get_name()));
        if !about.is_empty() {
            out.push_str(&roff(&about));
            out.push('\n');
        }
    }
    out.push_str(".SH SEE ALSO\n");
    out.push_str("ketch help, ketch completions, ketch doctor\n");
    out
}

fn roff(text: &str) -> String {
    text.replace('\\', "\\\\").replace('-', "\\-")
}

fn path_components(path: &str) -> Vec<String> {
    Path::new(path)
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect()
}

fn file_name(path: &str) -> Result<&str> {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty() && *n != "." && *n != "..")
        .ok_or_else(|| Error::msg(format!("extra path `{path}` has no file name")))
}

fn looks_like_man(components: &[String], basename: &str) -> Option<String> {
    if !has_man_dir(components) {
        return None;
    }
    man_section_from_basename(basename).map(str::to_string)
}

fn looks_like_completion(components: &[String], basename: &str) -> Option<CompletionShell> {
    if !has_completion_dir(components) {
        return None;
    }
    completion_shell_from_basename(basename, components)
}

fn has_completion_dir(components: &[String]) -> bool {
    components.iter().any(|c| {
        matches!(
            c.as_str(),
            "complete" | "completions" | "completion" | "bash-completion" | "site-functions"
        )
    })
}

fn has_man_dir(components: &[String]) -> bool {
    components
        .iter()
        .any(|c| c == "man" || c == "doc" || c == "docs" || is_man_n_dir(c))
}

fn is_man_n_dir(c: &str) -> bool {
    c.strip_prefix("man")
        .is_some_and(|rest| !rest.is_empty() && man_section_ok(rest))
}

fn man_section_ok(section: &str) -> bool {
    let mut chars = section.chars();
    match chars.next() {
        Some(c) if c.is_ascii_digit() && c != '0' => chars.all(|c| c.is_ascii_lowercase()),
        _ => false,
    }
}

fn man_section_from_basename(name: &str) -> Option<&str> {
    let name = name.strip_suffix(".gz").unwrap_or(name);
    let (stem, section) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    man_section_ok(section).then_some(section)
}

fn completion_shell_from_basename(name: &str, components: &[String]) -> Option<CompletionShell> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".bash") {
        return Some(CompletionShell::Bash);
    }
    if lower.ends_with(".zsh") {
        return Some(CompletionShell::Zsh);
    }
    if lower.ends_with(".fish") {
        return Some(CompletionShell::Fish);
    }
    if lower.ends_with(".ps1") {
        return Some(CompletionShell::Powershell);
    }
    if lower.ends_with(".elv") {
        return Some(CompletionShell::Elvish);
    }
    if name.starts_with('_') && !name[1..].contains('.') {
        return Some(CompletionShell::Zsh);
    }
    if !name.contains('.') && components.iter().any(|c| c == "bash-completion") {
        return Some(CompletionShell::Bash);
    }
    if !name.contains('.') && components.iter().any(|c| c == "site-functions") {
        return Some(CompletionShell::Zsh);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ExtraPathSpec;

    fn classified_path(path: &str) -> ClassifiedExtra {
        classify(&ExtraPath::Path(path.to_string())).expect(path)
    }

    #[test]
    fn documented_ripgrep_paths_classify() {
        let bash = classified_path("complete/rg.bash");
        assert_eq!(bash.kind, ExtraKind::Completion);
        assert_eq!(bash.shell, Some(CompletionShell::Bash));

        let man = classified_path("doc/rg.1");
        assert_eq!(man.kind, ExtraKind::Man);
        assert_eq!(man.section.as_deref(), Some("1"));

        let nested = classified_path("share/man/man1/rg.1");
        assert_eq!(nested.kind, ExtraKind::Man);
        assert_eq!(nested.section.as_deref(), Some("1"));
    }

    #[test]
    fn gzipped_man_page_under_man1_classifies() {
        let man = classified_path("share/man/man8/tool.8.gz");
        assert_eq!(man.kind, ExtraKind::Man);
        assert_eq!(man.section.as_deref(), Some("8"));
    }

    #[test]
    fn zsh_compdef_file_classifies() {
        let zsh = classified_path("completions/_rg");
        assert_eq!(zsh.kind, ExtraKind::Completion);
        assert_eq!(zsh.shell, Some(CompletionShell::Zsh));
    }

    #[test]
    fn extensionless_file_in_bash_completion_is_bash() {
        let bash = classified_path("share/bash-completion/completions/rg");
        assert_eq!(bash.kind, ExtraKind::Completion);
        assert_eq!(bash.shell, Some(CompletionShell::Bash));
    }

    #[test]
    fn rejects_a_man_like_file_in_a_completion_directory() {
        let err = classify(&ExtraPath::Path("complete/rg.1".into())).unwrap_err();
        assert!(
            err.to_string().contains("kind = "),
            "must refuse rather than guess: {err}"
        );
    }

    #[test]
    fn rejects_a_bare_man_page_at_payload_root() {
        let err = classify(&ExtraPath::Path("rg.1".into())).unwrap_err();
        assert!(err.to_string().contains("not a man page"), "{err}");
    }

    #[test]
    fn rejects_license_and_other_untyped_files() {
        let err = classify(&ExtraPath::Path("LICENSE".into())).unwrap_err();
        assert!(err.to_string().contains("kind = "), "{err}");
    }

    #[test]
    fn explicit_kind_wins_over_path_shape() {
        let spec = ExtraPath::Spec(ExtraPathSpec {
            path: "misc/custom".into(),
            kind: ExtraKind::Man,
            shell: None,
            section: Some("1".into()),
        });
        let classified = classify(&spec).unwrap();
        assert_eq!(classified.kind, ExtraKind::Man);
        assert_eq!(classified.section.as_deref(), Some("1"));
    }

    #[test]
    fn explicit_completion_without_shell_or_suffix_is_refused() {
        let spec = ExtraPath::Spec(ExtraPathSpec {
            path: "misc/custom".into(),
            kind: ExtraKind::Completion,
            shell: None,
            section: None,
        });
        assert!(classify(&spec).is_err());
    }

    #[test]
    fn escaping_paths_are_refused() {
        assert!(classify(&ExtraPath::Path("../etc/passwd".into())).is_err());
        assert!(classify(&ExtraPath::Path("/etc/passwd".into())).is_err());
    }

    #[test]
    fn plan_maps_onto_platform_destinations() {
        let extras = vec![
            ExtraPath::Path("complete/rg.bash".into()),
            ExtraPath::Path("doc/rg.1".into()),
        ];
        let man_root = PathBuf::from("/home/me/.local/share/man");
        let planned = plan(&extras, &man_root, |shell| match shell {
            CompletionShell::Bash => {
                PathBuf::from("/home/me/.local/share/bash-completion/completions")
            }
            other => PathBuf::from(format!("/tmp/{other:?}")),
        })
        .unwrap();
        assert_eq!(planned.len(), 2);
        assert_eq!(
            planned[0].dest,
            PathBuf::from("/home/me/.local/share/bash-completion/completions/rg")
        );
        assert_eq!(planned[0].role, LinkRole::Completion);
        assert_eq!(
            planned[1].dest,
            PathBuf::from("/home/me/.local/share/man/man1/rg.1")
        );
        assert_eq!(planned[1].role, LinkRole::Man);
    }

    #[test]
    fn resolve_under_requires_a_file_inside_the_payload() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("doc")).unwrap();
        std::fs::write(root.join("doc/rg.1"), b".TH RG 1\n").unwrap();

        assert_eq!(
            resolve_under(root, "doc/rg.1").unwrap(),
            root.join("doc/rg.1")
        );
        assert!(resolve_under(root, "doc/missing.1").is_err());
        assert!(resolve_under(root, "../doc/rg.1").is_err());
        assert!(resolve_under(root, "doc").is_err());
    }

    #[test]
    fn generated_ketch_docs_classify_and_stay_in_the_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path().join("store/ketch/1.0.0");
        let extras = write_ketch_docs(&prefix).unwrap();
        assert!(prefix.join("share/man/man1/ketch.1").is_file());
        assert!(prefix.join("share/ketch/completions/ketch").is_file());
        for entry in &extras {
            classify(entry).unwrap_or_else(|e| panic!("{}: {e}", entry.as_rel_path()));
        }
        let man = std::fs::read_to_string(prefix.join("share/man/man1/ketch.1")).unwrap();
        assert!(man.contains(".TH KETCH 1"));
        assert!(man.contains(".SH COMMANDS"));
    }

    #[test]
    fn extra_paths_toml_accepts_strings_and_tables() {
        let manifest: crate::model::Manifest = toml::from_str(
            r#"
name = "rg"
source = "github:BurntSushi/ripgrep"
extra_paths = [
  "complete/rg.bash",
  { path = "misc/custom", kind = "man", section = "1" },
]
"#,
        )
        .unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.extra_paths.len(), 2);
    }

    #[test]
    fn license_as_an_extra_path_is_refused_by_validate() {
        let mut manifest =
            crate::model::Manifest::inferred(crate::model::PackageRef::github("a/b"));
        manifest.extra_paths = vec![ExtraPath::Path("LICENSE".into())];
        assert!(manifest.validate().is_err());
    }
}
