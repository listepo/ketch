//! `ketch import`: a package from another package manager, as a ketch manifest.
//!
//! Each backend looks a name up in its own catalogue (`winget.rs`,
//! `brew.rs`, `linux.rs`) and reduces what it finds to the same plain
//! [`Converted`]: one GitHub repository, one release tag, and the file that
//! catalogue would download for each target. The rules every backend shares
//! live here, so "is this a GitHub release?" has one answer whichever
//! catalogue asked it.
//!
//! The source's own definition is untrusted input — someone else wrote the
//! formula, the winget manifest, the PKGBUILD — so nothing from it reaches
//! the manifest unchecked: the repository goes through
//! `config::validate_repo`, names through `Manifest::validate`, and only the
//! handful of fields ketch needs is carried over at all.

pub mod brew;
mod fetch;

#[cfg(test)]
pub(crate) use fetch::Recorded;
pub use fetch::{Endpoints, Fetch, HttpFetch};

use crate::error::{Error, Result};
use crate::model::{
    normalize_name, AssetSelector, BinSpec, Manifest, PackageKind, PackageRef, TargetSpec,
};
use std::collections::BTreeMap;
use std::fmt;

/// Which catalogue a package is imported from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The Windows Package Manager's community repository.
    Winget,
    /// Homebrew: formulae and casks.
    Brew,
    /// Arch Linux: the official repositories and the AUR.
    Linux,
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Backend::Winget => "winget",
            Backend::Brew => "brew",
            Backend::Linux => "linux",
        })
    }
}

/// The file a catalogue downloads for one target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    /// The release asset's file name.
    pub file: String,
    /// Lowercase hex SHA-256 the catalogue recorded, when it recorded one.
    pub sha256: Option<String>,
}

/// A package another manager describes, reduced to what ketch needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    pub backend: Backend,
    /// The catalogue's own identifier: a cask token, a winget id, an AUR name.
    pub package: String,
    /// The install name, from the catalogue's name for the package.
    pub name: String,
    /// The version the catalogue gives, for messages.
    pub version: String,
    /// `owner/repo`, already validated.
    pub repo: String,
    /// The release tag every artifact comes from; `None` for a
    /// `releases/latest/download/` URL, which names no tag.
    pub tag: Option<String>,
    /// `App` for a macOS bundle, `Auto` otherwise.
    pub kind: PackageKind,
    /// What to put on `PATH`. Empty only for an app.
    pub bins: Vec<BinSpec>,
    /// Per target (`TargetSpec` display form), the asset that catalogue uses.
    pub artifacts: BTreeMap<String, Artifact>,
}

/// What a lookup settled on, and anything about the choice worth saying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub converted: Converted,
    /// Lines for the user: which of several candidates was taken, and why.
    pub notes: Vec<String>,
}

impl From<Converted> for Found {
    fn from(converted: Converted) -> Self {
        Found {
            converted,
            notes: Vec::new(),
        }
    }
}

/// Why a package found in a catalogue does not convert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejected {
    /// At least one artifact is not a GitHub release asset.
    NotGithubReleases,
    /// It is on GitHub Releases, but in a shape ketch cannot install; the
    /// text completes "`<name>` can't be converted: …".
    Unsupported(String),
}

/// A converter's verdict on one package.
pub type Conversion = std::result::Result<Converted, Rejected>;

impl Rejected {
    /// The error the command exits with.
    pub fn into_error(self, name: &str) -> Error {
        match self {
            Rejected::NotGithubReleases => Error::msg(not_github_message(name)),
            Rejected::Unsupported(why) => Error::msg(format!("{name} can't be converted: {why}.")),
        }
    }
}

/// The one sentence the hard rule speaks in, kept in one place so every
/// backend and every test says it the same way.
pub fn not_github_message(name: &str) -> String {
    format!(
        "{name} can't be converted: it is not distributed through GitHub Releases, \
         and that is not supported yet."
    )
}

/// A name as typed, checked before it is put into a URL path or query.
///
/// Every catalogue's identifiers fit this set (winget ids are dotted, cask
/// tokens may carry `@`, Arch names `+`); anything outside it is not a name
/// any of them would answer to, and refusing it here means no backend has to
/// think about escaping.
pub fn check_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let usable = !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '@'))
        && !name.starts_with(['.', '-']);
    if usable {
        Ok(name)
    } else {
        Err(Error::msg(format!(
            "`{}` is not a package name",
            name.escape_debug()
        )))
    }
}

/// A GitHub release download URL, taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseUrl {
    pub repo: String,
    pub tag: Option<String>,
    pub file: String,
}

/// `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`, or
/// `…/releases/latest/download/<file>`; `None` for anything else.
///
/// Deliberately narrow. A GitHub homepage, an `archive/refs/tags/` source
/// tarball, `codeload`, `objects.githubusercontent.com` and a raw file are
/// all on GitHub without being a release asset someone published, and the
/// rule is about what was published.
pub(crate) fn release_url(url: &str) -> Option<ReleaseUrl> {
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    if !host.eq_ignore_ascii_case("github.com") || path.contains(['?', '#']) {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    let (owner, repo, tag, file) = match parts.as_slice() {
        [owner, repo, "releases", "download", tag, file] => (*owner, *repo, Some(*tag), *file),
        [owner, repo, "releases", "latest", "download", file] => (*owner, *repo, None, *file),
        _ => return None,
    };
    let repo = crate::config::validate_repo("release URL", format!("{owner}/{repo}")).ok()?;
    let tag = match tag {
        Some(t) => Some(percent_decode(t)?),
        None => None,
    };
    let file = percent_decode(file)?;
    if tag.as_deref().is_some_and(str::is_empty) || file.is_empty() || file.contains(['/', '\\']) {
        return None;
    }
    Some(ReleaseUrl { repo, tag, file })
}

/// `%XX` decoding for a path segment; `None` for a malformed escape or bytes
/// that are not UTF-8.
fn percent_decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = raw.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Package formats some other installer unpacks, which ketch cannot.
const FOREIGN_PACKAGES: &[&str] = &[
    ".msi",
    ".msix",
    ".msixbundle",
    ".appx",
    ".appxbundle",
    ".deb",
    ".rpm",
    ".snap",
    ".flatpak",
    ".pkg.tar.zst",
    ".pkg.tar.xz",
];

/// The format, when `file` is a package for another installer.
pub(crate) fn foreign_package(file: &str) -> Option<&'static str> {
    let lower = file.to_ascii_lowercase();
    FOREIGN_PACKAGES
        .iter()
        .find(|ext| lower.ends_with(**ext))
        .copied()
}

/// True when `file` names an operating system and an architecture, the way
/// a prebuilt release asset does.
///
/// A project that publishes binaries publishes one per platform and says
/// which in the name; a source archive is the same for everyone and says
/// neither. Word-sized tokens (`mac`, `win`, `gnu`) only match whole words,
/// or `darwin` would contain `win`.
pub(crate) fn names_a_platform(file: &str) -> bool {
    use crate::model::{Arch, Os};
    let lower = file.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |token: &str| {
        if token.chars().all(|c| c.is_ascii_alphanumeric()) {
            words.contains(&token)
        } else {
            lower.contains(token)
        }
    };
    let os = [Os::MacOs, Os::Linux, Os::Windows]
        .iter()
        .any(|os| os.tokens().iter().any(|t| has(t)));
    let arch = [Arch::Aarch64, Arch::X86_64, Arch::Universal]
        .iter()
        .any(|arch| arch.tokens().iter().any(|t| has(t)));
    os && arch
}

/// The targets a prebuilt asset's name says it is for: both macOS
/// architectures for a universal build, nothing when it names no OS.
pub(crate) fn targets_named(file: &str) -> Vec<TargetSpec> {
    use crate::model::{Arch, Os};
    let lower = file.to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |token: &str| {
        if token.chars().all(|c| c.is_ascii_alphanumeric()) {
            words.contains(&token)
        } else {
            lower.contains(token)
        }
    };
    let Some(os) = [Os::MacOs, Os::Windows, Os::Linux]
        .into_iter()
        .find(|os| os.tokens().iter().any(|t| has(t)))
    else {
        return Vec::new();
    };
    let arches: Vec<Arch> = if Arch::Aarch64.tokens().iter().any(|t| has(t)) {
        vec![Arch::Aarch64]
    } else if Arch::X86_64.tokens().iter().any(|t| has(t)) {
        vec![Arch::X86_64]
    } else {
        vec![Arch::Aarch64, Arch::X86_64]
    };
    arches
        .into_iter()
        .map(|arch| TargetSpec { os, arch })
        .collect()
}

/// One artifact as a catalogue states it: the target, the URL, the checksum.
#[derive(Debug, Clone)]
pub(crate) struct Download {
    pub target: TargetSpec,
    pub url: String,
    pub sha256: Option<String>,
}

/// Reduce a catalogue's downloads to a [`Converted`], or say why not.
///
/// The order of the checks is the order of the messages a user should see:
/// first whether this is GitHub Releases at all — the hard rule, all or
/// nothing — then whether it is one release, then whether ketch can unpack
/// what was published.
pub(crate) fn assemble(
    backend: Backend,
    package: &str,
    name: &str,
    version: &str,
    kind: PackageKind,
    bins: Vec<BinSpec>,
    downloads: Vec<Download>,
) -> Conversion {
    if downloads.is_empty() {
        return Err(Rejected::NotGithubReleases);
    }
    let mut parsed = Vec::with_capacity(downloads.len());
    for download in downloads {
        let Some(url) = release_url(&download.url) else {
            return Err(Rejected::NotGithubReleases);
        };
        parsed.push((download, url));
    }
    let repo = parsed[0].1.repo.clone();
    // A mixed set — one architecture from GitHub, another from a mirror or
    // another repository — has no single `source` to write.
    if parsed
        .iter()
        .any(|(_, u)| !u.repo.eq_ignore_ascii_case(&repo))
    {
        return Err(Rejected::NotGithubReleases);
    }
    let tag = parsed[0].1.tag.clone();
    if parsed.iter().any(|(_, u)| u.tag != tag) {
        let mut tags: Vec<String> = parsed
            .iter()
            .map(|(_, u)| u.tag.clone().unwrap_or_else(|| "latest".into()))
            .collect();
        tags.sort();
        tags.dedup();
        return Err(Rejected::Unsupported(format!(
            "its downloads come from several releases of {repo} ({})",
            tags.join(", ")
        )));
    }
    let mut artifacts: BTreeMap<String, Artifact> = BTreeMap::new();
    for (download, url) in parsed {
        if let Some(format) = foreign_package(&url.file) {
            return Err(Rejected::Unsupported(format!(
                "it ships `{}`, a `{format}` package ketch can't unpack yet",
                url.file
            )));
        }
        let sha256 = download
            .sha256
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()));
        let artifact = Artifact {
            file: url.file,
            sha256,
        };
        let key = download.target.to_string();
        match artifacts.get(&key) {
            Some(existing) if existing.file != artifact.file => {
                return Err(Rejected::Unsupported(format!(
                    "it offers several downloads for {key} ({}, {}) and ketch can't tell which one it means",
                    existing.file, artifact.file
                )));
            }
            Some(_) => {}
            None => {
                artifacts.insert(key, artifact);
            }
        }
    }
    Ok(Converted {
        backend,
        package: package.to_string(),
        name: crate::config::sanitize_component(&normalize_name(name)),
        version: version.to_string(),
        repo,
        tag,
        kind,
        bins,
        artifacts,
    })
}

/// `name` (and a `path` only when the file is called something else), for
/// one executable a catalogue links.
///
/// The path is written as `*<file name>`: `*` matches across directories, so
/// the entry survives both the wrapper directory ketch unwraps and the
/// version a release puts in that directory's name.
pub(crate) fn bin_entry(command: &str, file_in_payload: Option<&str>) -> Option<BinSpec> {
    let command = strip_exe(command.trim());
    let command = command.rsplit(['/', '\\']).next().unwrap_or(command);
    if command.is_empty() {
        return None;
    }
    let path = file_in_payload
        .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string())
        .filter(|file| !file.is_empty() && !strip_exe(file).eq_ignore_ascii_case(command))
        .map(|file| format!("*{file}"));
    Some(BinSpec {
        path,
        name: Some(command.to_string()),
    })
}

fn strip_exe(name: &str) -> &str {
    if name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(".exe") {
        &name[..name.len() - 4]
    } else {
        name
    }
}

impl Converted {
    /// The manifest, with nothing in it ketch would not use.
    pub fn manifest(&self) -> Result<Manifest> {
        let mut target = BTreeMap::new();
        for (key, artifact) in &self.artifacts {
            target.insert(key.clone(), asset_glob(&artifact.file, self));
        }
        let manifest = Manifest {
            kind: self.kind,
            bin: self.bins.clone(),
            asset: AssetSelector {
                target,
                ..AssetSelector::default()
            },
            name: self.name.clone(),
            ..Manifest::inferred(PackageRef::github(self.repo.clone()))
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// The first line of the file, which also marks it as `ketch import`'s
    /// to rewrite.
    pub fn header(&self) -> String {
        format!(
            "{IMPORT_HEADER} {} {}`. Schema: docs/MANIFESTS.md.\n",
            self.backend, self.package
        )
    }

    /// The whole user manifest file.
    pub fn render(&self) -> Result<String> {
        Ok(crate::wizard::render_with_header(
            &self.manifest()?,
            &self.header(),
        ))
    }

    /// The artifact this catalogue chose for `target`, if it covers it.
    pub fn artifact_for(&self, target: &TargetSpec) -> Option<&Artifact> {
        self.artifacts.get(&target.to_string())
    }
}

/// What every file `ketch import` writes starts with.
pub const IMPORT_HEADER: &str = "# Written by `ketch import";

/// A per-target asset pattern: the file name with its version replaced by
/// `*`, so the pin keeps matching when `ketch upgrade` moves to the next
/// release. A name with no version in it stays exact.
pub(crate) fn asset_glob(file: &str, converted: &Converted) -> String {
    let mut candidates: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        let s = s.trim();
        // Two characters is too little to be a version rather than part of
        // a word: `v1` would also eat the `1` out of `x86_64`… almost.
        if s.len() >= 3 && s.chars().any(|c| c.is_ascii_digit()) {
            candidates.push(s.to_string());
        }
    };
    if let Some(tag) = &converted.tag {
        push(tag);
        push(tag.trim_start_matches(['v', 'V']));
        push(&numeric_core(tag));
    }
    push(&converted.version);
    push(&converted.version.replace(',', "-"));
    push(&numeric_core(&converted.version));
    candidates.sort_by_key(|c| std::cmp::Reverse(c.len()));
    candidates.dedup();
    let mut glob = file.to_string();
    for candidate in &candidates {
        glob = glob.replace(candidate.as_str(), "*");
    }
    while glob.contains("**") {
        glob = glob.replace("**", "*");
    }
    if glob == "*" || glob.chars().all(|c| c == '*' || c == '.') {
        file.to_string()
    } else {
        glob
    }
}

/// The longest run of digits and dots in `text` that starts and ends with a
/// digit: `0.160.0` from `rust-v0.160.0`.
fn numeric_core(text: &str) -> String {
    let mut best = "";
    for run in text.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
        let run = run.trim_matches('.');
        if run.len() > best.len() {
            best = run;
        }
    }
    best.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Arch, Os};
    use pretty_assertions::assert_eq;

    fn target(os: Os, arch: Arch) -> TargetSpec {
        TargetSpec { os, arch }
    }

    #[test]
    fn a_release_download_url_is_taken_apart() {
        let url = release_url(
            "https://github.com/BurntSushi/ripgrep/releases/download/15.2.0/ripgrep-15.2.0-x86_64-pc-windows-msvc.zip",
        )
        .unwrap();
        assert_eq!(url.repo, "BurntSushi/ripgrep");
        assert_eq!(url.tag.as_deref(), Some("15.2.0"));
        assert_eq!(url.file, "ripgrep-15.2.0-x86_64-pc-windows-msvc.zip");
    }

    #[test]
    fn a_latest_download_url_has_no_tag() {
        let url = release_url("https://github.com/o/r/releases/latest/download/tool.zip").unwrap();
        assert_eq!(url.tag, None);
        assert_eq!(url.file, "tool.zip");
    }

    #[test]
    fn an_encoded_tag_is_decoded() {
        let url =
            release_url("https://github.com/o/r/releases/download/v1.0%2Bbuild/t.zip").unwrap();
        assert_eq!(url.tag.as_deref(), Some("v1.0+build"));
    }

    #[rstest::rstest]
    #[case::source_tarball("https://github.com/BurntSushi/ripgrep/archive/refs/tags/15.2.0.tar.gz")]
    #[case::homepage("https://github.com/BurntSushi/ripgrep")]
    #[case::plain_http("http://github.com/o/r/releases/download/v1/t.zip")]
    #[case::other_host("https://dl.google.com/chrome/mac/universal/stable/GGRO/googlechrome.dmg")]
    #[case::sourceforge("https://downloads.sourceforge.net/project/x/x-1.0.zip")]
    #[case::lookalike_host("https://github.com.evil.example/o/r/releases/download/v1/t.zip")]
    #[case::query("https://github.com/o/r/releases/download/v1/t.zip?raw=1")]
    #[case::raw("https://raw.githubusercontent.com/o/r/main/t.zip")]
    #[case::bad_owner("https://github.com/../r/releases/download/v1/t.zip")]
    #[case::nested_file("https://github.com/o/r/releases/download/v1/a/t.zip")]
    #[case::encoded_slash("https://github.com/o/r/releases/download/v1/a%2Ft.zip")]
    fn anything_else_is_not_a_release_asset(#[case] url: &str) {
        assert_eq!(release_url(url), None, "{url}");
    }

    #[rstest::rstest]
    #[case("ripgrep-15.2.0-x86_64-apple-darwin.tar.gz", true)]
    #[case("fly-8.3.0-darwin-arm64.tgz", true)]
    #[case("lazygit_0.65.1_Windows_x86_64.zip", true)]
    #[case("jq-1.8.2.tar.gz", false)]
    #[case("Linux-PAM-1.7.3.tar.xz", false)]
    #[case("universal-ctags-6.2.1.tar.gz", false)]
    #[case("source.tar.gz", false)]
    #[case("darwin-tools-1.0.tar.gz", false)]
    fn a_prebuilt_asset_names_an_os_and_an_architecture(
        #[case] file: &str,
        #[case] expected: bool,
    ) {
        assert_eq!(names_a_platform(file), expected, "{file}");
    }

    fn converted(tag: Option<&str>, version: &str) -> Converted {
        Converted {
            backend: Backend::Brew,
            package: "x".into(),
            name: "x".into(),
            version: version.into(),
            repo: "o/r".into(),
            tag: tag.map(str::to_string),
            kind: PackageKind::Auto,
            bins: Vec::new(),
            artifacts: BTreeMap::new(),
        }
    }

    #[rstest::rstest]
    #[case(
        "ripgrep-15.2.0-x86_64-pc-windows-msvc.zip",
        Some("15.2.0"),
        "15.2.0",
        "ripgrep-*-x86_64-pc-windows-msvc.zip"
    )]
    #[case(
        "lazygit_0.65.1_Windows_x86_64.zip",
        Some("v0.65.1"),
        "0.65.1",
        "lazygit_*_Windows_x86_64.zip"
    )]
    #[case(
        "codex-package-aarch64-apple-darwin.tar.gz",
        Some("rust-v0.160.0"),
        "0.160.0",
        "codex-package-aarch64-apple-darwin.tar.gz"
    )]
    #[case(
        "WezTerm-macos-20240203-110809-5046fc22.zip",
        Some("20240203-110809-5046fc22"),
        "20240203-110809,5046fc22",
        "WezTerm-macos-*.zip"
    )]
    #[case("Hackintool.zip", Some("4.1.5"), "4.1.5", "Hackintool.zip")]
    #[case(
        "anki-26.09.3-mac-apple.dmg",
        Some("26.09.3"),
        "26.09.3",
        "anki-*-mac-apple.dmg"
    )]
    #[case("tool-v1-x86_64.tar.gz", Some("v1"), "1", "tool-v1-x86_64.tar.gz")]
    fn the_version_in_an_asset_name_becomes_a_wildcard(
        #[case] file: &str,
        #[case] tag: Option<&str>,
        #[case] version: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(asset_glob(file, &converted(tag, version)), expected);
    }

    fn download(t: TargetSpec, url: &str) -> Download {
        Download {
            target: t,
            url: url.into(),
            sha256: None,
        }
    }

    #[test]
    fn one_download_off_github_rejects_the_whole_package() {
        let verdict = assemble(
            Backend::Winget,
            "X.Y",
            "x",
            "1.0",
            PackageKind::Auto,
            Vec::new(),
            vec![
                download(
                    target(Os::Windows, Arch::X86_64),
                    "https://github.com/o/r/releases/download/v1.0/r-x64.zip",
                ),
                download(
                    target(Os::Windows, Arch::Aarch64),
                    "https://cdn.example.com/r-arm64.zip",
                ),
            ],
        );
        assert_eq!(verdict, Err(Rejected::NotGithubReleases));
    }

    #[test]
    fn downloads_from_two_repositories_are_not_one_source() {
        let verdict = assemble(
            Backend::Winget,
            "X.Y",
            "x",
            "1.0",
            PackageKind::Auto,
            Vec::new(),
            vec![
                download(
                    target(Os::Windows, Arch::X86_64),
                    "https://github.com/o/r/releases/download/v1.0/r-x64.zip",
                ),
                download(
                    target(Os::Windows, Arch::Aarch64),
                    "https://github.com/fork/r/releases/download/v1.0/r-arm64.zip",
                ),
            ],
        );
        assert_eq!(verdict, Err(Rejected::NotGithubReleases));
    }

    #[test]
    fn downloads_from_two_releases_are_refused_with_both_tags() {
        let verdict = assemble(
            Backend::Brew,
            "x",
            "x",
            "1.0",
            PackageKind::Auto,
            Vec::new(),
            vec![
                download(
                    target(Os::MacOs, Arch::Aarch64),
                    "https://github.com/o/r/releases/download/v1.1/r-arm64.zip",
                ),
                download(
                    target(Os::MacOs, Arch::X86_64),
                    "https://github.com/o/r/releases/download/v1.0/r-x64.zip",
                ),
            ],
        );
        let Err(Rejected::Unsupported(why)) = verdict else {
            panic!("{verdict:?}")
        };
        assert!(why.contains("v1.0, v1.1"), "{why}");
    }

    #[test]
    fn a_foreign_package_format_is_refused_by_name() {
        let verdict = assemble(
            Backend::Winget,
            "X.Y",
            "x",
            "1.0",
            PackageKind::Auto,
            Vec::new(),
            vec![download(
                target(Os::Windows, Arch::X86_64),
                "https://github.com/o/r/releases/download/v1.0/r-1.0-x64.msi",
            )],
        );
        let Err(Rejected::Unsupported(why)) = verdict else {
            panic!("{verdict:?}")
        };
        assert!(why.contains("`.msi`"), "{why}");
    }

    #[test]
    fn a_checksum_that_is_not_sha256_is_dropped_not_trusted() {
        let mut d = download(
            target(Os::Linux, Arch::X86_64),
            "https://github.com/o/r/releases/download/v1.0/r-linux-x86_64.tar.gz",
        );
        d.sha256 = Some("SKIP".into());
        let c = assemble(
            Backend::Linux,
            "r-bin",
            "x",
            "1.0",
            PackageKind::Auto,
            Vec::new(),
            vec![d],
        )
        .unwrap();
        assert_eq!(c.artifacts["linux-x86_64"].sha256, None);
    }

    #[test]
    fn the_hard_rule_message_is_exact() {
        assert_eq!(
            Rejected::NotGithubReleases
                .into_error("google-chrome")
                .to_string(),
            "google-chrome can't be converted: it is not distributed through GitHub Releases, \
             and that is not supported yet."
        );
    }

    #[rstest::rstest]
    #[case("BurntSushi.ripgrep.MSVC", true)]
    #[case("python@3.12", true)]
    #[case("gtk+3", true)]
    #[case("../etc", false)]
    #[case("a b", false)]
    #[case("a/b", false)]
    #[case("", false)]
    #[case("-rf", false)]
    #[case("x?y=1", false)]
    fn a_name_is_checked_before_it_reaches_a_url(#[case] name: &str, #[case] ok: bool) {
        assert_eq!(check_name(name).is_ok(), ok, "{name}");
    }

    #[test]
    fn a_bin_entry_carries_a_path_only_when_the_file_is_named_otherwise() {
        let same = bin_entry("rg", Some("ripgrep-15.2.0-x86_64-pc-windows-msvc\\rg.exe")).unwrap();
        assert_eq!(same.name.as_deref(), Some("rg"));
        assert_eq!(same.path, None);
        let other = bin_entry("tool", Some("bin/tool-cli")).unwrap();
        assert_eq!(other.path.as_deref(), Some("*tool-cli"));
        let exe = bin_entry("tool.exe", Some("tool-1.0\\Tool.EXE")).unwrap();
        assert_eq!(exe.name.as_deref(), Some("tool"));
        assert_eq!(exe.path, None);
        assert!(bin_entry("  ", None).is_none());
    }
}
