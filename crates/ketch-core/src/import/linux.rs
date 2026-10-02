//! Linux: a package from Arch Linux — the official repositories, then the AUR.
//!
//! No cross-distribution catalogue names an artifact URL (plan.md, M17), and
//! the distribution repositories build from source. Arch is the exception
//! worth having: the AUR's `-bin` packages repackage what upstream
//! published, and every package's `.SRCINFO` lists its downloads per
//! architecture with their checksums, in a format meant for machines.
//!
//! A name is looked for in four places, in this order: the official
//! repositories, the AUR package of that exact name, `<name>-bin`, and AUR
//! packages that `provide` the name. The first that converts wins — unless
//! the ones that convert are different projects, which is refused rather
//! than guessed.

use super::{
    assemble, bin_entry, check_name, foreign_package, names_a_platform, release_url, Backend,
    Conversion, Download, Endpoints, Fetch, Found, Rejected,
};
use crate::error::{Error, Result};
use crate::model::{Arch, Os, PackageKind, TargetSpec};
use serde::Deserialize;
use std::collections::BTreeMap;

/// The parts of a `.SRCINFO` a conversion reads.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SrcInfo {
    pub pkgbase: String,
    pub pkgver: String,
    pub arch: Vec<String>,
    pub provides: Vec<String>,
    /// `source` and `source_<arch>`, keyed by the suffix ("" for none).
    pub sources: BTreeMap<String, Vec<String>>,
    /// `sha256sums` and `sha256sums_<arch>`, index-aligned with `sources`.
    pub sha256: BTreeMap<String, Vec<String>>,
}

/// Read a `.SRCINFO`, keeping the base section and the one `pkgname` section
/// asked for, which may override `arch` and `provides`.
pub(crate) fn parse_srcinfo(text: &str, pkgname: &str) -> SrcInfo {
    let mut info = SrcInfo::default();
    let mut section_arch: Option<Vec<String>> = None;
    let mut section_provides: Option<Vec<String>> = None;
    // None while in the base section; Some(true) inside the wanted package.
    let mut in_package: Option<bool> = None;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once(" = ") else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim().to_string());
        if key == "pkgname" {
            in_package = Some(value == pkgname);
            continue;
        }
        match in_package {
            Some(false) => continue,
            Some(true) => match key {
                "arch" => section_arch.get_or_insert_with(Vec::new).push(value),
                "provides" => section_provides.get_or_insert_with(Vec::new).push(value),
                _ => {}
            },
            None => match key {
                "pkgbase" => info.pkgbase = value,
                "pkgver" => info.pkgver = value,
                "arch" => info.arch.push(value),
                "provides" => info.provides.push(value),
                _ => {
                    if let Some(suffix) = key.strip_prefix("source") {
                        let suffix = suffix.trim_start_matches('_').to_string();
                        info.sources.entry(suffix).or_default().push(value);
                    } else if let Some(suffix) = key.strip_prefix("sha256sums") {
                        let suffix = suffix.trim_start_matches('_').to_string();
                        info.sha256.entry(suffix).or_default().push(value);
                    }
                }
            },
        }
    }
    if let Some(arch) = section_arch {
        info.arch = arch;
    }
    if let Some(provides) = section_provides {
        info.provides = provides;
    }
    info
}

/// `name::https://…` or `https://…`: the URL, when this source is remote. A
/// local file is packaging (a `.desktop`, a wrapper script), and a VCS
/// source (`git+https://`) is a checkout, not a download.
fn remote_url(source: &str) -> Option<&str> {
    let url = source.split_once("::").map_or(source, |(_, u)| u);
    (url.starts_with("https://") || url.starts_with("http://")).then_some(url)
}

/// Whether a download is the package itself rather than something packaged
/// beside it (a licence, an icon): an archive, an AppImage, a distribution
/// package, or a file whose name says which platform it is for.
fn is_payload(url: &str) -> bool {
    let file = url.rsplit('/').next().unwrap_or(url).to_ascii_lowercase();
    const ARCHIVES: &[&str] = &[
        ".tar.gz",
        ".tgz",
        ".tar.xz",
        ".txz",
        ".tar.bz2",
        ".tbz",
        ".tar.zst",
        ".zip",
        ".gz",
        ".xz",
        ".appimage",
    ];
    ARCHIVES.iter().any(|ext| file.ends_with(ext))
        || foreign_package(&file).is_some()
        || names_a_platform(&file)
        || !file.contains('.')
}

/// The ketch targets an Arch `arch` value covers.
fn targets_of(arch: &str) -> Vec<TargetSpec> {
    let linux = |arch| TargetSpec {
        os: Os::Linux,
        arch,
    };
    match arch {
        "x86_64" => vec![linux(Arch::X86_64)],
        "aarch64" => vec![linux(Arch::Aarch64)],
        "any" => vec![linux(Arch::X86_64), linux(Arch::Aarch64)],
        _ => Vec::new(),
    }
}

/// The install name: the package name without the packaging suffix.
fn base_name(pkgname: &str) -> &str {
    for suffix in ["-bin", "-appimage", "-git"] {
        if let Some(base) = pkgname.strip_suffix(suffix) {
            if !base.is_empty() {
                return base;
            }
        }
    }
    pkgname
}

/// Convert one package's `.SRCINFO`.
pub(crate) fn convert(pkgname: &str, srcinfo: &str) -> Conversion {
    let info = parse_srcinfo(srcinfo, pkgname);
    let per_arch = info.sources.keys().any(|k| !k.is_empty());
    let mut downloads = Vec::new();
    for arch in &info.arch {
        // With per-architecture sources, the shared `source` list is the
        // packaging around them; without, it is the download for every arch.
        let key = if per_arch { arch.as_str() } else { "" };
        let sources = info.sources.get(key).cloned().unwrap_or_default();
        let sums = info.sha256.get(key).cloned().unwrap_or_default();
        let payloads: Vec<(String, Option<String>)> = sources
            .iter()
            .enumerate()
            .filter_map(|(i, s)| remote_url(s).map(|u| (u.to_string(), sums.get(i).cloned())))
            .filter(|(u, _)| is_payload(u))
            .collect();
        // A package that offers its own `.deb` beside an AppImage or a
        // tarball means the one ketch can unpack.
        let placeable: Vec<&(String, Option<String>)> = payloads
            .iter()
            .filter(|(u, _)| foreign_package(u).is_none())
            .collect();
        let chosen: Vec<&(String, Option<String>)> = if placeable.is_empty() {
            payloads.iter().collect()
        } else {
            placeable
        };
        for target in targets_of(arch) {
            for (url, sha) in &chosen {
                downloads.push(Download {
                    target,
                    url: url.clone(),
                    sha256: sha.clone(),
                });
            }
        }
    }

    let name = base_name(pkgname);
    let command = info
        .provides
        .iter()
        .map(|p| p.split(['=', '<', '>']).next().unwrap_or(p).trim())
        .find(|p| !p.is_empty() && !p.contains(".so"))
        .unwrap_or(name)
        .to_string();
    // A bare download (an AppImage) is the executable itself, named however
    // upstream named it; an archive is searched for the command by name.
    let single_file = downloads
        .first()
        .and_then(|d| release_url(&d.url))
        .map(|u| u.file)
        .filter(|f| {
            let lower = f.to_ascii_lowercase();
            lower.ends_with(".appimage") || !lower.contains('.')
        });
    let bins = bin_entry(&command, single_file.as_deref())
        .into_iter()
        .collect();
    assemble(
        Backend::Linux,
        pkgname,
        name,
        &info.pkgver,
        PackageKind::Auto,
        bins,
        downloads,
    )
}

#[derive(Debug, Deserialize)]
struct OfficialSearch {
    #[serde(default)]
    results: Vec<OfficialPackage>,
}

#[derive(Debug, Deserialize)]
struct OfficialPackage {
    pkgname: String,
    pkgbase: String,
    repo: String,
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    #[serde(default)]
    results: Vec<AurPackage>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AurPackage {
    name: String,
    package_base: String,
}

/// One place a name was found.
struct Candidate {
    pkgname: String,
    pkgbase: String,
    /// "Arch [extra]" or "the AUR", for messages.
    origin: String,
    official: bool,
}

fn aur(fetch: &dyn Fetch, url: &str) -> Result<Vec<AurPackage>> {
    let Some(body) = fetch.text(url)? else {
        return Ok(Vec::new());
    };
    let response: AurResponse =
        serde_json::from_str(&body).map_err(|e| Error::parse("AUR RPC response", e.to_string()))?;
    if let Some(error) = response.error {
        return Err(Error::msg(format!("the AUR refused the query: {error}")));
    }
    Ok(response.results)
}

fn candidates(fetch: &dyn Fetch, e: &Endpoints, name: &str) -> Result<Vec<Candidate>> {
    let mut out: Vec<Candidate> = Vec::new();
    // `+` is literal in Arch names but a space in a query string.
    let q = name.replace('+', "%2B");
    if let Some(body) = fetch.text(&format!("{}/packages/search/json/?name={q}", e.arch))? {
        let search: OfficialSearch = serde_json::from_str(&body)
            .map_err(|err| Error::parse("Arch package search", err.to_string()))?;
        if let Some(p) = search.results.iter().find(|p| {
            p.pkgname == name && !p.repo.contains("testing") && !p.repo.contains("staging")
        }) {
            out.push(Candidate {
                pkgname: p.pkgname.clone(),
                pkgbase: p.pkgbase.clone(),
                origin: format!("Arch [{}]", p.repo),
                official: true,
            });
        }
    }
    let mut info_url = format!("{}/rpc/v5/info?arg[]={q}", e.aur);
    if !name.ends_with("-bin") {
        info_url.push_str(&format!("&arg[]={q}-bin"));
    }
    let mut aur_found = aur(fetch, &info_url)?;
    // The exact name first, then `-bin`, whatever order the AUR answered in.
    aur_found.sort_by_key(|p| p.name != name);
    let provides = aur(fetch, &format!("{}/rpc/v5/search/{q}?by=provides", e.aur))?;
    let mut prebuilt: Vec<AurPackage> = provides
        .into_iter()
        .filter(|p| p.name.ends_with("-bin") || p.name.ends_with("-appimage"))
        .collect();
    prebuilt.sort_by(|a, b| a.name.cmp(&b.name));
    for p in aur_found.into_iter().chain(prebuilt) {
        if out.iter().any(|c| c.pkgname == p.name) {
            continue;
        }
        out.push(Candidate {
            pkgname: p.name,
            pkgbase: p.package_base,
            origin: "the AUR".into(),
            official: false,
        });
    }
    Ok(out)
}

/// Look `name` up in Arch Linux and convert the package it settles on.
pub fn lookup(fetch: &dyn Fetch, endpoints: &Endpoints, name: &str) -> Result<Found> {
    let name = check_name(name)?;
    let found = candidates(fetch, endpoints, name)?;
    if found.is_empty() {
        return Err(Error::msg(format!(
            "Arch Linux has no package named `{name}`, in the official repositories or the AUR"
        )));
    }
    let mut verdicts: Vec<(&Candidate, Conversion)> = Vec::new();
    for candidate in &found {
        let url = if candidate.official {
            format!(
                "{}/{}/-/raw/main/.SRCINFO",
                endpoints.arch_gitlab, candidate.pkgbase
            )
        } else {
            format!(
                "{}/cgit/aur.git/plain/.SRCINFO?h={}",
                endpoints.aur, candidate.pkgbase
            )
        };
        let verdict = match fetch.text(&url)? {
            Some(srcinfo) => convert(&candidate.pkgname, &srcinfo),
            None => Err(Rejected::Unsupported(format!(
                "{} lists it, but its .SRCINFO could not be read",
                candidate.origin
            ))),
        };
        verdicts.push((candidate, verdict));
    }

    let converted: Vec<(&Candidate, &crate::import::Converted)> = verdicts
        .iter()
        .filter_map(|(c, v)| v.as_ref().ok().map(|conv| (*c, conv)))
        .collect();
    let Some(&(first, chosen)) = converted
        .iter()
        .find(|(c, _)| c.pkgname == name)
        .or_else(|| converted.first())
    else {
        // Nothing converts: the reason given is the first place's, the one a
        // user asking for this exact name meant.
        let (_, verdict) = verdicts.swap_remove(0);
        return Err(verdict
            .err()
            .unwrap_or(Rejected::NotGithubReleases)
            .into_error(name));
    };
    if first.pkgname != name {
        let others: Vec<String> = converted
            .iter()
            .filter(|(_, c)| !c.repo.eq_ignore_ascii_case(&chosen.repo))
            .map(|(cand, c)| format!("{} ({})", cand.pkgname, c.repo))
            .collect();
        if !others.is_empty() {
            return Err(Error::msg(format!(
                "`{name}` is ambiguous in Arch Linux: {} ({}) and {}; import one of them by its exact name",
                first.pkgname,
                chosen.repo,
                others.join(", ")
            )));
        }
    }
    let mut notes = Vec::new();
    if let Some((skipped, _)) = verdicts
        .iter()
        .find(|(c, v)| c.official && v.is_err() && c.pkgname != first.pkgname)
    {
        notes.push(format!(
            "{} {} builds from source; using {}'s {}",
            skipped.origin, skipped.pkgname, first.origin, first.pkgname
        ));
    } else if first.pkgname != name {
        notes.push(format!("using {}'s {}", first.origin, first.pkgname));
    }
    Ok(Found {
        converted: chosen.clone(),
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{not_github_message, Recorded};
    use pretty_assertions::assert_eq;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/src/import/fixtures/linux/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn a_bin_package_converts_its_two_ketch_architectures() {
        let c = convert("lazydocker-bin", &fixture("aur-lazydocker-bin.SRCINFO")).unwrap();
        assert_eq!(c.name, "lazydocker");
        assert_eq!(c.repo, "jesseduffield/lazydocker");
        assert_eq!(c.tag.as_deref(), Some("v0.25.2"));
        assert_eq!(c.bins[0].name.as_deref(), Some("lazydocker"));
        assert_eq!(c.bins[0].path, None);
        assert_eq!(
            c.artifacts["linux-aarch64"].sha256.as_deref(),
            Some("005c38b685aaa557e7d646d83a3dadb5024340eeed8c6a2e1949eee6f530de23")
        );
        insta::assert_snapshot!(c.render().unwrap(), @r#"
        # Written by `ketch import linux lazydocker-bin`. Schema: docs/MANIFESTS.md.
        name = "lazydocker"
        source = "github:jesseduffield/lazydocker"

        bin = [{ name = "lazydocker" }]

        [asset.target]
        "linux-aarch64" = "lazydocker_*_Linux_arm64.tar.gz"
        "linux-x86_64" = "lazydocker_*_Linux_x86_64.tar.gz"
        "#);
    }

    #[test]
    fn an_official_package_built_from_a_source_tarball_is_refused() {
        assert_eq!(
            convert("lazygit", &fixture("arch-lazygit.SRCINFO")),
            Err(Rejected::NotGithubReleases)
        );
    }

    #[test]
    fn a_vendor_download_is_refused_by_the_hard_rule() {
        assert_eq!(
            convert(
                "visual-studio-code-bin",
                &fixture("aur-visual-studio-code-bin.SRCINFO")
            ),
            Err(Rejected::NotGithubReleases)
        );
    }

    #[test]
    fn of_a_deb_and_an_appimage_the_appimage_is_taken_and_linked_by_its_command() {
        let c = convert("obsidian-bin", &fixture("aur-obsidian.SRCINFO")).unwrap();
        assert_eq!(c.name, "obsidian");
        assert_eq!(c.repo, "obsidianmd/obsidian-releases");
        assert_eq!(c.artifacts["linux-x86_64"].file, "Obsidian-1.13.7.AppImage");
        // `obsidian-bin` is x86_64 only; its sibling `obsidian-appimage`
        // in the same base is the one with arm64.
        assert_eq!(c.artifacts.len(), 1);
        let manifest = c.manifest().unwrap();
        assert_eq!(manifest.bin[0].name.as_deref(), Some("obsidian"));
        assert_eq!(
            manifest.bin[0].path.as_deref(),
            Some("*Obsidian-*.AppImage")
        );
        assert_eq!(manifest.asset.target["linux-x86_64"], "Obsidian-*.AppImage");
    }

    #[test]
    fn a_split_package_reads_its_own_section() {
        let c = convert("obsidian-appimage", &fixture("aur-obsidian.SRCINFO")).unwrap();
        assert_eq!(
            c.artifacts.keys().collect::<Vec<_>>(),
            vec!["linux-aarch64", "linux-x86_64"]
        );
    }

    #[test]
    fn only_a_deb_is_refused_for_its_format() {
        let srcinfo = "pkgbase = t-bin\n\tpkgver = 1.0\n\tarch = x86_64\n\
            \tsource_x86_64 = https://github.com/o/t/releases/download/v1.0/t_1.0_amd64.deb\n\
            \tsha256sums_x86_64 = SKIP\npkgname = t-bin\n";
        let Err(Rejected::Unsupported(why)) = convert("t-bin", srcinfo) else {
            panic!("a .deb should not convert")
        };
        assert!(why.contains("`.deb`"), "{why}");
    }

    #[test]
    fn a_skipped_checksum_converts_with_none_and_local_files_are_ignored() {
        let srcinfo = "pkgbase = t-bin\n\tpkgver = 1.0\n\tarch = x86_64\n\
            \tsource = t.desktop\n\tsource = LICENSE::https://raw.githubusercontent.com/o/t/v1.0/LICENSE\n\
            \tsha256sums = SKIP\n\tsha256sums = SKIP\n\
            \tsource_x86_64 = https://github.com/o/t/releases/download/v1.0/t-linux-x86_64.tar.gz\n\
            \tsha256sums_x86_64 = SKIP\npkgname = t-bin\n";
        let c = convert("t-bin", srcinfo).unwrap();
        assert_eq!(c.artifacts["linux-x86_64"].sha256, None);
        assert_eq!(c.artifacts.len(), 1);
    }

    fn arch() -> (Recorded, Endpoints) {
        (
            Recorded::default(),
            Endpoints {
                arch: "https://arch.test".into(),
                arch_gitlab: "https://gitlab.test/p".into(),
                aur: "https://aur.test".into(),
                ..Endpoints::default()
            },
        )
    }

    const NOTHING: &str = r#"{"resultcount":0,"results":[],"type":"multiinfo","version":5}"#;

    #[test]
    fn an_official_source_build_yields_to_the_aur_bin_package() {
        let (f, e) = arch();
        let f = f
            .with(
                "https://arch.test/packages/search/json/?name=lazydocker",
                fixture("arch-search-lazydocker.json"),
            )
            .with(
                "https://gitlab.test/p/lazydocker/-/raw/main/.SRCINFO",
                fixture("arch-lazydocker.SRCINFO"),
            )
            .with(
                "https://aur.test/rpc/v5/info?arg[]=lazydocker&arg[]=lazydocker-bin",
                fixture("aur-info-lazydocker-bin.json"),
            )
            .with(
                "https://aur.test/cgit/aur.git/plain/.SRCINFO?h=lazydocker-bin",
                fixture("aur-lazydocker-bin.SRCINFO"),
            );
        let found = lookup(&f, &e, "lazydocker").unwrap();
        assert_eq!(found.converted.package, "lazydocker-bin");
        assert_eq!(
            found.notes,
            vec!["Arch [extra] lazydocker builds from source; using the AUR's lazydocker-bin"]
        );
    }

    #[test]
    fn an_official_source_build_alone_is_refused_by_the_hard_rule() {
        let (f, e) = arch();
        let f = f
            .with(
                "https://arch.test/packages/search/json/?name=lazygit",
                fixture("arch-search-lazygit.json"),
            )
            .with(
                "https://gitlab.test/p/lazygit/-/raw/main/.SRCINFO",
                fixture("arch-lazygit.SRCINFO"),
            )
            .with(
                "https://aur.test/rpc/v5/info?arg[]=lazygit&arg[]=lazygit-bin",
                NOTHING,
            );
        let err = lookup(&f, &e, "lazygit").unwrap_err().to_string();
        assert_eq!(err, not_github_message("lazygit"));
    }

    #[test]
    fn packages_providing_a_name_from_one_project_resolve_to_the_first() {
        let (f, e) = arch();
        let f = f
            .with(
                "https://aur.test/rpc/v5/info?arg[]=obsidian&arg[]=obsidian-bin",
                fixture("aur-info-obsidian-bin.json"),
            )
            .with(
                "https://aur.test/rpc/v5/search/obsidian?by=provides",
                fixture("aur-provides-obsidian.json"),
            )
            .with(
                "https://aur.test/cgit/aur.git/plain/.SRCINFO?h=obsidian",
                fixture("aur-obsidian.SRCINFO"),
            );
        let found = lookup(&f, &e, "obsidian").unwrap();
        assert_eq!(found.converted.package, "obsidian-bin");
        assert_eq!(found.notes, vec!["using the AUR's obsidian-bin"]);
    }

    fn srcinfo(base: &str, repo: &str) -> String {
        format!(
            "pkgbase = {base}\n\tpkgver = 1.0\n\tarch = x86_64\n\
             \tsource_x86_64 = https://github.com/{repo}/releases/download/v1.0/tool-linux-x86_64.tar.gz\n\
             pkgname = {base}\n"
        )
    }

    #[test]
    fn a_name_two_projects_provide_is_refused_and_both_are_named() {
        let (f, e) = arch();
        let f = f
            .with(
                "https://aur.test/rpc/v5/info?arg[]=tool&arg[]=tool-bin",
                NOTHING,
            )
            .with(
                "https://aur.test/rpc/v5/search/tool?by=provides",
                r#"{"results":[{"Name":"tool-bin","PackageBase":"tool-bin"},
                               {"Name":"other-tool-bin","PackageBase":"other-tool-bin"},
                               {"Name":"tool-git","PackageBase":"tool-git"}]}"#,
            )
            .with(
                "https://aur.test/cgit/aur.git/plain/.SRCINFO?h=tool-bin",
                srcinfo("tool-bin", "alice/tool"),
            )
            .with(
                "https://aur.test/cgit/aur.git/plain/.SRCINFO?h=other-tool-bin",
                srcinfo("other-tool-bin", "bob/tool"),
            );
        let err = lookup(&f, &e, "tool").unwrap_err().to_string();
        assert!(
            err.contains("ambiguous")
                && err.contains("other-tool-bin (bob/tool)")
                && err.contains("tool-bin (alice/tool)"),
            "{err}"
        );
        // Asked for by its exact name, the package is not ambiguous.
        let f = f.with(
            "https://aur.test/rpc/v5/info?arg[]=tool-bin",
            r#"{"results":[{"Name":"tool-bin","PackageBase":"tool-bin"}]}"#,
        );
        let found = lookup(&f, &e, "tool-bin").unwrap();
        assert_eq!(found.converted.repo, "alice/tool");
    }

    #[test]
    fn an_unknown_name_is_reported_as_unknown() {
        let (f, e) = arch();
        let err = lookup(&f, &e, "nope").unwrap_err().to_string();
        assert!(err.contains("no package named `nope`"), "{err}");
    }

    #[test]
    fn an_aur_error_is_an_error_not_an_empty_answer() {
        let (f, e) = arch();
        let f = f.with(
            "https://aur.test/rpc/v5/info?arg[]=x&arg[]=x-bin",
            r#"{"type":"error","error":"Too many package results.","results":[]}"#,
        );
        let err = lookup(&f, &e, "x").unwrap_err().to_string();
        assert!(err.contains("Too many package results"), "{err}");
    }
}
