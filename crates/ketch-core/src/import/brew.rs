// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Homebrew: a formula or a cask, from `formulae.brew.sh`'s JSON API.
//!
//! A cask downloads what the project published, so a cask whose `url` is a
//! GitHub release asset converts. A formula is a build recipe: its
//! `urls.stable` is the source it compiles, and the binaries Homebrew ships
//! are bottles on `ghcr.io`. So a formula converts only in the rare case
//! that its stable URL is a release asset naming a platform, and is "not
//! distributed through GitHub Releases" otherwise — which, for homebrew-core,
//! is every formula there is.

use super::{
    assemble, bin_entry, check_name, names_a_platform, release_url, targets_named, Backend,
    Conversion, Download, Endpoints, Fetch, Found, Rejected,
};
use crate::error::{Error, Result};
use crate::model::{Arch, BinSpec, Os, PackageKind, TargetSpec};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

/// Which half of Homebrew to look in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pick {
    /// Both; a name that is both is settled by which one converts.
    #[default]
    Either,
    Formula,
    Cask,
}

#[derive(Debug, Deserialize)]
struct Cask {
    token: String,
    version: String,
    url: String,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    artifacts: Vec<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    variations: Ordered<Variation>,
}

#[derive(Debug, Deserialize)]
struct Variation {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Formula {
    name: String,
    versions: FormulaVersions,
    urls: FormulaUrls,
}

#[derive(Debug, Deserialize)]
struct FormulaVersions {
    stable: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FormulaUrls {
    stable: Option<FormulaUrl>,
}

#[derive(Debug, Deserialize)]
struct FormulaUrl {
    url: String,
    #[serde(default)]
    checksum: Option<String>,
}

/// A JSON object kept in the order it was written. A cask's `variations`
/// list macOS releases newest first, and the newest Intel one is the one
/// that matters; a sorted map would hand over `big_sur` instead.
#[derive(Debug)]
struct Ordered<V>(Vec<(String, V)>);

impl<V> Default for Ordered<V> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Ordered<V> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct V2<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for V2<V> {
            type Value = Ordered<V>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = Vec::new();
                while let Some((k, v)) = map.next_entry::<String, V>()? {
                    out.push((k, v));
                }
                Ok(Ordered(out))
            }
            fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
                Ok(Ordered(Vec::new()))
            }
        }
        deserializer.deserialize_any(V2(std::marker::PhantomData))
    }
}

/// Cask artifacts that only say how Homebrew cleans up or what it prints:
/// ketch has its own uninstall, and nothing here changes what is installed.
const IGNORED_ARTIFACTS: &[&str] = &[
    "zap",
    "uninstall",
    "caveats",
    "bash_completion",
    "zsh_completion",
    "fish_completion",
    "manpage",
    "generate_completions_from_executable",
];

/// Convert a cask's API JSON.
pub fn convert_cask(json: &str) -> Result<Conversion> {
    let cask: Cask = serde_json::from_str(json)
        .map_err(|e| Error::parse("Homebrew cask JSON", e.to_string()))?;
    let checksum = |s: &Option<String>| s.clone().filter(|s| s != "no_check");

    // The top level is the arm64 download; the first variation without an
    // `arm64_` prefix is the newest Intel macOS. Linux variations are not
    // macOS downloads at all.
    let arm = (cask.url.clone(), checksum(&cask.sha256));
    let intel = cask
        .variations
        .0
        .iter()
        .find(|(key, v)| !key.starts_with("arm64_") && !key.contains("linux") && v.url.is_some())
        .and_then(|(_, v)| v.url.clone().map(|u| (u, checksum(&v.sha256))))
        .unwrap_or_else(|| arm.clone());
    let downloads = vec![
        Download {
            target: TargetSpec {
                os: Os::MacOs,
                arch: Arch::Aarch64,
            },
            url: arm.0,
            sha256: arm.1,
        },
        Download {
            target: TargetSpec {
                os: Os::MacOs,
                arch: Arch::X86_64,
            },
            url: intel.0,
            sha256: intel.1,
        },
    ];

    let (kind, bins, refused) = cask_payload(&cask.artifacts);
    let converted = match assemble(
        Backend::Brew,
        &cask.token,
        &cask.token,
        &cask.version,
        kind,
        bins,
        downloads,
    ) {
        Ok(c) => c,
        Err(rejected) => return Ok(Err(rejected)),
    };
    if let Some(why) = refused {
        return Ok(Err(Rejected::Unsupported(why)));
    }
    Ok(Ok(converted))
}

/// What a cask installs, as ketch would: an app, or binaries. The third
/// value is why it cannot, when it cannot.
fn cask_payload(
    artifacts: &[serde_json::Map<String, serde_json::Value>],
) -> (PackageKind, Vec<BinSpec>, Option<String>) {
    let mut app = false;
    let mut bins: Vec<BinSpec> = Vec::new();
    for artifact in artifacts {
        let Some(key) = artifact.keys().find(|k| k.as_str() != "target") else {
            continue;
        };
        match key.as_str() {
            "app" => app = true,
            "binary" => {
                let source = artifact
                    .get("binary")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                // Binaries inside the bundle come with the app; ketch places
                // the bundle whole.
                if source.starts_with("$APPDIR") {
                    continue;
                }
                let command = artifact
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or(source);
                let command = command.rsplit('/').next().unwrap_or(command);
                if let Some(entry) = bin_entry(command, Some(source)) {
                    bins.push(entry);
                }
            }
            other if IGNORED_ARTIFACTS.contains(&other) => {}
            other => {
                return (
                    PackageKind::Auto,
                    Vec::new(),
                    Some(format!(
                        "it installs with a `{other}` artifact, which ketch can't place yet"
                    )),
                )
            }
        }
    }
    if app {
        // An app's command-line helpers stay inside the bundle: `kind = "app"`
        // places the bundle and links nothing (docs/MANIFESTS.md).
        return (PackageKind::App, Vec::new(), None);
    }
    if bins.is_empty() {
        return (
            PackageKind::Auto,
            Vec::new(),
            Some("it has no app and no binary for ketch to install".into()),
        );
    }
    (PackageKind::Auto, bins, None)
}

/// Convert a formula's API JSON.
pub fn convert_formula(json: &str) -> Result<Conversion> {
    let formula: Formula = serde_json::from_str(json)
        .map_err(|e| Error::parse("Homebrew formula JSON", e.to_string()))?;
    let Some(stable) = formula.urls.stable else {
        return Ok(Err(Rejected::NotGithubReleases));
    };
    let Some(release) = release_url(&stable.url) else {
        return Ok(Err(Rejected::NotGithubReleases));
    };
    // A release asset that names no platform is the project's source
    // tarball, attached to the release for convenience: still source only.
    if !names_a_platform(&release.file) {
        return Ok(Err(Rejected::NotGithubReleases));
    }
    let downloads: Vec<Download> = targets_named(&release.file)
        .into_iter()
        .map(|target| Download {
            target,
            url: stable.url.clone(),
            sha256: stable.checksum.clone(),
        })
        .collect();
    let bins = bin_entry(&formula.name, None).into_iter().collect();
    Ok(assemble(
        Backend::Brew,
        &formula.name,
        &formula.name,
        formula.versions.stable.as_deref().unwrap_or_default(),
        PackageKind::Auto,
        bins,
        downloads,
    ))
}

/// Look `name` up and convert it.
pub fn lookup(fetch: &dyn Fetch, endpoints: &Endpoints, name: &str, pick: Pick) -> Result<Found> {
    let name = check_name(name)?;
    let formula = match pick {
        Pick::Cask => None,
        _ => fetch.text(&format!("{}/formula/{name}.json", endpoints.brew))?,
    };
    let cask = match pick {
        Pick::Formula => None,
        _ => fetch.text(&format!("{}/cask/{name}.json", endpoints.brew))?,
    };
    let formula = formula.map(|j| convert_formula(&j)).transpose()?;
    let cask = cask.map(|j| convert_cask(&j)).transpose()?;
    match (formula, cask) {
        (None, None) => Err(Error::msg(match pick {
            Pick::Either => format!("Homebrew has no formula or cask named `{name}`"),
            Pick::Formula => format!("Homebrew has no formula named `{name}`"),
            Pick::Cask => format!("Homebrew has no cask named `{name}`"),
        })),
        (Some(verdict), None) | (None, Some(verdict)) => {
            verdict.map(Found::from).map_err(|r| r.into_error(name))
        }
        (Some(Ok(_)), Some(Ok(_))) => Err(Error::msg(format!(
            "`{name}` is both a Homebrew formula and a cask; pass --formula or --cask"
        ))),
        (Some(Ok(formula)), Some(Err(_))) => Ok(Found {
            converted: formula,
            notes: vec![format!(
                "`{name}` is also a cask, which does not convert; using the formula"
            )],
        }),
        (Some(Err(_)), Some(Ok(cask))) => Ok(Found {
            converted: cask,
            notes: vec![format!(
                "`{name}` is also a formula, which builds from source; using the cask"
            )],
        }),
        // Homebrew's own default for a bare name is the formula, so its
        // reason is the one given.
        (Some(Err(rejected)), Some(Err(_))) => Err(rejected.into_error(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Recorded;
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/src/import/fixtures/brew/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn cask(name: &str) -> Conversion {
        convert_cask(&fixture(&format!("cask-{name}.json"))).unwrap()
    }

    fn formula(name: &str) -> Conversion {
        convert_formula(&fixture(&format!("formula-{name}.json"))).unwrap()
    }

    #[test]
    fn a_binary_cask_converts_to_two_pinned_targets_and_one_command() {
        let c = cask("fly").unwrap();
        assert_eq!(c.repo, "concourse/concourse");
        assert_eq!(c.tag.as_deref(), Some("v8.3.0"));
        assert_eq!(c.kind, PackageKind::Auto);
        assert_eq!(c.bins.len(), 1);
        assert_eq!(c.bins[0].name.as_deref(), Some("fly"));
        assert_eq!(c.bins[0].path, None);
        assert_eq!(
            c.artifacts["macos-aarch64"].file,
            "fly-8.3.0-darwin-arm64.tgz"
        );
        assert_eq!(
            c.artifacts["macos-x86_64"].file,
            "fly-8.3.0-darwin-amd64.tgz"
        );
        assert_eq!(
            c.artifacts["macos-aarch64"].sha256.as_deref(),
            Some("69a33b6f3dfa9c59af96442d1bd1fe054734305e5544bd2b7dd17641234dc4cb")
        );
    }

    #[test]
    fn the_cask_golden_holds_only_what_ketch_needs() {
        let body = cask("fly").unwrap().render().unwrap();
        insta::assert_snapshot!(body, @r#"
        # Written by `ketch import brew fly`. Schema: docs/MANIFESTS.md.
        name = "fly"
        source = "github:concourse/concourse"

        bin = [{ name = "fly" }]

        [asset.target]
        "macos-aarch64" = "fly-*-darwin-arm64.tgz"
        "macos-x86_64" = "fly-*-darwin-amd64.tgz"
        "#);
    }

    #[test]
    fn a_binary_in_a_subdirectory_keeps_its_name_and_no_path() {
        let c = cask("codex").unwrap();
        assert_eq!(c.bins[0].name.as_deref(), Some("codex"));
        assert_eq!(c.bins[0].path, None, "bin/codex is found by its name");
        assert_eq!(
            c.render().unwrap().lines().last(),
            Some("\"macos-x86_64\" = \"codex-package-x86_64-apple-darwin.tar.gz\"")
        );
    }

    #[test]
    fn an_app_cask_becomes_an_app_with_no_bin() {
        let c = cask("anki").unwrap();
        assert_eq!(c.kind, PackageKind::App);
        assert!(c.bins.is_empty());
        insta::assert_snapshot!(c.render().unwrap(), @r#"
        # Written by `ketch import brew anki`. Schema: docs/MANIFESTS.md.
        name = "anki"
        source = "github:ankitects/anki"
        kind = "app"

        [asset.target]
        "macos-aarch64" = "anki-*-mac-apple.dmg"
        "macos-x86_64" = "anki-*-mac-intel.dmg"
        "#);
    }

    #[test]
    fn an_app_with_helpers_inside_the_bundle_is_still_just_the_app() {
        let c = cask("wezterm").unwrap();
        assert_eq!(c.kind, PackageKind::App);
        assert!(c.bins.is_empty());
        // No variations: one universal download serves both architectures.
        assert_eq!(
            c.artifacts["macos-aarch64"], c.artifacts["macos-x86_64"],
            "one file for both"
        );
        assert_eq!(
            c.manifest().unwrap().asset.target["macos-x86_64"],
            "WezTerm-macos-*.zip"
        );
    }

    #[test]
    fn a_vendor_cdn_cask_is_refused_by_the_hard_rule() {
        assert_eq!(cask("google-chrome"), Err(Rejected::NotGithubReleases));
    }

    #[test]
    fn a_pkg_cask_on_github_is_refused_for_its_installer() {
        let Err(Rejected::Unsupported(why)) = cask("background-music") else {
            panic!("background-music should not convert")
        };
        assert!(why.contains("`pkg`"), "{why}");
    }

    #[test]
    fn a_cask_without_a_checksum_still_converts_with_none_recorded() {
        let c = cask("hackintool").unwrap();
        assert_eq!(
            c.artifacts["macos-aarch64"].sha256, None,
            "sha256 is no_check"
        );
        assert_eq!(c.artifacts["macos-aarch64"].file, "Hackintool.zip");
    }

    #[rstest::rstest]
    #[case::release_source_tarball("jq")]
    #[case::archive_tarball("ripgrep")]
    #[case::name_with_a_platform_word("universal-ctags")]
    fn a_formula_that_builds_from_source_is_refused(#[case] name: &str) {
        assert_eq!(formula(name), Err(Rejected::NotGithubReleases));
    }

    const PREBUILT_FORMULA: &str = r#"{
        "name": "tool",
        "versions": { "stable": "2.0.1" },
        "urls": { "stable": {
            "url": "https://github.com/o/tool/releases/download/v2.0.1/tool-2.0.1-aarch64-apple-darwin.tar.gz",
            "checksum": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        } }
    }"#;

    #[test]
    fn a_formula_whose_url_is_a_prebuilt_asset_converts_for_that_target() {
        let c = convert_formula(PREBUILT_FORMULA).unwrap().unwrap();
        assert_eq!(c.repo, "o/tool");
        assert_eq!(
            c.artifacts.keys().collect::<Vec<_>>(),
            vec!["macos-aarch64"]
        );
        assert_eq!(
            c.artifacts["macos-aarch64"].sha256.as_deref(),
            Some(&"a".repeat(64)[..])
        );
        assert_eq!(c.bins[0].name.as_deref(), Some("tool"));
    }

    fn brew() -> (Recorded, Endpoints) {
        let e = Endpoints {
            brew: "https://brew.test/api".into(),
            ..Endpoints::default()
        };
        (Recorded::default(), e)
    }

    #[test]
    fn an_unknown_name_says_so_and_names_both_halves() {
        let (f, e) = brew();
        let err = lookup(&f, &e, "nope", Pick::Either).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Homebrew has no formula or cask named `nope`"
        );
        let err = lookup(&f, &e, "nope", Pick::Cask).unwrap_err();
        assert_eq!(err.to_string(), "Homebrew has no cask named `nope`");
    }

    #[test]
    fn a_name_that_is_both_takes_the_half_that_converts() {
        let (f, e) = brew();
        let f = f
            .with(
                "https://brew.test/api/formula/fly.json",
                fixture("formula-jq.json"),
            )
            .with(
                "https://brew.test/api/cask/fly.json",
                fixture("cask-fly.json"),
            );
        let found = lookup(&f, &e, "fly", Pick::Either).unwrap();
        assert_eq!(found.converted.repo, "concourse/concourse");
        assert_eq!(found.notes.len(), 1);
        assert!(
            found.notes[0].contains("using the cask"),
            "{:?}",
            found.notes
        );
    }

    #[test]
    fn a_name_that_is_both_and_converts_both_ways_must_be_picked() {
        let (f, e) = brew();
        let f = f
            .with("https://brew.test/api/formula/tool.json", PREBUILT_FORMULA)
            .with(
                "https://brew.test/api/cask/tool.json",
                fixture("cask-fly.json"),
            );
        let err = lookup(&f, &e, "tool", Pick::Either).unwrap_err();
        assert!(err.to_string().contains("--formula or --cask"), "{err}");
        let found = lookup(&f, &e, "tool", Pick::Formula).unwrap();
        assert_eq!(found.converted.repo, "o/tool");
        let found = lookup(&f, &e, "tool", Pick::Cask).unwrap();
        assert_eq!(found.converted.repo, "concourse/concourse");
    }

    #[test]
    fn a_formula_only_name_is_refused_with_the_hard_rule() {
        let (f, e) = brew();
        let f = f.with(
            "https://brew.test/api/formula/jq.json",
            fixture("formula-jq.json"),
        );
        let err = lookup(&f, &e, "jq", Pick::Either).unwrap_err();
        assert_eq!(err.to_string(), crate::import::not_github_message("jq"));
    }

    #[test]
    fn a_hostile_name_never_reaches_a_url() {
        let (f, e) = brew();
        assert!(lookup(&f, &e, "../../x", Pick::Either).is_err());
    }
}
