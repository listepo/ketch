// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! winget: a package from the community repository, `microsoft/winget-pkgs`.
//!
//! The community source has no public REST endpoint — the REST protocol is
//! for private sources and the Store — so the newest version folder is found
//! with the GitHub contents API and its installer manifest read raw. The
//! manifest is YAML, in the multi-file form (`<Id>.installer.yaml`) or the
//! older singleton (`<Id>.yaml`); both carry the same `Installers` list.
//!
//! winget runs installers; ketch unpacks. So only the two installer types
//! that are a file to place convert: `portable` (a bare executable) and
//! `zip` with a `portable` nested installer.

use super::{
    assemble, bin_entry, check_name, Backend, Conversion, Download, Endpoints, Fetch, Found,
    Rejected,
};
use crate::error::{Error, Result};
use crate::model::{Arch, BinSpec, Os, PackageKind, TargetSpec};
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "PascalCase", default)]
struct Common {
    installer_type: Option<String>,
    nested_installer_type: Option<String>,
    nested_installer_files: Option<Vec<NestedFile>>,
    commands: Option<Vec<String>>,
    scope: Option<String>,
    installer_locale: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
struct NestedFile {
    relative_file_path: String,
    #[serde(default)]
    portable_command_alias: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InstallerManifest {
    package_identifier: String,
    // serde-saphyr hands a plain scalar to a `String` as written, so
    // `PackageVersion: 2.0` stays "2.0" instead of becoming the float 2.
    package_version: String,
    #[serde(flatten)]
    common: Common,
    #[serde(default)]
    installers: Vec<Installer>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Installer {
    architecture: String,
    installer_url: String,
    #[serde(default)]
    installer_sha256: Option<String>,
    #[serde(flatten)]
    common: Common,
}

/// An installer with the root's defaults filled in, as winget reads it.
struct Effective<'a> {
    installer: &'a Installer,
    kind: String,
    nested: Option<String>,
    files: Vec<NestedFile>,
    commands: Vec<String>,
    scope: Option<String>,
    locale: Option<String>,
}

impl Effective<'_> {
    /// Whether this is a file ketch can place rather than a program to run.
    fn placeable(&self) -> bool {
        match self.kind.as_str() {
            "portable" => true,
            "zip" => self.nested.as_deref() == Some("portable"),
            _ => false,
        }
    }

    /// `zip/msi` for a zip around an installer, the type otherwise.
    fn describe(&self) -> String {
        match (&*self.kind, &self.nested) {
            ("zip", Some(nested)) => format!("zip/{nested}"),
            (kind, _) => kind.to_string(),
        }
    }

    /// The order winget would offer: no locale or English first, then the
    /// per-user install over the machine-wide one. Lower is better.
    fn rank(&self) -> (u8, u8) {
        let locale = match self.locale.as_deref() {
            None => 0,
            Some(l) if l.eq_ignore_ascii_case("en-US") => 0,
            Some(l) if l.to_ascii_lowercase().starts_with("en") => 1,
            Some(_) => 2,
        };
        let scope = match self.scope.as_deref() {
            Some("user") => 0,
            None => 1,
            Some(_) => 2,
        };
        (locale, scope)
    }
}

fn effective<'a>(root: &Common, installer: &'a Installer) -> Effective<'a> {
    let own = &installer.common;
    let lower = |s: &Option<String>| s.as_ref().map(|t| t.trim().to_ascii_lowercase());
    Effective {
        installer,
        kind: lower(&own.installer_type)
            .or_else(|| lower(&root.installer_type))
            .unwrap_or_default(),
        nested: lower(&own.nested_installer_type).or_else(|| lower(&root.nested_installer_type)),
        files: own
            .nested_installer_files
            .clone()
            .or_else(|| root.nested_installer_files.clone())
            .unwrap_or_default(),
        commands: own
            .commands
            .clone()
            .or_else(|| root.commands.clone())
            .unwrap_or_default(),
        scope: lower(&own.scope).or_else(|| lower(&root.scope)),
        locale: own
            .installer_locale
            .clone()
            .or_else(|| root.installer_locale.clone()),
    }
}

/// The targets a winget architecture covers. `x86` and 32-bit `arm` have no
/// ketch target; `neutral` runs anywhere Windows does.
fn targets(architecture: &str) -> Vec<TargetSpec> {
    let windows = |arch| TargetSpec {
        os: Os::Windows,
        arch,
    };
    match architecture.trim().to_ascii_lowercase().as_str() {
        "x64" => vec![windows(Arch::X86_64)],
        "arm64" => vec![windows(Arch::Aarch64)],
        "neutral" => vec![windows(Arch::X86_64), windows(Arch::Aarch64)],
        _ => Vec::new(),
    }
}

/// Convert an installer manifest (multi-file or singleton YAML).
pub fn convert(yaml: &str) -> Result<Conversion> {
    let manifest: InstallerManifest = serde_saphyr::from_str(yaml)
        .map_err(|e| Error::parse("winget installer manifest", e.to_string()))?;
    let id = manifest.package_identifier.trim().to_string();
    let version = manifest.package_version.clone();

    let mut by_target: BTreeMap<String, (TargetSpec, Vec<Effective<'_>>)> = BTreeMap::new();
    for installer in &manifest.installers {
        for target in targets(&installer.architecture) {
            by_target
                .entry(target.to_string())
                .or_insert_with(|| (target, Vec::new()))
                .1
                .push(effective(&manifest.common, installer));
        }
    }

    let mut downloads = Vec::new();
    let mut refused: Option<String> = None;
    let mut chosen_all: Vec<&Effective<'_>> = Vec::new();
    for (target, candidates) in by_target.values() {
        let placeable: Vec<&Effective<'_>> = candidates.iter().filter(|e| e.placeable()).collect();
        let pool: Vec<&Effective<'_>> = if placeable.is_empty() {
            if refused.is_none() {
                refused = candidates.first().map(|e| {
                    format!(
                        "it installs with a winget `{}` installer, which ketch can't run",
                        e.describe()
                    )
                });
            }
            candidates.iter().collect()
        } else {
            let best = placeable.iter().map(|e| e.rank()).min().unwrap_or_default();
            placeable.into_iter().filter(|e| e.rank() == best).collect()
        };
        let mut seen: Vec<&str> = Vec::new();
        for e in pool {
            if seen.contains(&e.installer.installer_url.as_str()) {
                continue;
            }
            seen.push(&e.installer.installer_url);
            downloads.push(Download {
                target: *target,
                url: e.installer.installer_url.clone(),
                sha256: e.installer.installer_sha256.clone(),
            });
            chosen_all.push(e);
        }
    }

    let name = id.split('.').nth(1).unwrap_or(&id).to_string();
    let bins = chosen_all
        .first()
        .map(|e| bins_of(e, &name))
        .unwrap_or_default();
    let converted = match assemble(
        Backend::Winget,
        &id,
        &name,
        &version,
        PackageKind::Auto,
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

/// The commands winget would put on `PATH` for this installer.
fn bins_of(e: &Effective<'_>, name: &str) -> Vec<BinSpec> {
    if e.kind == "zip" {
        return e
            .files
            .iter()
            .filter_map(|f| {
                let file = f.relative_file_path.replace('\\', "/");
                let stem = file.rsplit('/').next().unwrap_or(&file).to_string();
                let command = f.portable_command_alias.clone().unwrap_or(stem);
                bin_entry(&command, Some(&file))
            })
            .collect();
    }
    let command = e.commands.first().map(String::as_str).unwrap_or(name);
    bin_entry(command, None).into_iter().collect()
}

#[derive(Debug, Deserialize)]
struct Entry {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

/// winget's own ordering of version folder names: dotted parts compared as
/// numbers when both are, as text otherwise, a missing part counting as 0.
fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts =
        |s: &str| -> Vec<String> { s.split(['.', '-']).map(str::to_string).collect::<Vec<_>>() };
    let (pa, pb) = (parts(a), parts(b));
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).map(String::as_str).unwrap_or("0");
        let y = pb.get(i).map(String::as_str).unwrap_or("0");
        let order = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(m), Ok(n)) => m.cmp(&n),
            _ => x.cmp(y),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

/// Look a package identifier up and convert its newest version.
pub fn lookup(fetch: &dyn Fetch, endpoints: &Endpoints, id: &str) -> Result<Found> {
    let id = check_name(id)?;
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 2 || segments.iter().any(|s| s.is_empty()) {
        return Err(Error::msg(format!(
            "`{id}` is not a winget package identifier: expected `Publisher.Package`"
        )));
    }
    let letter = id[..1].to_ascii_lowercase();
    let dir = format!("{letter}/{}", segments.join("/"));
    let missing = || {
        Error::msg(format!(
            "winget has no package `{id}` (identifiers are case-sensitive: `winget search` shows the exact one)"
        ))
    };
    let listing = fetch
        .text(&format!("{}/{dir}", endpoints.winget_api))?
        .ok_or_else(missing)?;
    let entries: Vec<Entry> = serde_json::from_str(&listing)
        .map_err(|e| Error::parse("winget-pkgs folder listing", e.to_string()))?;
    // A package folder holds its version folders and, sometimes, the folders
    // of packages named under it (`Google.Chrome.Canary`), which start with
    // a letter.
    let version = entries
        .iter()
        .filter(|e| e.kind == "dir" && e.name.starts_with(|c: char| c.is_ascii_digit()))
        .map(|e| e.name.as_str())
        .max_by(|a, b| compare_versions(a, b))
        .ok_or_else(missing)?;
    let base = format!("{}/{dir}/{version}", endpoints.winget_raw);
    let yaml = match fetch.text(&format!("{base}/{id}.installer.yaml"))? {
        Some(yaml) => yaml,
        None => fetch.text(&format!("{base}/{id}.yaml"))?.ok_or_else(|| {
            Error::msg(format!(
                "winget-pkgs has `{id}` {version} but no installer manifest in it"
            ))
        })?,
    };
    convert(&yaml)?
        .map(Found::from)
        .map_err(|rejected| rejected.into_error(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Recorded;
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/src/import/fixtures/winget/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    fn convert_fixture(id: &str) -> Conversion {
        convert(&fixture(&format!("{id}.installer.yaml"))).unwrap()
    }

    #[test]
    fn a_portable_zip_converts_per_architecture_and_skips_x86() {
        let c = convert_fixture("BurntSushi.ripgrep.MSVC").unwrap();
        assert_eq!(c.name, "ripgrep");
        assert_eq!(c.repo, "BurntSushi/ripgrep");
        assert_eq!(c.tag.as_deref(), Some("15.2.0"));
        assert_eq!(
            c.artifacts.keys().collect::<Vec<_>>(),
            vec!["windows-aarch64", "windows-x86_64"]
        );
        assert_eq!(
            c.artifacts["windows-x86_64"].sha256.as_deref(),
            Some("71b2fef860abe467217a538ff31de02f5258807c0129f771846f87bd029aafc5"),
            "lowercased"
        );
        insta::assert_snapshot!(c.render().unwrap(), @r#"
        # Written by `ketch import winget BurntSushi.ripgrep.MSVC`. Schema: docs/MANIFESTS.md.
        name = "ripgrep"
        source = "github:BurntSushi/ripgrep"

        bin = [{ name = "rg" }]

        [asset.target]
        "windows-aarch64" = "ripgrep-*-aarch64-pc-windows-msvc.zip"
        "windows-x86_64" = "ripgrep-*-x86_64-pc-windows-msvc.zip"
        "#);
    }

    #[test]
    fn root_level_nested_files_apply_to_every_installer() {
        let c = convert_fixture("JesseDuffield.lazygit").unwrap();
        assert_eq!(c.name, "lazygit");
        assert_eq!(c.tag.as_deref(), Some("v0.65.1"));
        assert_eq!(c.bins[0].name.as_deref(), Some("lazygit"));
        assert_eq!(
            c.manifest().unwrap().asset.target["windows-x86_64"],
            "lazygit_*_Windows_x86_64.zip"
        );
    }

    #[test]
    fn an_inno_installer_on_github_is_refused_for_its_type() {
        // Two installers per architecture (user and machine scope) with the
        // same URL: one download, and the refusal names the type.
        let Err(Rejected::Unsupported(why)) = convert_fixture("Git.Git") else {
            panic!("Git.Git should not convert")
        };
        assert!(why.contains("`inno`"), "{why}");
    }

    #[test]
    fn a_vendor_hosted_installer_is_refused_by_the_hard_rule() {
        assert_eq!(
            convert_fixture("Google.Chrome"),
            Err(Rejected::NotGithubReleases)
        );
    }

    const MIXED: &str = "\
PackageIdentifier: Some.Tool
PackageVersion: 1.2.0
InstallerType: zip
NestedInstallerType: portable
NestedInstallerFiles:
- RelativeFilePath: tool.exe
Installers:
- Architecture: x64
  InstallerUrl: https://github.com/some/tool/releases/download/v1.2.0/tool-1.2.0-x64.zip
  InstallerSha256: 0000000000000000000000000000000000000000000000000000000000000000
- Architecture: arm64
  InstallerUrl: https://downloads.example.com/tool/1.2.0/tool-arm64.zip
  InstallerSha256: 1111111111111111111111111111111111111111111111111111111111111111
ManifestType: installer
ManifestVersion: 1.12.0
";

    #[test]
    fn one_architecture_off_github_rejects_the_package() {
        assert_eq!(convert(MIXED).unwrap(), Err(Rejected::NotGithubReleases));
    }

    const CHOICES: &str = "\
PackageIdentifier: Some.Tool
PackageVersion: 2.0
Installers:
- Architecture: x64
  InstallerType: msi
  InstallerUrl: https://github.com/some/tool/releases/download/v2.0/tool-2.0-x64.msi
- Architecture: x64
  InstallerType: zip
  NestedInstallerType: portable
  NestedInstallerFiles:
  - RelativeFilePath: tool-2.0/tool-cli.exe
    PortableCommandAlias: tool
  InstallerLocale: de-DE
  InstallerUrl: https://github.com/some/tool/releases/download/v2.0/tool-2.0-x64-de.zip
- Architecture: x64
  InstallerType: zip
  NestedInstallerType: portable
  NestedInstallerFiles:
  - RelativeFilePath: tool-2.0/tool-cli.exe
    PortableCommandAlias: tool
  InstallerLocale: en-US
  InstallerUrl: https://github.com/some/tool/releases/download/v2.0/tool-2.0-x64.zip
  InstallerSha256: ABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAB
ManifestType: installer
ManifestVersion: 1.12.0
";

    #[test]
    fn of_several_installers_the_placeable_english_one_is_taken() {
        let c = convert(CHOICES).unwrap().unwrap();
        assert_eq!(c.version, "2.0", "a YAML number is still the version text");
        assert_eq!(c.artifacts["windows-x86_64"].file, "tool-2.0-x64.zip");
        assert_eq!(c.bins[0].name.as_deref(), Some("tool"));
        assert_eq!(c.bins[0].path.as_deref(), Some("*tool-cli.exe"));
    }

    const TWO_ZIPS: &str = "\
PackageIdentifier: Some.Tool
PackageVersion: 3.0.0
InstallerType: portable
Installers:
- Architecture: x64
  InstallerUrl: https://github.com/some/tool/releases/download/v3.0.0/tool-gnu.exe
- Architecture: x64
  InstallerUrl: https://github.com/some/tool/releases/download/v3.0.0/tool-msvc.exe
";

    #[test]
    fn two_equally_good_installers_for_one_target_are_refused_by_name() {
        let Err(Rejected::Unsupported(why)) = convert(TWO_ZIPS).unwrap() else {
            panic!("two candidates should be refused")
        };
        assert!(
            why.contains("tool-gnu.exe") && why.contains("tool-msvc.exe"),
            "{why}"
        );
    }

    const PORTABLE_NEUTRAL: &str = "\
PackageIdentifier: Some.Script
PackageVersion: 0.4.1
InstallerType: portable
Commands:
- some-script
Installers:
- Architecture: neutral
  InstallerUrl: https://github.com/some/script/releases/latest/download/some-script.exe
";

    #[test]
    fn a_neutral_portable_covers_both_windows_targets_with_its_command() {
        let c = convert(PORTABLE_NEUTRAL).unwrap().unwrap();
        assert_eq!(c.tag, None, "a latest/download URL names no tag");
        assert_eq!(c.artifacts.len(), 2);
        assert_eq!(c.bins[0].name.as_deref(), Some("some-script"));
        assert_eq!(c.artifacts["windows-x86_64"].sha256, None);
    }

    fn winget() -> (Recorded, Endpoints) {
        (
            Recorded::default(),
            Endpoints {
                winget_api: "https://api.test/m".into(),
                winget_raw: "https://raw.test/m".into(),
                ..Endpoints::default()
            },
        )
    }

    #[test]
    fn the_newest_version_folder_is_read_numerically() {
        let (f, e) = winget();
        let f = f
            .with("https://api.test/m/g/Git/Git", fixture("list-Git.Git.json"))
            .with(
                "https://raw.test/m/g/Git/Git/2.55.0.5/Git.Git.installer.yaml",
                fixture("Git.Git.installer.yaml"),
            );
        // 2.55.0.5 sorts after 2.55.0 and 2.9.x; reaching the inno refusal
        // proves that is the manifest that was read.
        let err = lookup(&f, &e, "Git.Git").unwrap_err().to_string();
        assert!(
            err.starts_with("Git.Git can't be converted: it installs with a winget `inno`"),
            "{err}"
        );
    }

    #[test]
    fn subpackage_folders_are_not_versions() {
        let (f, e) = winget();
        let f = f
            .with(
                "https://api.test/m/g/Google/Chrome",
                fixture("list-Google.Chrome.json"),
            )
            .with(
                "https://raw.test/m/g/Google/Chrome/154.0.8037.93/Google.Chrome.installer.yaml",
                fixture("Google.Chrome.installer.yaml"),
            );
        let err = lookup(&f, &e, "Google.Chrome").unwrap_err().to_string();
        assert_eq!(err, crate::import::not_github_message("Google.Chrome"));
    }

    #[test]
    fn a_singleton_manifest_is_read_when_there_is_no_installer_file() {
        let (f, e) = winget();
        let f = f
            .with(
                "https://api.test/m/s/Some/Tool",
                r#"[{"name":"1.2.0","type":"dir"}]"#,
            )
            .with("https://raw.test/m/s/Some/Tool/1.2.0/Some.Tool.yaml", MIXED);
        let err = lookup(&f, &e, "Some.Tool").unwrap_err().to_string();
        assert_eq!(err, crate::import::not_github_message("Some.Tool"));
    }

    #[test]
    fn an_unknown_identifier_says_identifiers_are_case_sensitive() {
        let (f, e) = winget();
        let err = lookup(&f, &e, "burntsushi.ripgrep.msvc")
            .unwrap_err()
            .to_string();
        assert!(err.contains("case-sensitive"), "{err}");
        let err = lookup(&f, &e, "ripgrep").unwrap_err().to_string();
        assert!(err.contains("Publisher.Package"), "{err}");
    }

    #[test]
    fn the_whole_lookup_reaches_the_converted_package() {
        let (f, e) = winget();
        let f = f
            .with(
                "https://api.test/m/b/BurntSushi/ripgrep/MSVC",
                fixture("list-BurntSushi.ripgrep.MSVC.json"),
            )
            .with(
                "https://raw.test/m/b/BurntSushi/ripgrep/MSVC/15.2.0/BurntSushi.ripgrep.MSVC.installer.yaml",
                fixture("BurntSushi.ripgrep.MSVC.installer.yaml"),
            );
        let found = lookup(&f, &e, "BurntSushi.ripgrep.MSVC").unwrap();
        assert_eq!(found.converted.version, "15.2.0");
    }

    #[test]
    fn version_folders_order_like_winget_orders_them() {
        assert_eq!(compare_versions("2.55.0.5", "2.55.0"), Ordering::Greater);
        assert_eq!(compare_versions("2.10.0", "2.9.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), Ordering::Equal);
    }
}
