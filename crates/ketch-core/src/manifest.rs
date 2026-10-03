// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! Turning what the user typed into a `Manifest`.
//!
//! Four tiers, in order: a user manifest in `~/.ketch/manifests/<name>.toml`,
//! the fetched package registry, the built-in registry compiled into the
//! binary, then inference from the source reference itself. Inference is what
//! lets `ketch install owner/repo` work for a repository nobody has curated.
//!
//! It is also the one place a user manifest is edited: [`write_bins`] records
//! the binaries a package links, through `toml_file`'s in-place edit.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::{normalize_name, Manifest, ManifestOrigin, PackageRef, PackageSpec};
use crate::report::{Ctx, Report};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The built-in registry: curated manifests for tools whose release layout
/// needs a hint that inference cannot guess.
pub const BUILTIN_TOML: &str = include_str!("builtin.toml");

/// A file holding several manifests, as `builtin.toml` does.
///
/// Strict like `Manifest` itself: this is the shape a user manifest takes, and
/// a key outside `[[package]]` is a manifest that parsed as nothing at all.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    #[serde(default)]
    package: Vec<Manifest>,
}

fn parse_registry(text: &str, what: &str) -> Result<Vec<Manifest>> {
    let doc = crate::toml_file::Document::parse(text, what)?;
    // One file may hold either a single manifest or a `[[package]]` array.
    // Which one is decided from the parsed shape, not from the source text: a
    // single manifest that merely mentions `[[package]]` — in a description, in
    // a note — is still a manifest, and sniffing for the string parsed it as an
    // array instead, which `#[serde(default)]` then turned into no packages at
    // all. The whole file disappeared without a word.
    let manifests = if doc.is_array("package") {
        doc.deserialize::<Registry>()?.package
    } else {
        vec![doc.deserialize::<Manifest>()?]
    };
    // Serde has checked the shape; this checks the values it cannot judge.
    for manifest in &manifests {
        manifest
            .validate()
            .map_err(|e| Error::parse(what, format!("package `{}`: {e}", manifest.name)))?;
    }
    Ok(manifests)
}

/// Resolves specs to manifests. Built once per command.
pub struct Resolver {
    builtin: Vec<Manifest>,
    /// Packages from the fetched registry, paired with their `ketch.toml`.
    registry: Vec<(Manifest, PathBuf)>,
    /// User manifests, paired with the file they came from so the origin can
    /// point at something the user can edit.
    user: Vec<(Manifest, PathBuf)>,
}

impl Resolver {
    pub fn new(cx: &Ctx<'_>) -> Result<Self> {
        let cfg = cx.cfg;
        // A malformed built-in registry is a bug in ketch, not in the user's
        // setup, so it fails loudly rather than degrading to inference.
        let builtin = parse_registry(BUILTIN_TOML, "the built-in registry")?;
        Ok(Resolver {
            user: load_user_manifests(&cfg.manifest_dir, cx.report),
            registry: crate::registry::load(cx),
            builtin,
        })
    }

    /// Resolve a spec, reporting where the manifest came from.
    pub fn resolve(&self, spec: &PackageSpec) -> Result<(Manifest, ManifestOrigin)> {
        // An explicit reference still gets a curated manifest when one exists:
        // `ketch install BurntSushi/ripgrep` should link `rg`, not `ripgrep`.
        if let Some(reference) = &spec.reference {
            if let Some(found) = self.find(|m| same_source(&m.source, reference)) {
                return Ok(found);
            }
            return Ok((
                Manifest::inferred(reference.clone()),
                ManifestOrigin::Inferred,
            ));
        }

        let alias = spec
            .alias
            .as_deref()
            .map(normalize_name)
            .unwrap_or_default();
        self.find(|m| answers_to(m, &alias)).ok_or_else(|| {
            Error::msg(format!(
                "no package named `{alias}`; run `ketch update` to refresh the registry, \
                 `ketch search {alias}` to look on GitHub, or give an `owner/repo` reference"
            ))
        })
    }

    /// Every alias the registry knows, for completion and `ketch search`.
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
    pub fn aliases(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .manifests()
            .flat_map(|m| {
                std::iter::once(m.name.as_str()).chain(m.provides.iter().map(String::as_str))
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Known packages matching a free-text query, highest tier first. A name
    /// is listed once, from whichever tier would actually install it.
    pub fn search(&self, query: &str) -> Vec<&Manifest> {
        let needle = query.trim().to_ascii_lowercase();
        let mut seen = std::collections::HashSet::new();
        self.manifests()
            .filter(|m| needle.is_empty() || matches_query(m, &needle))
            .filter(|m| seen.insert(normalize_name(&m.name)))
            .collect()
    }

    /// What `ketch list remote` offers: the fetched registry and the user's own
    /// manifests, one per name, the user's winning as it would at install.
    ///
    /// The built-in tier is left out on purpose. It is a bootstrap copy of a
    /// few registry entries, and listing it would make a machine that never
    /// ran `ketch update` look as if it had a registry.
    pub fn listed(&self) -> Vec<&Manifest> {
        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<&Manifest> = self
            .user
            .iter()
            .chain(self.registry.iter())
            .map(|(m, _)| m)
            .filter(|m| seen.insert(normalize_name(&m.name)))
            .collect();
        out.sort_by_key(|m| normalize_name(&m.name));
        out
    }

    /// True when `ketch update` has not fetched a registry that holds anything.
    pub fn registry_is_empty(&self) -> bool {
        self.registry.is_empty()
    }

    /// Precedence order: user manifests shadow the registry, which shadows the
    /// built-ins. Everything that reads the tiers goes through this.
    fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.user
            .iter()
            .map(|(m, _)| m)
            .chain(self.registry.iter().map(|(m, _)| m))
            .chain(self.builtin.iter())
    }

    fn find(&self, pred: impl Fn(&Manifest) -> bool) -> Option<(Manifest, ManifestOrigin)> {
        if let Some((manifest, path)) = self.user.iter().find(|(m, _)| pred(m)) {
            return Some((manifest.clone(), ManifestOrigin::User(path.clone())));
        }
        if let Some((manifest, path)) = self.registry.iter().find(|(m, _)| pred(m)) {
            return Some((manifest.clone(), ManifestOrigin::Registry(path.clone())));
        }
        self.builtin
            .iter()
            .find(|m| pred(m))
            .map(|m| (m.clone(), ManifestOrigin::Builtin))
    }
}

fn matches_query(manifest: &Manifest, needle: &str) -> bool {
    manifest.name.to_ascii_lowercase().contains(needle)
        || manifest.source.id.to_ascii_lowercase().contains(needle)
        || manifest
            .provides
            .iter()
            .any(|p| p.to_ascii_lowercase().contains(needle))
        || manifest
            .description
            .as_deref()
            .is_some_and(|d| d.to_ascii_lowercase().contains(needle))
}

fn answers_to(manifest: &Manifest, alias: &str) -> bool {
    normalize_name(&manifest.name) == alias
        || manifest.provides.iter().any(|p| normalize_name(p) == alias)
}

/// Whether two references name the same package.
///
/// GitHub owners and repositories are case-insensitive, so
/// `burntsushi/ripgrep` is the repository the curated `BurntSushi/ripgrep`
/// entry describes: without this, a differently-cased reference resolves by
/// inference and quietly drops the curated `bin`, `provides` and asset rules.
/// Every other scheme is compared exactly — a `local:` path, on Linux, can
/// differ only by case and be a different file.
pub(crate) fn same_source(a: &PackageRef, b: &PackageRef) -> bool {
    if !a.scheme.eq_ignore_ascii_case(&b.scheme) {
        return false;
    }
    if a.scheme.eq_ignore_ascii_case("github") {
        a.id.eq_ignore_ascii_case(&b.id)
    } else {
        a.id == b.id
    }
}

/// Read every `.toml` in the manifest directory.
///
/// One unreadable file must not take down every command, so failures are
/// reported and skipped rather than propagated.
fn load_user_manifests(dir: &Path, report: &Report) -> Vec<(Manifest, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        let label = path.display().to_string();
        // Both error kinds already name the file, so the warning does not.
        match std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e)) {
            Ok(text) => match parse_registry(&text, &label) {
                Ok(manifests) => out.extend(manifests.into_iter().map(|m| (m, path.clone()))),
                Err(e) => report.warn(&format!("ignoring manifest: {e}")),
            },
            Err(e) => report.warn(&format!("ignoring manifest: {e}")),
        }
    }
    out
}

/// Where `ketch edit`/`ketch pin` should write a manifest for this package.
pub fn user_manifest_path(cfg: &Config, name: &str) -> PathBuf {
    cfg.manifest_dir
        .join(format!("{}.toml", crate::config::sanitize_component(name)))
}

/// Serialise a manifest for a user manifest file.
pub fn to_toml(manifest: &Manifest) -> Result<String> {
    crate::toml_file::render(manifest, "manifest")
}

/// Write `bin = [{ name = "<a>" }, { name = "<b>" }, …]` for `package` into
/// the user manifest at `path`, one entry per name in `bins`, changing nothing
/// else in the file. `false` when the package there already has a `bin`: a
/// choice is only ever written where none was made.
///
/// The file is the user's, so what they wrote — comments, order, spacing —
/// survives: `toml_file` changes the one key and renders the rest back as it
/// was read. The result is parsed as a manifest again before it replaces the
/// file, so a write that would leave it unloadable fails instead; and it
/// replaces the file by renaming a finished copy over it, so an interrupted
/// write leaves the old one whole.
pub fn write_bins(path: &Path, package: &str, bins: &[String]) -> Result<bool> {
    // A manifest linked in from a dotfiles repository is edited there, not
    // replaced by a copy that silently stops following the repository.
    let target = std::fs::canonicalize(path).map_err(|e| Error::io(path, e))?;
    let label = target.display().to_string();
    let text = std::fs::read_to_string(&target).map_err(|e| Error::io(&target, e))?;
    let wanted = normalize_name(package);
    let body = match crate::toml_file::insert_inline_list(
        &text,
        &label,
        |name| normalize_name(name) == wanted,
        "bin",
        "name",
        bins,
    )? {
        crate::toml_file::ListInsert::Inserted(body) => body,
        crate::toml_file::ListInsert::AlreadySet => return Ok(false),
        crate::toml_file::ListInsert::NoTable => {
            return Err(Error::msg(format!(
                "{label} has no `[[package]]` table named `{package}` to write `bin` into"
            )))
        }
    };
    parse_registry(&body, &label)?;
    replace_file(&target, &body)?;
    Ok(true)
}

/// Write a whole user manifest that `ketch import` generated, creating the
/// manifest directory on first use. The text is parsed as a manifest first,
/// so a converter bug fails here instead of leaving a file every later
/// command warns about; a file linked from elsewhere is written through.
pub fn write_manifest(path: &Path, text: &str) -> Result<()> {
    let target = match std::fs::canonicalize(path) {
        Ok(real) => real,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(e) => return Err(Error::io(path, e)),
    };
    parse_registry(text, &target.display().to_string())?;
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    }
    replace_file(&target, text)
}

/// Replace `target` by renaming a finished copy over it, so an interrupted
/// write leaves the old file whole.
fn replace_file(target: &Path, body: &str) -> Result<()> {
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| Error::io(dir, e))?;
    std::io::Write::write_all(&mut temp, body.as_bytes()).map_err(|e| Error::io(temp.path(), e))?;
    if let Ok(meta) = std::fs::metadata(target) {
        // A temporary file is created private; the manifest keeps the mode
        // the user gave it.
        let _ = std::fs::set_permissions(temp.path(), meta.permissions());
    }
    temp.persist(target)
        .map_err(|e| Error::io(target, e.error))?;
    Ok(())
}

/// `parse_registry` for the `manifest_toml` fuzz target (`src/lib.rs`).
#[cfg(fuzzing)]
pub(crate) fn fuzz_parse_registry(text: &str) -> Result<Vec<Manifest>> {
    parse_registry(text, "fuzz")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver() -> Resolver {
        Resolver {
            builtin: parse_registry(BUILTIN_TOML, "builtin").expect("builtin.toml must parse"),
            registry: Vec::new(),
            user: Vec::new(),
        }
    }

    #[test]
    fn builtin_registry_parses_and_is_reachable_by_alias() {
        let resolver = resolver();
        assert!(!resolver.builtin.is_empty());
        let (manifest, origin) = resolver
            .resolve(&PackageSpec::parse("rg"))
            .expect("`rg` is a declared alias of ripgrep");
        assert_eq!(manifest.name, "ripgrep");
        assert_eq!(origin, ManifestOrigin::Builtin);
    }

    #[test]
    fn a_registry_file_with_a_stray_top_level_key_is_refused() {
        let text = "oops = true\n\n[[package]]\nname = \"thing\"\nsource = \"github:o/thing\"\n";
        let err = parse_registry(text, "test").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("unknown field") && msg.contains("oops"),
            "{msg}"
        );
    }

    #[test]
    fn a_single_manifest_that_mentions_the_array_marker_is_still_a_manifest() {
        // Sniffing the source text for `[[package]]` parsed this as a registry
        // of zero packages and dropped the file without a word.
        let text = "name = \"thing\"\n\
                    source = \"github:o/thing\"\n\
                    notes = \"declare it under [[package]] to ship it\"\n";
        let found = parse_registry(text, "test").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "thing");
    }

    #[test]
    fn a_reference_still_picks_up_the_curated_manifest() {
        let (manifest, origin) = resolver()
            .resolve(&PackageSpec::parse("BurntSushi/ripgrep@14.1.0"))
            .unwrap();
        assert_eq!(origin, ManifestOrigin::Builtin);
        assert_eq!(
            manifest.bin.first().and_then(|b| b.name.as_deref()),
            Some("rg")
        );
    }

    #[test]
    fn a_reference_matches_the_curated_entry_whatever_its_case() {
        // GitHub is case-insensitive, so this is the same repository, and the
        // curated manifest is what says which binary to link.
        for reference in ["burntsushi/ripgrep", "BurntSushi/RipGrep"] {
            let (manifest, origin) = resolver().resolve(&PackageSpec::parse(reference)).unwrap();
            assert_eq!(origin, ManifestOrigin::Builtin, "{reference}");
            assert_eq!(
                manifest.bin.first().and_then(|b| b.name.as_deref()),
                Some("rg"),
                "{reference}"
            );
        }
    }

    #[test]
    fn an_uncurated_reference_falls_through_to_inference() {
        let (manifest, origin) = resolver()
            .resolve(&PackageSpec::parse("someone/whatever-tool"))
            .unwrap();
        assert_eq!(origin, ManifestOrigin::Inferred);
        assert_eq!(manifest.name, "whatever-tool");
        assert_eq!(manifest.source.id, "someone/whatever-tool");
    }

    #[test]
    fn an_unknown_bare_name_is_an_error_not_a_guess() {
        assert!(resolver()
            .resolve(&PackageSpec::parse("definitely-not-a-package"))
            .is_err());
    }

    #[test]
    fn the_registry_shadows_builtins_and_lists_each_name_once() {
        let entry = Manifest {
            provides: vec!["rg".into()],
            ..Manifest::inferred(crate::model::PackageRef::github("registry/ripgrep"))
        };
        let resolver = Resolver {
            builtin: parse_registry(BUILTIN_TOML, "builtin").unwrap(),
            registry: vec![(entry, PathBuf::from("/tmp/registry/ripgrep/ketch.toml"))],
            user: Vec::new(),
        };

        let (manifest, origin) = resolver.resolve(&PackageSpec::parse("rg")).unwrap();
        assert_eq!(manifest.source.id, "registry/ripgrep");
        assert!(matches!(origin, ManifestOrigin::Registry(_)));

        // The built-in `ripgrep` is the same package by another route, so it
        // must not show up as a second search result.
        let hits = resolver.search("ripgrep");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source.id, "registry/ripgrep");
    }

    #[test]
    fn user_manifests_shadow_builtins() {
        let mine = Manifest {
            provides: vec!["rg".into()],
            ..Manifest::inferred(crate::model::PackageRef::github("me/my-ripgrep"))
        };
        let resolver = Resolver {
            builtin: parse_registry(BUILTIN_TOML, "builtin").unwrap(),
            registry: Vec::new(),
            user: vec![(mine, PathBuf::from("/tmp/my-ripgrep.toml"))],
        };
        let (manifest, origin) = resolver.resolve(&PackageSpec::parse("rg")).unwrap();
        assert_eq!(manifest.source.id, "me/my-ripgrep");
        assert!(matches!(origin, ManifestOrigin::User(_)));
    }

    fn user_file(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rtok.toml");
        std::fs::write(&path, body).unwrap();
        (dir, path)
    }

    #[test]
    fn write_bins_adds_the_entry_and_keeps_every_other_byte() {
        let body = concat!(
            "# my rtok, pinned to the fork\n",
            "name   = \"rtok\"   # spaced on purpose\n",
            "source = \"github:me/rtok\"\n",
            "\n",
            "[asset]\n",
            "include = [\"*.tar.xz\"]\n",
        );
        let (_dir, path) = user_file(body);

        assert!(write_bins(&path, "rtok", &one("rtok-cli")).unwrap());

        let written = std::fs::read_to_string(&path).unwrap();
        let expected = concat!(
            "# my rtok, pinned to the fork\n",
            "name   = \"rtok\"   # spaced on purpose\n",
            "source = \"github:me/rtok\"\n",
            "bin = [{ name = \"rtok-cli\" }]\n",
            "\n",
            "[asset]\n",
            "include = [\"*.tar.xz\"]\n",
        );
        pretty_assertions::assert_eq!(written, expected);
        let parsed = parse_registry(&written, "test").unwrap();
        assert_eq!(parsed[0].bin[0].name.as_deref(), Some("rtok-cli"));
    }

    fn one(bin: &str) -> Vec<String> {
        vec![bin.to_string()]
    }

    #[test]
    fn write_bins_writes_one_entry_per_name_in_order() {
        let (_dir, path) = user_file("name = \"rtok\"\nsource = \"github:me/rtok\"\n");
        let bins = vec!["rtok".to_string(), "other-tool".to_string()];
        assert!(write_bins(&path, "rtok", &bins).unwrap());
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.ends_with("bin = [{ name = \"rtok\" }, { name = \"other-tool\" }]\n"),
            "{written}"
        );
    }

    #[test]
    fn write_bins_leaves_a_manifest_that_already_names_one_alone() {
        let body = "name = \"rtok\"\nsource = \"github:me/rtok\"\nbin = [{ name = \"rtok\" }]\n";
        let (_dir, path) = user_file(body);
        assert!(!write_bins(&path, "rtok", &one("rtok-cli")).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    }

    #[test]
    fn write_bins_finds_the_package_in_a_multi_package_file() {
        let body = concat!(
            "[[package]]\n",
            "name = \"other\"\n",
            "source = \"github:me/other\"\n",
            "\n",
            "[[package]]\n",
            "name = \"rtok\"\n",
            "source = \"github:me/rtok\"\n",
        );
        let (_dir, path) = user_file(body);
        assert!(write_bins(&path, "rtok", &one("rtok-cli")).unwrap());
        let parsed = parse_registry(&std::fs::read_to_string(&path).unwrap(), "t").unwrap();
        assert!(parsed[0].bin.is_empty());
        assert_eq!(parsed[1].bin[0].name.as_deref(), Some("rtok-cli"));
    }

    #[cfg(unix)]
    #[test]
    fn write_bins_edits_the_target_of_a_symlinked_manifest() {
        let (dir, target) = user_file("name = \"rtok\"\nsource = \"github:me/rtok\"\n");
        let link = dir.path().join("link.toml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(write_bins(&link, "rtok", &one("rtok-cli")).unwrap());
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::read_to_string(&target)
            .unwrap()
            .contains("rtok-cli"));
    }

    #[test]
    fn write_manifest_creates_the_directory_and_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifests").join("fly.toml");
        let body = "name = \"fly\"\nsource = \"github:superfly/flyctl\"\n";
        write_manifest(&path, body).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    }

    #[test]
    fn write_manifest_refuses_text_that_is_not_a_manifest_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fly.toml");
        assert!(write_manifest(&path, "name = [\n").is_err());
        assert!(!path.exists());
    }

    #[test]
    fn write_manifest_replaces_an_older_copy_whole() {
        let (_dir, path) = user_file("name = \"rtok\"\nsource = \"github:me/rtok\"\n");
        let body = "name = \"rtok\"\nsource = \"github:me/rtok-fork\"\n";
        write_manifest(&path, body).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), body);
    }
}
