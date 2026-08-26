//! Shared domain types.
//!
//! Everything crossing a module boundary is defined here so sources, platforms,
//! extractors and commands agree on shapes without depending on each other.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Target
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    MacOs,
    Linux,
    Windows,
}

impl Os {
    /// Filename tokens that indicate this OS. Order is not significant.
    pub fn tokens(self) -> &'static [&'static str] {
        match self {
            Os::MacOs => &["darwin", "macos", "mac", "osx", "apple", "macosx"],
            Os::Linux => &["linux", "gnu", "musl"],
            Os::Windows => &["windows", "win32", "win64", "win", "msvc"],
        }
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Os::MacOs => "macos",
            Os::Linux => "linux",
            Os::Windows => "windows",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    Aarch64,
    X86_64,
    /// A fat binary that runs on any architecture of the host OS.
    Universal,
}

impl Arch {
    pub fn tokens(self) -> &'static [&'static str] {
        match self {
            Arch::Aarch64 => &["aarch64", "arm64", "armv8", "apple-silicon", "silicon", "m1"],
            Arch::X86_64 => &["x86_64", "x8664", "amd64", "x64", "intel", "64bit"],
            Arch::Universal => &["universal", "universal2", "fat", "all"],
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Arch::Aarch64 => "aarch64",
            Arch::X86_64 => "x86_64",
            Arch::Universal => "universal",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TargetSpec {
    pub os: Os,
    pub arch: Arch,
}

impl TargetSpec {
    /// The machine we are running on right now.
    pub fn host() -> Self {
        let os = if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else {
            Os::Linux
        };
        let arch = if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            Arch::X86_64
        };
        TargetSpec { os, arch }
    }
}

impl fmt::Display for TargetSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.os, self.arch)
    }
}

// ---------------------------------------------------------------------------
// Package identity
// ---------------------------------------------------------------------------

/// A fully-qualified package location: which source, and an id that source
/// understands. For GitHub the id is `owner/repo`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PackageRef {
    pub scheme: String,
    pub id: String,
}

impl PackageRef {
    pub fn new(scheme: impl Into<String>, id: impl Into<String>) -> Self {
        PackageRef {
            scheme: scheme.into(),
            id: id.into(),
        }
    }

    pub fn github(id: impl Into<String>) -> Self {
        PackageRef::new("github", id)
    }

    /// Parse `scheme:id` or a bare `owner/repo` (which implies GitHub).
    ///
    /// A bare word with neither `:` nor `/` is *not* a reference — it is an
    /// alias to be resolved against the manifest registry, so this returns
    /// `None` for it rather than guessing.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        // A scheme is alphanumeric and never contains `/`; this keeps
        // `https://host/x` and `owner/repo` from being read as schemes.
        if let Some((scheme, rest)) = text.split_once(':') {
            let looks_like_scheme = !scheme.is_empty()
                && !scheme.contains('/')
                && scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
            if looks_like_scheme && !rest.is_empty() {
                return Some(PackageRef::new(scheme.to_ascii_lowercase(), rest));
            }
        }
        if text.contains('/') {
            return Some(PackageRef::github(text));
        }
        None
    }

    /// Last path segment — the natural default package name.
    pub fn short_name(&self) -> &str {
        self.id.rsplit('/').next().unwrap_or(&self.id)
    }
}

impl fmt::Display for PackageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.scheme, self.id)
    }
}

/// Which version the user asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum VersionSpec {
    #[default]
    Latest,
    /// An exact tag or version string, matched with and without a `v` prefix.
    Exact(String),
}

impl fmt::Display for VersionSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionSpec::Latest => f.write_str("latest"),
            VersionSpec::Exact(v) => f.write_str(v),
        }
    }
}

/// Raw user input for a package: `ripgrep`, `BurntSushi/ripgrep@14.1.0`,
/// `github:cli/cli`, `myplugin:some-id@2.0`.
#[derive(Debug, Clone)]
pub struct PackageSpec {
    pub raw: String,
    /// Set when the input names a source explicitly or looks like `owner/repo`.
    pub reference: Option<PackageRef>,
    /// Set when the input is a bare name to look up in the registry.
    pub alias: Option<String>,
    pub version: VersionSpec,
}

impl PackageSpec {
    pub fn parse(input: &str) -> Self {
        let raw = input.trim().to_string();
        // Split the version off at the last `@` that follows the final `/`, so
        // scoped ids keep working and `owner/repo@v1` splits correctly.
        let split_from = raw.rfind('/').map(|i| i + 1).unwrap_or(0);
        let (body, version) = match raw[split_from..].find('@') {
            Some(rel) if rel > 0 => {
                let at = split_from + rel;
                (
                    raw[..at].to_string(),
                    VersionSpec::Exact(raw[at + 1..].to_string()),
                )
            }
            _ => (raw.clone(), VersionSpec::Latest),
        };
        let reference = PackageRef::parse(&body);
        let alias = if reference.is_none() {
            Some(body.to_ascii_lowercase())
        } else {
            None
        };
        PackageSpec {
            raw,
            reference,
            alias,
            version,
        }
    }

    /// Best available human label before a manifest is resolved.
    pub fn label(&self) -> String {
        match (&self.alias, &self.reference) {
            (Some(a), _) => a.clone(),
            (_, Some(r)) => r.short_name().to_string(),
            _ => self.raw.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// Versions
// ---------------------------------------------------------------------------

/// A version string that orders like semver when it can, and like a human
/// reading digits when it cannot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub raw: String,
    pub sem: Option<semver::Version>,
}

impl Version {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let core = trimmed.trim_start_matches(['v', 'V']);
        let sem = semver::Version::parse(core)
            .ok()
            .or_else(|| relaxed_semver(core));
        Version {
            raw: trimmed.to_string(),
            sem,
        }
    }

    /// True when this is a prerelease according to semver metadata.
    pub fn is_prerelease(&self) -> bool {
        self.sem.as_ref().is_some_and(|s| !s.pre.is_empty())
    }

    /// Compare ignoring a leading `v`, for matching a user-supplied tag.
    pub fn matches_request(&self, requested: &str) -> bool {
        let a = self.raw.trim_start_matches(['v', 'V']);
        let b = requested.trim().trim_start_matches(['v', 'V']);
        a.eq_ignore_ascii_case(b)
    }
}

/// Accept `1`, `1.2`, and `1.2.3.4` by padding or truncating to three parts.
fn relaxed_semver(text: &str) -> Option<semver::Version> {
    let head: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if head.is_empty() {
        return None;
    }
    let tail = &text[head.len()..];
    let mut parts: Vec<&str> = head.trim_end_matches('.').split('.').collect();
    parts.retain(|p| !p.is_empty());
    if parts.is_empty() {
        return None;
    }
    parts.truncate(3);
    while parts.len() < 3 {
        parts.push("0");
    }
    let base = parts.join(".");
    let suffix = tail.trim_start_matches(['-', '_', '+']);
    let candidate = if suffix.is_empty() {
        base
    } else {
        // Normalise separators semver rejects inside a prerelease tag.
        let cleaned: String = suffix
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '.' })
            .collect();
        format!("{base}-{}", cleaned.trim_matches('.'))
    };
    semver::Version::parse(&candidate).ok()
}

/// Compare strings the way a person reads them: digit runs numerically,
/// everything else lexicographically.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let mut xs = String::new();
                    let mut ys = String::new();
                    while ai.peek().is_some_and(|c| c.is_ascii_digit()) {
                        xs.push(ai.next().unwrap());
                    }
                    while bi.peek().is_some_and(|c| c.is_ascii_digit()) {
                        ys.push(bi.next().unwrap());
                    }
                    let xn: u128 = xs.trim_start_matches('0').parse().unwrap_or(0);
                    let yn: u128 = ys.trim_start_matches('0').parse().unwrap_or(0);
                    match xn.cmp(&yn) {
                        Ordering::Equal => continue,
                        other => return other,
                    }
                } else {
                    ai.next();
                    bi.next();
                    match x.to_ascii_lowercase().cmp(&y.to_ascii_lowercase()) {
                        Ordering::Equal => continue,
                        other => return other,
                    }
                }
            }
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match (&self.sem, &other.sem) {
            (Some(a), Some(b)) => a.cmp(b),
            _ => natural_cmp(&self.raw, &other.raw),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(d).map_err(de::Error::custom)?;
        Ok(Version::parse(&raw))
    }
}

// ---------------------------------------------------------------------------
// Releases
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checksum {
    /// Lowercase algorithm name, currently always `sha256`.
    pub algo: String,
    pub hex: String,
}

impl Checksum {
    pub fn sha256(hex: impl Into<String>) -> Self {
        Checksum {
            algo: "sha256".into(),
            hex: hex.into().to_ascii_lowercase(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub content_type: Option<String>,
    /// Checksum published by the source itself, when it offers one.
    #[serde(default)]
    pub digest: Option<Checksum>,
    /// Extra headers a source (usually a plugin) needs for the download.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

/// Repository-level metadata, used by `info` and `search`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub stars: Option<u64>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub archived: bool,
}

// ---------------------------------------------------------------------------
// Manifests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    /// Decide from what the payload actually contains.
    #[default]
    Auto,
    /// Command-line executables linked into the bin dir.
    Binary,
    /// A macOS `.app` bundle placed in the applications dir.
    App,
}

/// Which release asset to pick. Empty means "let the platform decide".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssetSelector {
    /// Asset must match at least one of these (glob: `*` and `?`).
    #[serde(default)]
    pub include: Vec<String>,
    /// Asset must match none of these.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Per-target override, keyed by `TargetSpec` display form, e.g.
    /// `"macos-aarch64" = "*-aarch64-apple-darwin.tar.gz"`.
    #[serde(default)]
    pub target: BTreeMap<String, String>,
}

impl AssetSelector {
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty() && self.target.is_empty()
    }
}

/// One executable to expose on PATH.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinSpec {
    /// Path inside the extracted payload. Globs allowed. When absent, ketch
    /// discovers executables automatically.
    #[serde(default)]
    pub path: Option<String>,
    /// Name of the symlink. Defaults to the file name of `path`.
    #[serde(default)]
    pub name: Option<String>,
}

/// How to install one package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub source: PackageRef,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub kind: PackageKind,
    #[serde(default)]
    pub asset: AssetSelector,
    #[serde(default)]
    pub bin: Vec<BinSpec>,
    /// Leading path components to drop when extracting.
    #[serde(default)]
    pub strip_prefix: Option<usize>,
    /// Consider prereleases when resolving `latest`.
    #[serde(default)]
    pub prerelease: bool,
    /// Alternate names this package answers to.
    #[serde(default)]
    pub provides: Vec<String>,
    /// Printed after a successful install.
    #[serde(default)]
    pub notes: Option<String>,
    /// Files to expose as man pages / completions later; recorded now so
    /// manifests written today stay valid.
    #[serde(default)]
    pub extra_paths: Vec<String>,
}

impl Manifest {
    /// The manifest ketch uses when nobody wrote one: everything inferred.
    pub fn inferred(source: PackageRef) -> Self {
        Manifest {
            name: normalize_name(source.short_name()),
            source,
            description: None,
            homepage: None,
            kind: PackageKind::Auto,
            asset: AssetSelector::default(),
            bin: Vec::new(),
            strip_prefix: None,
            prerelease: false,
            provides: Vec::new(),
            notes: None,
            extra_paths: Vec::new(),
        }
    }
}

/// Lowercase a package name and strip decoration people put in repo names.
pub fn normalize_name(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    let stripped = lower
        .strip_suffix(".rs")
        .or_else(|| lower.strip_suffix(".git"))
        .unwrap_or(&lower);
    stripped.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestOrigin {
    /// Shipped inside the ketch binary.
    Builtin,
    /// A `.toml` in the user's manifest directory.
    User(PathBuf),
    /// Nobody wrote one; ketch guessed.
    Inferred,
}

// ---------------------------------------------------------------------------
// Installed state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// Symlink in the bin dir pointing into the store.
    Symlink,
    /// A `.app` copied out to the applications dir; removing it is a delete.
    CopiedApp,
    /// A `.app` symlinked into the applications dir.
    LinkedApp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRecord {
    /// The path we created and are responsible for removing.
    pub link: PathBuf,
    /// What it points at inside the store.
    pub target: PathBuf,
    pub kind: LinkKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: Version,
    pub source: PackageRef,
    pub tag: String,
    pub target: TargetSpec,
    pub asset_name: String,
    /// SHA-256 of the downloaded asset, always recorded.
    pub sha256: String,
    /// True when the checksum was published by the source rather than
    /// trusted on first use.
    #[serde(default)]
    pub checksum_verified: bool,
    pub installed_at: u64,
    /// Store directory holding the extracted payload.
    pub prefix: PathBuf,
    #[serde(default)]
    pub links: Vec<LinkRecord>,
    #[serde(default)]
    pub pinned: bool,
    pub origin: ManifestOrigin,
    /// Kept so `upgrade` reuses the same selection rules as `install`.
    #[serde(default)]
    pub manifest: Option<Manifest>,
}

impl InstalledPackage {
    pub fn binaries(&self) -> impl Iterator<Item = &LinkRecord> {
        self.links.iter().filter(|l| l.kind == LinkKind::Symlink)
    }
}

/// Seconds since the Unix epoch, saturating at 0 on a broken clock.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Minimal glob: `*` matches any run, `?` matches one character. Case
/// insensitive, because release asset naming is not consistent about case.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_ascii_lowercase().chars().collect();
    let t: Vec<char> = text.to_ascii_lowercase().chars().collect();
    // Iterative backtracking keeps this linear in the common case and avoids
    // the exponential blowup a naive recursive matcher has on `*a*a*a*`.
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (usize::MAX, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = pi;
            mark = ti;
            pi += 1;
        } else if star != usize::MAX {
            pi = star + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_repo_as_github() {
        let r = PackageRef::parse("BurntSushi/ripgrep").unwrap();
        assert_eq!(r.scheme, "github");
        assert_eq!(r.id, "BurntSushi/ripgrep");
        assert_eq!(r.short_name(), "ripgrep");
    }

    #[test]
    fn bare_word_is_an_alias_not_a_ref() {
        assert!(PackageRef::parse("ripgrep").is_none());
        let spec = PackageSpec::parse("ripgrep");
        assert_eq!(spec.alias.as_deref(), Some("ripgrep"));
    }

    #[test]
    fn parses_explicit_scheme() {
        let r = PackageRef::parse("gitlab:group/proj").unwrap();
        assert_eq!(r.scheme, "gitlab");
        assert_eq!(r.id, "group/proj");
    }

    #[test]
    fn splits_version_after_last_slash_only() {
        let s = PackageSpec::parse("BurntSushi/ripgrep@14.1.0");
        assert_eq!(s.reference.unwrap().id, "BurntSushi/ripgrep");
        assert_eq!(s.version, VersionSpec::Exact("14.1.0".into()));

        let s = PackageSpec::parse("cli/cli");
        assert_eq!(s.version, VersionSpec::Latest);
    }

    #[test]
    fn versions_order_by_semver_then_naturally() {
        assert!(Version::parse("v1.10.0") > Version::parse("v1.9.0"));
        assert!(Version::parse("2024.10.1") > Version::parse("2024.9.30"));
        assert!(Version::parse("1.0.0") > Version::parse("1.0.0-beta.1"));
        // No digits at all: fall back to natural comparison.
        assert!(Version::parse("nightly-b") > Version::parse("nightly-a"));
    }

    #[test]
    fn relaxed_versions_parse() {
        assert!(Version::parse("v1.2").sem.is_some());
        assert!(Version::parse("3").sem.is_some());
        assert!(Version::parse("1.2.3.4").sem.is_some());
    }

    #[test]
    fn tag_matching_ignores_v_prefix() {
        assert!(Version::parse("v14.1.0").matches_request("14.1.0"));
        assert!(Version::parse("14.1.0").matches_request("v14.1.0"));
        assert!(!Version::parse("14.1.0").matches_request("14.1.1"));
    }

    #[test]
    fn glob_matches_asset_names() {
        assert!(glob_match("*-aarch64-apple-darwin.tar.gz", "rg-14-aarch64-apple-darwin.tar.gz"));
        assert!(glob_match("*.zip", "Tool-Universal.ZIP"));
        assert!(!glob_match("*.zip", "tool.tar.gz"));
        assert!(glob_match("rg?.tar.gz", "rg1.tar.gz"));
        // Pathological pattern must still terminate promptly.
        assert!(!glob_match("*a*a*a*a*b", &"a".repeat(64)));
    }
}
