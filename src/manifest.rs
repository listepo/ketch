//! Turning what the user typed into a `Manifest`.
//!
//! Four tiers, in order: a user manifest in `~/.ketch/manifests/<name>.toml`,
//! the fetched package registry, the built-in registry compiled into the
//! binary, then inference from the source reference itself. Inference is what
//! lets `ketch install owner/repo` work for a repository nobody has curated.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::{normalize_name, Manifest, ManifestOrigin, PackageSpec};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The built-in registry: curated manifests for tools whose release layout
/// needs a hint that inference cannot guess.
pub const BUILTIN_TOML: &str = include_str!("builtin.toml");

/// A file holding several manifests, as `builtin.toml` does.
#[derive(Debug, Deserialize)]
struct Registry {
    #[serde(default)]
    package: Vec<Manifest>,
}

fn parse_registry(text: &str, what: &str) -> Result<Vec<Manifest>> {
    let value: toml::Value = toml::from_str(text).map_err(|e| Error::parse(what, e.to_string()))?;
    // One file may hold either a single manifest or a `[[package]]` array.
    // Which one is decided from the parsed shape, not from the source text: a
    // single manifest that merely mentions `[[package]]` — in a description, in
    // a note — is still a manifest, and sniffing for the string parsed it as an
    // array instead, which `#[serde(default)]` then turned into no packages at
    // all. The whole file disappeared without a word.
    let manifests = if value.get("package").is_some_and(toml::Value::is_array) {
        Registry::deserialize(value)
            .map_err(|e| Error::parse(what, e.to_string()))?
            .package
    } else {
        vec![Manifest::deserialize(value).map_err(|e| Error::parse(what, e.to_string()))?]
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
    pub fn new(cfg: &Config) -> Result<Self> {
        // A malformed built-in registry is a bug in ketch, not in the user's
        // setup, so it fails loudly rather than degrading to inference.
        let builtin = parse_registry(BUILTIN_TOML, "the built-in registry")?;
        Ok(Resolver {
            user: load_user_manifests(&cfg.manifest_dir),
            registry: crate::registry::load(cfg),
            builtin,
        })
    }

    /// Resolve a spec, reporting where the manifest came from.
    pub fn resolve(&self, spec: &PackageSpec) -> Result<(Manifest, ManifestOrigin)> {
        // An explicit reference still gets a curated manifest when one exists:
        // `ketch install BurntSushi/ripgrep` should link `rg`, not `ripgrep`.
        if let Some(reference) = &spec.reference {
            if let Some(found) = self.find(|m| &m.source == reference) {
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

/// Read every `.toml` in the manifest directory.
///
/// One unreadable file must not take down every command, so failures are
/// reported and skipped rather than propagated.
fn load_user_manifests(dir: &Path) -> Vec<(Manifest, PathBuf)> {
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
                Err(e) => crate::ui::warn(&format!("ignoring manifest: {e}")),
            },
            Err(e) => crate::ui::warn(&format!("ignoring manifest: {e}")),
        }
    }
    out
}

/// Where `ketch edit`/`ketch pin` should write a manifest for this package.
// Part of the public surface, with no caller in the tree yet.
#[allow(dead_code)]
pub fn user_manifest_path(cfg: &Config, name: &str) -> PathBuf {
    cfg.manifest_dir
        .join(format!("{}.toml", crate::config::sanitize_component(name)))
}

/// Serialise a manifest for a user manifest file.
// Part of the public surface, with no caller in the tree yet.
#[allow(dead_code)]
pub fn to_toml(manifest: &Manifest) -> Result<String> {
    toml::to_string_pretty(manifest).map_err(|e| Error::parse("manifest", e.to_string()))
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
}
