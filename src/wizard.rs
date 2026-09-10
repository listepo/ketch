//! The `ketch config create` questionnaire: answers in, a package file out.
//!
//! Kept apart from `cmd/config.rs` because the mapping from answers to a
//! manifest is business logic with rules worth testing, while the command body
//! is only prompting, previewing and writing. Everything here works on plain
//! data, so the rules run in tests with no terminal attached.

use crate::error::{Error, Result};
use crate::model::{normalize_name, BinSpec, Manifest, PackageKind, PackageRef};
use std::collections::BTreeMap;

/// The answers the questionnaire collected, before they become a manifest.
///
/// Plain data on purpose: the command fills it field by field, and the mapping
/// below is the only place the answers are interpreted.
#[derive(Debug, Default)]
pub struct Answers {
    /// `owner/repo` or `scheme:id`, already parsed once so a re-ask happened
    /// at the prompt. `None` until answered: there is no default to invent.
    pub source: Option<String>,
    /// The install name as typed. The default comes from the current
    /// directory; `manifest` normalizes it the way repo names are normalized.
    pub name: String,
    pub description: Option<String>,
    pub homepage: Option<String>,
    /// What the payload is; `Auto` is the default and means "look inside".
    pub kind: PackageKind,
    pub prerelease: bool,
    /// Leading wrapper directories to drop. `None` means "leave it to ketch";
    /// an answer of `0` means the same thing and is stored as `None`.
    pub strip_prefix: Option<usize>,
    pub provides: Vec<String>,
    pub notes: Option<String>,
    pub bin: Vec<BinSpec>,
    pub extra_paths: Vec<String>,
    pub asset_include: Vec<String>,
    pub asset_exclude: Vec<String>,
    /// Per-target asset overrides, keyed by `<os>-<arch>`.
    pub asset_target: BTreeMap<String, String>,
}

/// Turn answers into the manifest they describe.
///
/// The `source` answer goes back through `PackageRef`'s own parser rather than
/// a new one, so the questionnaire accepts exactly what every other command
/// accepts. [`Manifest::validate`] runs last and its errors propagate: a name
/// or path that cannot be used verbatim is refused here, not when installing.
pub fn manifest(answers: &Answers) -> Result<Manifest> {
    let source_text = answers
        .source
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| Error::msg("`source` was never answered, and it has no default"))?;
    let source = PackageRef::try_from(source_text.to_string()).map_err(Error::msg)?;
    let name = normalize_name(&answers.name);
    if name.is_empty() {
        return Err(Error::msg(
            "`name` was never answered, and a manifest installs under its name",
        ));
    }
    let manifest = Manifest {
        name,
        source,
        description: text(answers.description.as_deref()),
        homepage: text(answers.homepage.as_deref()),
        kind: answers.kind,
        asset: crate::model::AssetSelector {
            include: answers.asset_include.clone(),
            exclude: answers.asset_exclude.clone(),
            target: answers.asset_target.clone(),
        },
        bin: answers.bin.clone(),
        strip_prefix: answers.strip_prefix,
        prerelease: answers.prerelease,
        provides: answers.provides.clone(),
        notes: text(answers.notes.as_deref()),
        extra_paths: answers.extra_paths.clone(),
    };
    manifest.validate()?;
    Ok(manifest)
}

/// Render a manifest as the `ketch.toml` the project keeps and the registry
/// receives.
///
/// Field order follows `docs/MANIFESTS.md`, and every empty or default field
/// is omitted so the file says only what someone chose to say: the smallest
/// answer set produces a file with `name` and `source` and nothing else.
pub fn render(manifest: &Manifest) -> String {
    let mut out = String::from("# Written by `ketch config create`. Schema: docs/MANIFESTS.md.\n");
    out.push_str("name = ");
    out.push_str(&string(&manifest.name));
    out.push('\n');
    out.push_str("source = ");
    out.push_str(&string(&manifest.source.to_string()));
    out.push('\n');
    if let Some(description) = &manifest.description {
        out.push_str("description = ");
        out.push_str(&string(description));
        out.push('\n');
    }
    if let Some(homepage) = &manifest.homepage {
        out.push_str("homepage = ");
        out.push_str(&string(homepage));
        out.push('\n');
    }
    if manifest.kind != PackageKind::Auto {
        let kind = match manifest.kind {
            PackageKind::Binary => "binary",
            PackageKind::App => "app",
            // Ruled out by the `if`; spelled out so a new variant fails here.
            PackageKind::Auto => "",
        };
        out.push_str(&format!("kind = {}\n", string(kind)));
    }
    if manifest.prerelease {
        out.push_str("prerelease = true\n");
    }
    if let Some(levels) = manifest.strip_prefix {
        out.push_str(&format!("strip_prefix = {levels}\n"));
    }
    if !manifest.provides.is_empty() {
        out.push_str(&format!("provides = {}\n", list(&manifest.provides)));
    }
    if let Some(notes) = &manifest.notes {
        out.push_str(&format!("notes = {}\n", string(notes)));
    }
    // A blank line before the list-shaped fields keeps the scalar block and
    // the list block readable as two groups, the way the documented example
    // is laid out.
    if !manifest.bin.is_empty() || !manifest.extra_paths.is_empty() {
        out.push('\n');
    }
    if !manifest.bin.is_empty() {
        let entries: Vec<String> = manifest.bin.iter().map(bin_entry).collect();
        out.push_str(&format!("bin = [{}]\n", entries.join(", ")));
    }
    if !manifest.extra_paths.is_empty() {
        out.push_str(&format!("extra_paths = {}\n", list(&manifest.extra_paths)));
    }
    if !manifest.asset.include.is_empty() || !manifest.asset.exclude.is_empty() {
        out.push_str("\n[asset]\n");
        if !manifest.asset.include.is_empty() {
            out.push_str(&format!("include = {}\n", list(&manifest.asset.include)));
        }
        if !manifest.asset.exclude.is_empty() {
            out.push_str(&format!("exclude = {}\n", list(&manifest.asset.exclude)));
        }
    }
    if !manifest.asset.target.is_empty() {
        out.push_str("\n[asset.target]\n");
        for (target, glob) in &manifest.asset.target {
            out.push_str(&format!("{} = {}\n", string(target), string(glob)));
        }
    }
    out
}

/// Parse a `kind` answer. Case and surrounding space are forgiven; anything
/// else is not a kind and the question is asked again.
pub fn parse_kind(text: &str) -> Option<PackageKind> {
    match text.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(PackageKind::Auto),
        "binary" => Some(PackageKind::Binary),
        "app" => Some(PackageKind::App),
        _ => None,
    }
}

/// Parse a `strip_prefix` answer: a whole number from 0 to 8, where 0 means
/// "leave it to ketch" and is stored as `None`.
///
/// The cap is the one `Manifest::validate` enforces; asking again here means
/// the answer that reaches a manifest has already passed the rule it will be
/// checked against.
pub fn parse_strip_prefix(text: &str) -> Result<Option<usize>> {
    let trimmed = text.trim();
    let levels = trimmed
        .parse::<usize>()
        .map_err(|_| Error::msg(format!("`{trimmed}` is not a number of directories")))?;
    if levels > 8 {
        return Err(Error::msg("`strip_prefix` must be at most 8"));
    }
    Ok((levels > 0).then_some(levels))
}

/// Split a comma-separated answer into its parts, trimmed, with blanks gone:
/// `rg, rgr ,,` is two aliases, not two aliases and an empty one.
pub fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// A quoted, escaped TOML string.
///
/// Built by rendering a `toml::Value` so escaping is never hand-rolled: one
/// writer, one answer to what quotes, backslashes and control bytes mean.
fn string(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

/// A TOML array of strings, escaped the same way [`string`] escapes.
fn list(items: &[String]) -> String {
    toml::Value::Array(
        items
            .iter()
            .map(|i| toml::Value::String(i.clone()))
            .collect(),
    )
    .to_string()
}

/// One `bin` entry as an inline table, `path` first to match the documented
/// examples. The keys are fixed identifiers, so only the values need escaping.
fn bin_entry(spec: &BinSpec) -> String {
    match (&spec.path, &spec.name) {
        (Some(path), Some(name)) => {
            format!("{{ path = {}, name = {} }}", string(path), string(name))
        }
        (Some(path), None) => format!("{{ path = {} }}", string(path)),
        (None, Some(name)) => format!("{{ name = {} }}", string(name)),
        // `Manifest::validate` refuses this before a render could reach it;
        // empty keeps the output well-formed if that ever changes.
        (None, None) => String::new(),
    }
}

/// An optional answer: trimmed, and whitespace-only counts as absent.
fn text(answer: Option<&str>) -> Option<String> {
    answer
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Every field answered, in the shapes the questionnaire collects them.
    fn full_answers() -> Answers {
        let mut asset_target = BTreeMap::new();
        asset_target.insert(
            "macos-aarch64".to_string(),
            "*-aarch64-apple-darwin.tar.gz".to_string(),
        );
        asset_target.insert(
            "macos-x86_64".to_string(),
            "*-x86_64-apple-darwin.tar.gz".to_string(),
        );
        Answers {
            source: Some("github:BurntSushi/ripgrep".to_string()),
            // Typed the way a person types it; the manifest spells it the way
            // ketch installs it.
            name: "RipGrep.rs".to_string(),
            description: Some("Recursively search directories for a regex pattern".to_string()),
            homepage: Some("https://github.com/BurntSushi/ripgrep".to_string()),
            kind: PackageKind::Binary,
            prerelease: true,
            strip_prefix: Some(1),
            provides: vec!["rg".to_string()],
            notes: Some("Shell completions are under complete/ in the payload.".to_string()),
            bin: vec![BinSpec {
                path: Some("*/rg".to_string()),
                name: Some("rg".to_string()),
            }],
            extra_paths: vec!["complete/rg.bash".to_string(), "doc/rg.1".to_string()],
            asset_include: vec!["*-apple-darwin.tar.gz".to_string()],
            asset_exclude: vec!["*-musl-*".to_string()],
            asset_target,
        }
    }

    fn minimal_answers(name: &str, source: &str) -> Answers {
        Answers {
            source: Some(source.to_string()),
            name: name.to_string(),
            ..Answers::default()
        }
    }

    #[test]
    fn a_full_answer_set_maps_to_the_expected_manifest() {
        let manifest = manifest(&full_answers()).unwrap();
        assert_eq!(manifest.name, "ripgrep");
        assert_eq!(manifest.source.to_string(), "github:BurntSushi/ripgrep");
        assert_eq!(
            manifest.description.as_deref(),
            Some("Recursively search directories for a regex pattern")
        );
        assert_eq!(
            manifest.homepage.as_deref(),
            Some("https://github.com/BurntSushi/ripgrep")
        );
        assert_eq!(manifest.kind, PackageKind::Binary);
        assert!(manifest.prerelease);
        assert_eq!(manifest.strip_prefix, Some(1));
        assert_eq!(manifest.provides, vec!["rg".to_string()]);
        assert_eq!(
            manifest.notes.as_deref(),
            Some("Shell completions are under complete/ in the payload.")
        );
        assert_eq!(manifest.bin.len(), 1);
        assert_eq!(manifest.bin[0].path.as_deref(), Some("*/rg"));
        assert_eq!(manifest.bin[0].name.as_deref(), Some("rg"));
        assert_eq!(
            manifest.asset.include,
            vec!["*-apple-darwin.tar.gz".to_string()]
        );
        assert_eq!(manifest.asset.exclude, vec!["*-musl-*".to_string()]);
        assert_eq!(
            manifest
                .asset
                .target
                .get("macos-aarch64")
                .map(String::as_str),
            Some("*-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(
            manifest.extra_paths,
            vec!["complete/rg.bash".to_string(), "doc/rg.1".to_string()]
        );
    }

    #[test]
    fn render_puts_every_field_in_the_documented_order() {
        let body = render(&manifest(&full_answers()).unwrap());
        let expected = concat!(
            "# Written by `ketch config create`. Schema: docs/MANIFESTS.md.\n",
            "name = \"ripgrep\"\n",
            "source = \"github:BurntSushi/ripgrep\"\n",
            "description = \"Recursively search directories for a regex pattern\"\n",
            "homepage = \"https://github.com/BurntSushi/ripgrep\"\n",
            "kind = \"binary\"\n",
            "prerelease = true\n",
            "strip_prefix = 1\n",
            "provides = [\"rg\"]\n",
            "notes = \"Shell completions are under complete/ in the payload.\"\n",
            "\n",
            "bin = [{ path = \"*/rg\", name = \"rg\" }]\n",
            "extra_paths = [\"complete/rg.bash\", \"doc/rg.1\"]\n",
            "\n",
            "[asset]\n",
            "include = [\"*-apple-darwin.tar.gz\"]\n",
            "exclude = [\"*-musl-*\"]\n",
            "\n",
            "[asset.target]\n",
            "\"macos-aarch64\" = \"*-aarch64-apple-darwin.tar.gz\"\n",
            "\"macos-x86_64\" = \"*-x86_64-apple-darwin.tar.gz\"\n",
        );
        assert_eq!(body, expected);
    }

    #[test]
    fn the_push_loader_accepts_what_the_wizard_renders() {
        let manifest = manifest(&full_answers()).unwrap();
        let body = render(&manifest);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ketch.toml");
        std::fs::write(&file, &body).unwrap();
        let proposal = crate::push::load(&file).unwrap();
        // The body is sent to the registry verbatim, so equality here is the
        // whole claim: what was written is what loads, byte for byte.
        assert_eq!(proposal.body, body);
        assert_eq!(proposal.name, manifest.name);
        assert_eq!(
            proposal.manifest.source.to_string(),
            manifest.source.to_string()
        );
        assert_eq!(proposal.manifest.kind, manifest.kind);
        assert_eq!(proposal.manifest.strip_prefix, manifest.strip_prefix);
        assert_eq!(
            proposal.manifest.asset.include,
            vec!["*-apple-darwin.tar.gz".to_string()]
        );
        assert_eq!(
            proposal
                .manifest
                .asset
                .target
                .get("macos-x86_64")
                .map(String::as_str),
            Some("*-x86_64-apple-darwin.tar.gz")
        );
    }

    #[test]
    fn quotes_and_backslashes_in_an_answer_survive_the_round_trip() {
        let hostile = r#"says "hi", C:\path\to, and \ itself"#;
        let answers = Answers {
            description: Some(hostile.to_string()),
            ..minimal_answers("tool", "github:acme/tool")
        };
        let body = render(&manifest(&answers).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ketch.toml");
        std::fs::write(&file, &body).unwrap();
        let back = crate::push::load(&file).unwrap();
        assert_eq!(back.manifest.description.as_deref(), Some(hostile));
    }

    #[test]
    fn defaults_are_omitted_and_the_smallest_file_names_only_name_and_source() {
        // A bare `owner/repo` answer must come back out as the `scheme:id`
        // form, the way every other command would read it.
        let body = render(&manifest(&minimal_answers("ripgrep", "BurntSushi/ripgrep")).unwrap());
        let expected = concat!(
            "# Written by `ketch config create`. Schema: docs/MANIFESTS.md.\n",
            "name = \"ripgrep\"\n",
            "source = \"github:BurntSushi/ripgrep\"\n",
        );
        assert_eq!(body, expected);
    }

    #[test]
    fn answers_whose_manifest_would_fail_validation_are_refused() {
        assert!(manifest(&Answers::default()).is_err(), "no source at all");
        let mut no_name = minimal_answers("", "github:acme/tool");
        no_name.name = "   ".to_string();
        assert!(manifest(&no_name).is_err(), "no name");
        let mut escaping = minimal_answers("../evil", "github:acme/tool");
        escaping.name = "../evil".to_string();
        assert!(manifest(&escaping).is_err(), "name escapes the store");
        let mut wordless_alias = minimal_answers("tool", "github:acme/tool");
        wordless_alias.provides = vec!["two words".to_string()];
        assert!(manifest(&wordless_alias).is_err(), "alias nobody can type");
        let mut empty_bin = minimal_answers("tool", "github:acme/tool");
        empty_bin.bin = vec![BinSpec::default()];
        assert!(manifest(&empty_bin).is_err(), "bin entry that says nothing");
    }

    #[test]
    fn kind_answers_accept_only_the_three_words() {
        assert_eq!(parse_kind("auto"), Some(PackageKind::Auto));
        assert_eq!(parse_kind("APP"), Some(PackageKind::App));
        assert_eq!(parse_kind(" binary "), Some(PackageKind::Binary));
        assert_eq!(parse_kind("exe"), None);
    }

    #[test]
    fn strip_prefix_answers_must_be_a_small_whole_number() {
        assert_eq!(parse_strip_prefix("0").unwrap(), None);
        assert_eq!(parse_strip_prefix("2").unwrap(), Some(2));
        assert_eq!(parse_strip_prefix("8").unwrap(), Some(8));
        assert!(parse_strip_prefix("9").is_err());
        assert!(parse_strip_prefix("one").is_err());
        assert!(parse_strip_prefix("-1").is_err());
    }

    #[test]
    fn lists_split_on_commas_and_drop_the_blanks() {
        assert_eq!(
            split_list("rg, rgr ,,"),
            vec!["rg".to_string(), "rgr".to_string()]
        );
        assert!(split_list(" , ").is_empty());
        assert!(split_list("").is_empty());
    }
}
