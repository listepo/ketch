// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Shared domain types.
//!
//! Everything crossing a module boundary is defined here so sources, platforms,
//! extractors and commands agree on shapes without depending on each other.

use crate::error::{Error, Result};
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Target
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
// `MacOs` reads as `Os::MacOs` at every call site, which is the point.
#[allow(clippy::enum_variant_names)]
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
            Arch::Aarch64 => &[
                "aarch64",
                "arm64",
                "armv8",
                "apple-silicon",
                "silicon",
                "m1",
            ],
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

/// The inverse of the `Display` form, for values that arrive as text — a
/// lockfile entry, a command line.
impl std::str::FromStr for TargetSpec {
    type Err = String;

    fn from_str(text: &str) -> std::result::Result<Self, Self::Err> {
        let unknown = || format!("`{text}` is not a target; expected `<os>-<arch>`");
        let (os, arch) = text.split_once('-').ok_or_else(unknown)?;
        let os = match os {
            "macos" => Os::MacOs,
            "linux" => Os::Linux,
            "windows" => Os::Windows,
            _ => return Err(unknown()),
        };
        let arch = match arch {
            "aarch64" => Arch::Aarch64,
            "x86_64" => Arch::X86_64,
            "universal" => Arch::Universal,
            _ => return Err(unknown()),
        };
        Ok(TargetSpec { os, arch })
    }
}

// ---------------------------------------------------------------------------
// Package identity
// ---------------------------------------------------------------------------

/// A fully-qualified package location: which source, and an id that source
/// understands. For GitHub the id is `owner/repo`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
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
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-');
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

/// `PackageRef` is written as the `scheme:id` string everywhere it is stored,
/// so manifests and the state file read the same way a user would type it.
impl TryFrom<String> for PackageRef {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, Self::Error> {
        PackageRef::parse(&text).ok_or_else(|| {
            format!("`{text}` is not a package reference; expected `scheme:id` or `owner/repo`")
        })
    }
}

impl From<PackageRef> for String {
    fn from(value: PackageRef) -> Self {
        value.to_string()
    }
}

impl fmt::Display for PackageRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.scheme, self.id)
    }
}

/// The schema describes the string [`PackageRef::parse`] accepts, not the two
/// fields it parses into: the file holds the string.
#[cfg(test)]
impl schemars::JsonSchema for PackageRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PackageRef".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "`scheme:id` (`github:BurntSushi/ripgrep`, `myplugin:some-id`), \
                            or a bare `owner/repo`, which means GitHub.",
            "type": "string",
            "pattern": schema_pattern::PACKAGE_REF,
        })
    }
}

/// Regular expressions for the JSON Schema of a manifest, each the twin of a
/// check in Rust that `tests::schema_patterns_agree_with_the_rust_checks`
/// holds them to.
///
/// They are written in the subset ECMA-262 and the `regex` crate read alike,
/// because a JSON Schema `pattern` is ECMA-262 and an editor's validator may
/// be either. That is also why whitespace is a spelled-out class rather than
/// `\s`: ECMA-262's `\s` has U+FEFF and lacks U+0085, while `str::trim` and
/// `char::is_whitespace` go by Unicode `White_Space`.
#[cfg(test)]
mod schema_pattern {
    /// Unicode `White_Space`, as the body of a character class.
    macro_rules! white_space {
        () => {
            r"\x09-\x0d \x85\xa0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000"
        };
    }

    /// What `config::sanitize_component` rewrites wherever it appears: path
    /// separators, `:`, control characters and bidi overrides.
    macro_rules! unsafe_in_name {
        () => {
            r"/\\:\x00-\x1f\x7f-\x9f\u202a-\u202e\u2066-\u2069"
        };
    }

    /// [`super::PackageRef::parse`]: a scheme of letters, digits and `-`
    /// before the first `:` with something after it, or anything with a `/`.
    pub const PACKAGE_REF: &str = concat!(
        "^[",
        white_space!(),
        r"]*[A-Za-z0-9-]+:[\s\S]*[^",
        white_space!(),
        "]|/"
    );

    /// `usable_file_name`: nothing `sanitize_component` would rewrite, and no
    /// `.`, space or `-` at either end, which it would trim.
    pub const FILE_NAME: &str = concat!(
        "^[^",
        unsafe_in_name!(),
        r". \-]([^",
        unsafe_in_name!(),
        "]*[^",
        unsafe_in_name!(),
        r". \-])?$"
    );

    /// A `provides` alias: not empty, no whitespace anywhere.
    pub const ALIAS: &str = concat!("^[^", white_space!(), "]+$");

    /// A hook command: something besides whitespace.
    pub const NOT_BLANK: &str = concat!("[^", white_space!(), "]");
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
    /// Part of the public surface, with no reader in the tree yet.
    #[allow(dead_code)]
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
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
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
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' {
                    c
                } else {
                    '.'
                }
            })
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
        // Leading `v`/`V` is tag noise, not part of the version. Semver already
        // strips it at parse time; the raw fallback must too, or `0.4.0` and
        // `v0.4.0` look unequal after an Equal semver cmp and self-update
        // claims a fake upgrade.
        let left = self.raw.trim_start_matches(['v', 'V']);
        let right = other.raw.trim_start_matches(['v', 'V']);
        match (&self.sem, &other.sem) {
            // Relaxing to semver is lossy: `1.2.3.4` and `1.2.3.5` both become
            // `1.2.3`, and semver ignores build metadata outright. Falling back
            // to the raw strings keeps two genuinely different releases from
            // comparing equal, which would leave `max_by` picking whichever it
            // happened to see first — sometimes the older one.
            (Some(a), Some(b)) => a.cmp(b).then_with(|| natural_cmp(left, right)),
            _ => natural_cmp(left, right),
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
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
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
#[cfg_attr(test, derive(schemars::JsonSchema))]
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
#[cfg_attr(test, derive(schemars::JsonSchema))]
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
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty() && self.target.is_empty()
    }
}

/// One executable to expose on PATH.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
// `Manifest::validate` refuses an entry with neither key.
#[cfg_attr(
    test,
    schemars(extend("anyOf" = [{ "required": ["name"] }, { "required": ["path"] }]))
)]
pub struct BinSpec {
    /// Path inside the extracted payload. Globs allowed; when a glob matches
    /// several files, the one whose stem is `name` is linked. When absent,
    /// ketch discovers executables automatically.
    #[serde(default)]
    pub path: Option<String>,
    /// Name of the symlink. Defaults to the file name of `path`.
    #[serde(default)]
    #[cfg_attr(test, schemars(pattern(schema_pattern::FILE_NAME)))]
    pub name: Option<String>,
}

/// What an `extra_paths` entry is for. Required in the table form; inferred
/// from path rules when the entry is a bare string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExtraKind {
    /// A man page, linked into the user man root.
    Man,
    /// A shell completion script, linked into that shell's user directory.
    Completion,
}

/// Shell whose user completion directory an extra path should land in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Elvish,
    Powershell,
}

impl CompletionShell {
    /// Shells ketch will generate and install completions for.
    pub const ALL: [Self; 5] = [
        Self::Bash,
        Self::Zsh,
        Self::Fish,
        Self::Elvish,
        Self::Powershell,
    ];

    /// The `clap_complete` shell this maps to. Fig is intentionally absent:
    /// it is not a user directory ketch should write into.
    pub fn to_clap(self) -> clap_complete::Shell {
        match self {
            Self::Bash => clap_complete::Shell::Bash,
            Self::Zsh => clap_complete::Shell::Zsh,
            Self::Fish => clap_complete::Shell::Fish,
            Self::Elvish => clap_complete::Shell::Elvish,
            Self::Powershell => clap_complete::Shell::PowerShell,
        }
    }

    /// Inverse of [`Self::to_clap`]. `None` for shells ketch will not install.
    pub fn from_clap(shell: clap_complete::Shell) -> Option<Self> {
        match shell {
            clap_complete::Shell::Bash => Some(Self::Bash),
            clap_complete::Shell::Zsh => Some(Self::Zsh),
            clap_complete::Shell::Fish => Some(Self::Fish),
            clap_complete::Shell::Elvish => Some(Self::Elvish),
            clap_complete::Shell::PowerShell => Some(Self::Powershell),
            _ => None,
        }
    }

    /// Stable name used in doctor checks and error text.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Zsh => "zsh",
            Self::Fish => "fish",
            Self::Elvish => "elvish",
            Self::Powershell => "powershell",
        }
    }
}

/// Table form of `extra_paths`: an explicit kind, so path rules are not guessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ExtraPathSpec {
    /// Path inside the extracted payload. Same containment rules as a string entry.
    pub path: String,
    pub kind: ExtraKind,
    /// Required for a completion when the file name does not name a shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<CompletionShell>,
    /// Man section (`1`, `8`, `1p`). Inferred from a `*.N` file name when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

/// One `extra_paths` entry: a payload-relative string, or a table with `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ExtraPath {
    /// Classified by the path rules in `docs/MANIFESTS.md`.
    Path(String),
    /// Explicit kind; path rules are not consulted except to fill `shell`/`section`.
    Spec(ExtraPathSpec),
}

impl ExtraPath {
    /// The payload-relative path, whether the entry was a string or a table.
    pub fn as_rel_path(&self) -> &str {
        match self {
            Self::Path(path) => path,
            Self::Spec(spec) => &spec.path,
        }
    }
}

/// An `extra_paths` entry after classification, before a destination is chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedExtra {
    pub rel_path: String,
    pub kind: ExtraKind,
    pub shell: Option<CompletionShell>,
    pub section: Option<String>,
}

/// Commands a manifest runs around its own install, update and uninstall.
///
/// Each is one line for the platform shell. `crate::hooks` runs them, and
/// only from a manifest in the user's own manifest directory: anywhere else
/// the manifest is someone else's file, and a command in it is their code.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, default)]
pub struct Hooks {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(pattern(schema_pattern::NOT_BLANK)))]
    pub before_install: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(pattern(schema_pattern::NOT_BLANK)))]
    pub after_install: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(pattern(schema_pattern::NOT_BLANK)))]
    pub before_update: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(pattern(schema_pattern::NOT_BLANK)))]
    pub after_update: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(pattern(schema_pattern::NOT_BLANK)))]
    pub before_uninstall: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, schemars(pattern(schema_pattern::NOT_BLANK)))]
    pub after_uninstall: Option<String>,
}

impl Hooks {
    /// Every hook with its manifest key, set or not.
    pub fn entries(&self) -> [(&'static str, Option<&str>); 6] {
        [
            ("before_install", self.before_install.as_deref()),
            ("after_install", self.after_install.as_deref()),
            ("before_update", self.before_update.as_deref()),
            ("after_update", self.after_update.as_deref()),
            ("before_uninstall", self.before_uninstall.as_deref()),
            ("after_uninstall", self.after_uninstall.as_deref()),
        ]
    }

    /// True when no hook is set, so an empty table need not be written.
    pub fn is_empty(&self) -> bool {
        self.entries().iter().all(|(_, cmd)| cmd.is_none())
    }
}

/// How to install one package.
///
/// `deny_unknown_fields` is deliberate: a manifest is hand-written, often by
/// someone else, and a misspelt key that is silently ignored produces a package
/// that installs the wrong thing with no complaint anywhere.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(transform = name_from_folder))]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The install name: a directory in the store and the key in
    /// `state.json`. A registry package folder supplies it; elsewhere it is
    /// required.
    #[cfg_attr(test, schemars(pattern(schema_pattern::FILE_NAME)))]
    pub name: String,
    /// Where releases come from.
    pub source: PackageRef,
    /// One line, shown by `ketch info` and `ketch search`.
    #[serde(default)]
    pub description: Option<String>,
    /// A URL for humans.
    #[serde(default)]
    pub homepage: Option<String>,
    /// What the payload is.
    #[serde(default)]
    pub kind: PackageKind,
    /// Narrows which release asset is chosen.
    #[serde(default)]
    pub asset: AssetSelector,
    /// Which executables to link, and under what names. Empty means discover.
    #[serde(default)]
    pub bin: Vec<BinSpec>,
    /// Leading path components to drop when extracting.
    #[serde(default)]
    #[cfg_attr(test, schemars(range(max = MAX_STRIP_PREFIX)))]
    pub strip_prefix: Option<usize>,
    /// Consider prereleases when resolving `latest`.
    #[serde(default)]
    pub prerelease: bool,
    /// Alternate names this package answers to.
    #[serde(default)]
    #[cfg_attr(test, schemars(inner(pattern(schema_pattern::ALIAS))))]
    pub provides: Vec<String>,
    /// Printed after a successful install.
    #[serde(default)]
    pub notes: Option<String>,
    /// Man pages and completions to expose from the payload. A string is
    /// classified by the path rules in `docs/MANIFESTS.md`; a table sets `kind`.
    #[serde(default)]
    pub extra_paths: Vec<ExtraPath>,
    /// Whose signature a release must carry. See `crate::trust`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<TrustPolicy>,
    /// Commands to run around install, update and uninstall. See `crate::hooks`.
    #[serde(default, skip_serializing_if = "Hooks::is_empty")]
    pub hooks: Hooks,
}

impl Manifest {
    /// Check what serde cannot: that the names in this manifest are usable.
    ///
    /// Two of them become paths — `name` is a directory in the store and each
    /// `bin.name` is a link in the bin directory — so this is the trust
    /// boundary between a manifest ketch did not write and the user's disk.
    /// Names that would need sanitising are refused rather than rewritten: a
    /// package that installs somewhere other than where it says is worse than
    /// one that refuses to install.
    pub fn validate(&self) -> Result<()> {
        usable_file_name("package name", &self.name)?;
        for spec in &self.bin {
            if let Some(name) = &spec.name {
                usable_file_name("binary name", name)?;
            }
            if let Some(path) = &spec.path {
                contained_path("binary path", path)?;
            }
            if spec.name.is_none() && spec.path.is_none() {
                return Err(Error::msg(
                    "a `bin` entry needs `name`, `path`, or both".to_string(),
                ));
            }
        }
        for path in &self.extra_paths {
            contained_path("extra path", path.as_rel_path())?;
            crate::extra::classify(path)?;
        }
        // Each level costs a directory listing of the payload, and no real
        // archive nests its wrapper directories this deep.
        if self.strip_prefix.is_some_and(|n| n > MAX_STRIP_PREFIX) {
            return Err(Error::msg(format!(
                "`strip_prefix` must be at most {MAX_STRIP_PREFIX}"
            )));
        }
        for alias in &self.provides {
            if alias.trim().is_empty() || alias.chars().any(char::is_whitespace) {
                return Err(Error::msg(format!(
                    "`{alias}` cannot be an alias: it is not something anyone can type"
                )));
            }
        }
        if let Some(trust) = &self.trust {
            // Both name files downloaded beside the asset. `{file}` is the
            // only placeholder, and the sidecar cannot be the signed file.
            if let Some(template) = &trust.signature {
                let sample = template.replace("{file}", "x");
                if sample.contains(['{', '}']) || sample == "x" {
                    return Err(Error::msg(format!(
                        "`trust.signature` `{}` must name a file beside the signed one, \
                         with `{{file}}` for that file's name",
                        template.escape_debug()
                    )));
                }
                usable_file_name("trust.signature", &sample)?;
            }
            if let Some(signed) = &trust.signed {
                usable_file_name("trust.signed", signed)?;
            }
            crate::trust::check_policy(trust)?;
        }
        // A blank hook would still spawn a shell and report success; a key
        // that is present and says nothing is a mistake worth naming.
        for (key, cmd) in self.hooks.entries() {
            if cmd.is_some_and(|c| c.trim().is_empty()) {
                return Err(Error::msg(format!("`hooks.{key}` is empty")));
            }
        }
        Ok(())
    }

    /// The manifest ketch uses when nobody wrote one: everything inferred.
    ///
    /// The name is sanitized rather than validated. It becomes a directory in
    /// the store and a key in the state file, and nobody authored it — so a
    /// reference whose last segment is unusable gets a usable name instead of
    /// failing an install the user had every right to expect to work.
    pub fn inferred(source: PackageRef) -> Self {
        Manifest {
            name: crate::config::sanitize_component(&normalize_name(source.short_name())),
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
            trust: None,
            hooks: Hooks::default(),
        }
    }
}

/// How many wrapper directories a manifest may ask to strip.
const MAX_STRIP_PREFIX: usize = 8;

/// Takes `name` out of the schema's `required`. The schema is for a
/// `ketch.toml`, and in a registry package folder or a project being pushed
/// the folder names the package; ketch fills `name` in before this type
/// reads the file.
#[cfg(test)]
fn name_from_folder(schema: &mut schemars::Schema) {
    if let Some(serde_json::Value::Array(required)) = schema.get_mut("required") {
        required.retain(|key| key != "name");
    }
}

/// Which kind of publisher signature a `trust` table asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Verifier {
    Sigstore,
    Minisign,
    Gpg,
}

impl Verifier {
    pub fn as_str(self) -> &'static str {
        match self {
            Verifier::Sigstore => "sigstore",
            Verifier::Minisign => "minisign",
            Verifier::Gpg => "gpg",
        }
    }

    /// The sidecar name each tool writes by default.
    fn default_signature(self) -> &'static str {
        match self {
            Verifier::Sigstore => "{file}.sigstore.json",
            Verifier::Minisign => "{file}.minisig",
            Verifier::Gpg => "{file}.asc",
        }
    }
}

impl std::fmt::Display for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What an unverifiable signature does to the install.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum TrustMode {
    /// Refuse it. Fail closed.
    #[default]
    Require,
    /// Install on the checksum alone, and say so.
    Warn,
}

/// A manifest's `trust` table: whose signature a release must carry.
///
/// Which keys a verifier reads is checked by `crate::trust::check_policy`,
/// called from [`Manifest::validate`]; `docs/MANIFESTS.md` is the reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TrustPolicy {
    pub verifier: Verifier,
    #[serde(default)]
    pub mode: TrustMode,
    /// The sidecar's name; `{file}` is the signed file's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// A glob naming a checksum list the signature covers instead of the asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed: Option<String>,
    /// sigstore: the OIDC issuer of the signing certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// sigstore: the certificate's exact identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// sigstore: any GitHub Actions workflow in this `owner/repo`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    /// minisign: the key. gpg: the armored key block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// gpg: the primary key's full fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
}

impl TrustPolicy {
    /// The sidecar that signs `file`.
    pub fn signature_name(&self, file: &str) -> String {
        self.signature
            .as_deref()
            .unwrap_or(self.verifier.default_signature())
            .replace("{file}", file)
    }
}

/// A publisher signature that verified, as `state.json` records it.
///
/// `identity` is what the manifest pinned, never what the signature file says
/// about itself: that text is the signer's own, and is not printed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub verifier: Verifier,
    pub identity: String,
    /// The sidecar release asset and its SHA-256, so the check can be redone.
    pub signature: String,
    pub signature_sha256: String,
    /// The checksum list the signature covered, when it covered that and
    /// not the asset itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed: Option<String>,
    /// sigstore: the Rekor transparency-log index of the entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_index: Option<u64>,
}

/// Reject a name that could not be used verbatim as one path component.
///
/// `sanitize_component` already knows every character that is unsafe here, so
/// asking whether it would change the value is the whole check.
fn usable_file_name(what: &str, value: &str) -> Result<()> {
    if crate::config::sanitize_component(value) == value {
        Ok(())
    } else {
        Err(Error::msg(format!(
            "{what} `{}` is not usable as a file name",
            value.escape_debug()
        )))
    }
}

/// Reject a path that would reach outside the payload it is relative to.
fn contained_path(what: &str, value: &str) -> Result<()> {
    crate::extract::safe_member_path(std::path::Path::new(value))
        .map(|_| ())
        .map_err(|_| Error::msg(format!("{what} `{value}` must stay inside the package")))
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
    /// A package folder in the fetched registry.
    Registry(PathBuf),
    /// A `.toml` in the user's manifest directory.
    User(PathBuf),
    /// Nobody wrote one; ketch guessed.
    Inferred,
}

impl ManifestOrigin {
    /// Precedence name: user, registry, builtin, inferred.
    pub fn tier(&self) -> &'static str {
        match self {
            ManifestOrigin::Builtin => "builtin",
            ManifestOrigin::Registry(_) => "registry",
            ManifestOrigin::User(_) => "user",
            ManifestOrigin::Inferred => "inferred",
        }
    }

    /// A path the user can open, or a short label when there is none.
    pub fn location(&self) -> String {
        match self {
            ManifestOrigin::Builtin => "builtin".into(),
            ManifestOrigin::Registry(path) | ManifestOrigin::User(path) => {
                path.display().to_string()
            }
            ManifestOrigin::Inferred => "inferred".into(),
        }
    }
}

/// How a `local:` package arrived on disk.
///
/// Optional on `InstalledPackage` with `#[serde(default)]` so state written
/// before local installs existed still deserialises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalKind {
    /// A recognised archive that was extracted into the store.
    Archive,
    /// A single executable file placed into the store and linked into bin.
    Binary,
    /// A symlink; payload bytes come from the resolved target.
    Symlink,
    /// A macOS `.app` bundle copied into the store and applications dir.
    App,
}

impl LocalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LocalKind::Archive => "archive",
            LocalKind::Binary => "binary",
            LocalKind::Symlink => "symlink",
            LocalKind::App => "app",
        }
    }
}

impl fmt::Display for LocalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Installed state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// Symlink in the bin dir pointing into the store.
    Symlink,
    /// A regular file copied into the bin dir. Windows cannot create unprivileged
    /// symlinks, so identity is the file's own bytes against the store copy.
    CopiedFile,
    /// A `.app` copied out to the applications dir; removing it is a delete.
    CopiedApp,
    /// A `.app` symlinked into the applications dir.
    LinkedApp,
}

/// What a recorded link is *for*. Mechanism stays in [`LinkKind`]; this is
/// how `binaries()` and uninstall tell a PATH entry from a man page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LinkRole {
    /// A binary on PATH. The default so state written before this field existed
    /// still reads as binaries.
    #[default]
    Binary,
    /// A man page in the user man root.
    Man,
    /// A shell completion script.
    Completion,
}

impl LinkRole {
    /// True for the default role, used to keep old-looking JSON for binaries.
    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRecord {
    /// The path we created and are responsible for removing.
    pub link: PathBuf,
    /// What it points at inside the store.
    pub target: PathBuf,
    pub kind: LinkKind,
    /// Absent on state written before man/completion links existed.
    #[serde(default, skip_serializing_if = "LinkRole::is_binary")]
    pub role: LinkRole,
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
    /// Present when `source` is `local:…`. Absent (default) on packages
    /// installed before local installs existed — still readable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_kind: Option<LocalKind>,
    /// Absolute path the user named, when it differs from `source.id` or when
    /// we want an explicit field for `info --json`. Usually mirrors the id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    /// Signature check recorded when this version was placed. Absent on
    /// packages installed before trust was persisted.
    #[serde(default, skip_serializing_if = "TrustResult::is_not_applicable")]
    pub trust: TrustResult,
    /// Older prefixes still on disk. Upgrade appends; `ketch prune` removes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retained: Vec<RetainedVersion>,
    /// The publisher signature verified when this version was placed, from
    /// the manifest's `trust` table. Absent when there was none to check,
    /// and on packages installed before signatures were checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    /// The file name of the binary picked when a release shipped several
    /// sharing the package's name and no manifest named one (B64). Reused on
    /// upgrade, reinstall, relink and rollback rather than asking again.
    /// Absent on packages that never needed the choice, and on state written
    /// before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bin_choice: Option<String>,
}

impl InstalledPackage {
    /// How far the publisher of this download is established: a verified
    /// signature, a published checksum, or nothing beyond first use.
    pub fn publisher_trust(&self) -> &'static str {
        if self.provenance.is_some() {
            "signed"
        } else if self.checksum_verified {
            "checksum"
        } else {
            "first use"
        }
    }

    pub fn binaries(&self) -> impl Iterator<Item = &LinkRecord> {
        self.links.iter().filter(|l| {
            l.role.is_binary() && matches!(l.kind, LinkKind::Symlink | LinkKind::CopiedFile)
        })
    }

    /// Most recently replaced version, if any is still recorded.
    pub fn previous_retained(&self) -> Option<&RetainedVersion> {
        self.retained.first()
    }

    /// Find a retained version by version string or tag.
    pub fn find_retained(&self, spec: &str) -> Option<(usize, &RetainedVersion)> {
        self.retained.iter().enumerate().find(|(_, r)| {
            r.version.matches_request(spec) || r.tag.eq_ignore_ascii_case(spec.trim())
        })
    }
}

/// How many previous prefixes `ketch prune` leaves on disk.
///
/// Upgrade never deletes a prefix. This number is applied only by the
/// explicit prune command, so a keep of 1 still leaves every version until
/// someone prunes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    #[serde(default = "default_retain_keep")]
    pub keep: u32,
}

pub fn default_retain_keep() -> u32 {
    1
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        RetentionPolicy {
            keep: default_retain_keep(),
        }
    }
}

/// A previously installed version whose prefix is still in the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetainedVersion {
    pub version: Version,
    pub prefix: PathBuf,
    /// SHA-256 of the asset that produced this prefix.
    pub sha256: String,
    #[serde(default)]
    pub checksum_verified: bool,
    #[serde(default)]
    pub links: Vec<LinkRecord>,
    #[serde(default, skip_serializing_if = "TrustResult::is_not_applicable")]
    pub trust: TrustResult,
    /// Carried so a rollback restores what was verified for this version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    pub tag: String,
    pub asset_name: String,
    #[serde(default)]
    pub installed_at: u64,
    pub target: TargetSpec,
}

impl RetainedVersion {
    /// Snapshot the version-specific fields of an installed package.
    pub fn from_installed(pkg: &InstalledPackage) -> Self {
        RetainedVersion {
            version: pkg.version.clone(),
            prefix: pkg.prefix.clone(),
            sha256: pkg.sha256.clone(),
            checksum_verified: pkg.checksum_verified,
            links: pkg.links.clone(),
            trust: pkg.trust.clone(),
            provenance: pkg.provenance.clone(),
            tag: pkg.tag.clone(),
            asset_name: pkg.asset_name.clone(),
            installed_at: pkg.installed_at,
            target: pkg.target,
        }
    }
}

/// Persisted form of a platform trust check. `NotApplicable` is the default
/// so state written before this field existed still loads.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustResult {
    Trusted {
        authority: String,
    },
    Weak {
        detail: String,
    },
    Untrusted {
        detail: String,
    },
    #[default]
    NotApplicable,
}

impl TrustResult {
    pub fn is_not_applicable(&self) -> bool {
        matches!(self, TrustResult::NotApplicable)
    }
}

impl fmt::Display for TrustResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustResult::Trusted { authority } => write!(f, "trusted ({authority})"),
            TrustResult::Weak { detail } => write!(f, "weak ({detail})"),
            TrustResult::Untrusted { detail } => write!(f, "untrusted ({detail})"),
            TrustResult::NotApplicable => f.write_str("not applicable"),
        }
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

/// The payload file to link when one `bin` glob matched several: the one whose
/// stem is the spec's link `name` — `rtok.exe` for `rtok*`, never the
/// `rtok-hook.exe` a release ships beside it. When several share that stem the
/// sorted-first one wins. With several matches and none named like the link,
/// the manifest is ambiguous and this refuses, listing the candidates sorted
/// and relative to `root` so the message reads the same on every OS: taking
/// the first match would link whatever the directory lists first, and NTFS
/// lists `rtok-hook.exe` ahead of `rtok.exe` where ext4 and APFS do not
/// (B62, B71).
pub fn glob_preferred<'a>(
    root: &Path,
    pattern: &str,
    matched: &[&'a Path],
    name: Option<&str>,
) -> Result<Option<&'a Path>> {
    let mut sorted: Vec<&'a Path> = matched.to_vec();
    sorted.sort();
    if sorted.len() <= 1 {
        return Ok(sorted.first().copied());
    }
    let want = name
        .and_then(|n| Path::new(n).file_stem())
        .map(|s| s.to_string_lossy().into_owned());
    let named = sorted.iter().copied().find(|p| {
        want.as_deref()
            .is_some_and(|w| p.file_stem().is_some_and(|s| s.eq_ignore_ascii_case(w)))
    });
    if named.is_some() {
        return Ok(named);
    }
    let listed = sorted
        .iter()
        .map(|p| {
            let rel = p.strip_prefix(root).unwrap_or(p);
            let parts: Vec<_> = rel.iter().map(|c| c.to_string_lossy()).collect();
            format!("  {}", parts.join("/"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    Err(Error::msg(format!(
        "the manifest's `bin` path `{pattern}` matches {} files and none is named like the link:\n\
         {listed}\n\
         set the entry's `name` to the one to link, or narrow its `path` to a single file",
        sorted.len()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_refuses_names_that_would_escape_their_directory() {
        let base = Manifest::inferred(PackageRef::github("a/b"));
        assert!(base.validate().is_ok());

        let bad_package = Manifest {
            name: "../evil".into(),
            ..base.clone()
        };
        assert!(bad_package.validate().is_err());

        let bad_link = Manifest {
            bin: vec![BinSpec {
                name: Some("../../.zshrc".into()),
                path: None,
            }],
            ..base.clone()
        };
        assert!(bad_link.validate().is_err());

        let bidi_link = Manifest {
            bin: vec![BinSpec {
                name: Some("safe\u{202e}sudo".into()),
                path: None,
            }],
            ..base.clone()
        };
        assert!(bidi_link.validate().is_err());

        let bad_path = Manifest {
            bin: vec![BinSpec {
                name: Some("x".into()),
                path: Some("../../../x".into()),
            }],
            ..base.clone()
        };
        assert!(bad_path.validate().is_err());

        let empty_bin = Manifest {
            bin: vec![BinSpec::default()],
            ..base.clone()
        };
        assert!(empty_bin.validate().is_err());

        let untypeable_alias = Manifest {
            provides: vec!["two words".into()],
            ..base
        };
        assert!(untypeable_alias.validate().is_err());
    }

    #[test]
    fn v_prefix_does_not_make_equal_semver_versions_order_apart() {
        let plain = Version::parse("0.4.0");
        let tagged = Version::parse("v0.4.0");
        assert_eq!(plain.cmp(&tagged), Ordering::Equal);
        assert_eq!(tagged.cmp(&plain), Ordering::Equal);
        assert!(tagged <= plain && plain <= tagged);
    }

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
    fn versions_that_differ_only_past_semver_still_order() {
        // Both relax to `1.2.3`, so semver alone calls them equal and `max_by`
        // is free to hand back the older release.
        assert!(Version::parse("1.2.3.5") > Version::parse("1.2.3.4"));
        assert!(Version::parse("1.2.3.10") > Version::parse("1.2.3.9"));
        let releases = [
            Version::parse("1.2.3.4"),
            Version::parse("1.2.3.5"),
            Version::parse("1.2.3.2"),
        ];
        assert_eq!(releases.iter().max().unwrap().raw, "1.2.3.5");
    }

    #[test]
    fn an_inferred_name_is_always_usable_as_a_directory() {
        let odd = Manifest::inferred(PackageRef::github("owner/.."));
        assert!(odd.validate().is_ok(), "name was {:?}", odd.name);
    }

    #[test]
    fn strip_prefix_is_bounded() {
        let base = Manifest::inferred(PackageRef::github("a/b"));
        assert!(Manifest {
            strip_prefix: Some(2),
            ..base.clone()
        }
        .validate()
        .is_ok());
        assert!(Manifest {
            strip_prefix: Some(usize::MAX),
            ..base
        }
        .validate()
        .is_err());
    }

    #[test]
    fn tag_matching_ignores_v_prefix() {
        assert!(Version::parse("v14.1.0").matches_request("14.1.0"));
        assert!(Version::parse("14.1.0").matches_request("v14.1.0"));
        assert!(!Version::parse("14.1.0").matches_request("14.1.1"));
    }

    #[test]
    fn glob_preferred_picks_the_stem_named_match_over_directory_order() {
        let hook = PathBuf::from("payload/rtok-hook.exe");
        let main = PathBuf::from("payload/rtok.exe");
        let matched = [hook.as_path(), main.as_path()];
        fn pick<'a>(m: &[&'a Path], n: Option<&str>) -> Option<&'a Path> {
            glob_preferred(Path::new("payload"), "rtok*", m, n).unwrap()
        }
        // NTFS lists rtok-hook.exe first; the stem preference must win anyway.
        assert_eq!(pick(&matched, Some("rtok")), Some(main.as_path()));
        // Case-insensitive like glob_match; a name carrying its own suffix still stems.
        assert_eq!(pick(&matched, Some("RTOK.EXE")), Some(main.as_path()));
        // One match is not ambiguous, whatever it is called; nothing matched is None.
        assert_eq!(pick(&[hook.as_path()], None), Some(hook.as_path()));
        assert_eq!(pick(&[], Some("rtok")), None);
    }

    #[test]
    fn glob_preferred_refuses_several_matches_none_named_like_the_link() {
        let root = Path::new("payload");
        let hook = PathBuf::from("payload/bin/rtok-hook.exe");
        let main = PathBuf::from("payload/bin/rtok.exe");
        // Both directory orders must give the same message.
        for matched in [
            [hook.as_path(), main.as_path()],
            [main.as_path(), hook.as_path()],
        ] {
            for name in [None, Some("other")] {
                let err = glob_preferred(root, "bin/rtok*", &matched, name)
                    .unwrap_err()
                    .to_string();
                assert!(err.contains("`bin/rtok*` matches 2 files"), "{err}");
                assert!(
                    err.contains("  bin/rtok-hook.exe\n  bin/rtok.exe\n"),
                    "candidates sorted and root-relative: {err}"
                );
                assert!(err.contains("set the entry's `name`"), "{err}");
            }
        }
    }

    #[test]
    fn glob_matches_asset_names() {
        assert!(glob_match(
            "*-aarch64-apple-darwin.tar.gz",
            "rg-14-aarch64-apple-darwin.tar.gz"
        ));
        assert!(glob_match("*.zip", "Tool-Universal.ZIP"));
        assert!(!glob_match("*.zip", "tool.tar.gz"));
        assert!(glob_match("rg?.tar.gz", "rg1.tar.gz"));
        // Pathological pattern must still terminate promptly.
        assert!(!glob_match("*a*a*a*a*b", &"a".repeat(64)));
    }

    #[test]
    fn hooks_parse_from_their_table_and_a_blank_one_is_refused() {
        let manifest: Manifest = toml::from_str(
            "name = \"tool\"\nsource = \"o/r\"\n[hooks]\nafter_install = \"./setup\"\n",
        )
        .unwrap();
        assert_eq!(manifest.hooks.after_install.as_deref(), Some("./setup"));
        assert!(manifest.hooks.before_install.is_none());
        manifest.validate().unwrap();
        // Round-trips without writing the five unset keys.
        let written = toml::to_string(&manifest).unwrap();
        assert!(written.contains("after_install"), "{written}");
        assert!(!written.contains("before_install"), "{written}");

        let blank: Manifest =
            toml::from_str("name = \"tool\"\nsource = \"o/r\"\n[hooks]\nbefore_update = \" \"\n")
                .unwrap();
        let err = blank.validate().unwrap_err().to_string();
        assert!(err.contains("hooks.before_update"), "{err}");

        let unknown = toml::from_str::<Manifest>(
            "name = \"tool\"\nsource = \"o/r\"\n[hooks]\nafter_instal = \"x\"\n",
        );
        assert!(unknown.is_err(), "a misspelt hook key must not be ignored");
    }

    #[test]
    fn committed_manifest_schema_matches_manifest() {
        crate::config::assert_schema_current::<Manifest>("docs/manifest.schema.json");
    }

    /// The committed schema, as an editor would load it.
    fn manifest_schema() -> jsonschema::Validator {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/manifest.schema.json");
        let text = std::fs::read_to_string(&path).expect("read the manifest schema");
        let schema: serde_json::Value = serde_json::from_str(&text).expect("parse the schema");
        jsonschema::validator_for(&schema).expect("compile the schema")
    }

    /// Schema errors for one manifest written as TOML; empty when it is valid.
    fn schema_errors(schema: &jsonschema::Validator, toml_text: &str) -> Vec<String> {
        let value: toml::Value = toml::from_str(toml_text).expect("parse TOML");
        let json = serde_json::to_value(value).expect("TOML as JSON");
        schema.iter_errors(&json).map(|e| e.to_string()).collect()
    }

    /// Whether ketch itself takes this manifest: serde, then `validate`.
    fn ketch_accepts(toml_text: &str) -> bool {
        toml::from_str::<Manifest>(toml_text).is_ok_and(|m| m.validate().is_ok())
    }

    #[test]
    fn every_manifest_ketch_ships_validates_against_the_schema() {
        let schema = manifest_schema();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

        let own = std::fs::read_to_string(root.join("ketch.toml")).expect("read ketch.toml");
        let errors = schema_errors(&schema, &own);
        assert!(errors.is_empty(), "ketch.toml: {errors:?}");

        let builtin: toml::Value = toml::from_str(crate::manifest::BUILTIN_TOML).expect("parse");
        let packages = builtin["package"].as_array().expect("[[package]]");
        assert!(!packages.is_empty());
        for package in packages {
            let json = serde_json::to_value(package).expect("TOML as JSON");
            let errors: Vec<String> = schema.iter_errors(&json).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "builtin {}: {errors:?}", package["name"]);
        }
    }

    /// Every TOML example in `MANIFESTS.md`, and the ones elsewhere in the docs
    /// that open with a `# <folder>/ketch.toml` comment. Most show one table or
    /// key, so a `source` is added where the example leaves it out; each must
    /// also be one ketch accepts, or the example is wrong rather than the schema.
    #[test]
    fn every_manifest_example_in_the_docs_validates_against_the_schema() {
        let schema = manifest_schema();
        let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        let mut seen = 0;
        for file in ["MANIFESTS.md", "REGISTRY.md"] {
            // A Windows checkout may have turned LF into CRLF.
            let text = std::fs::read_to_string(docs.join(file))
                .expect("read docs")
                .replace("\r\n", "\n");
            for block in text.split("```toml\n").skip(1) {
                let body = block.split("```").next().unwrap_or_default();
                let names_a_package_file = body
                    .lines()
                    .next()
                    .is_some_and(|line| line.starts_with('#') && line.ends_with("/ketch.toml"));
                if file != "MANIFESTS.md" && !names_a_package_file {
                    continue;
                }
                let parsed: toml::Value = toml::from_str(body).expect("example parses");
                let mut example = body.to_string();
                if parsed.get("source").is_none() {
                    example = format!("source = \"github:o/r\"\n{example}");
                }
                if parsed.get("name").is_none() {
                    example = format!("name = \"example\"\n{example}");
                }
                assert!(ketch_accepts(&example), "{file}: ketch refuses\n{example}");
                let errors = schema_errors(&schema, &example);
                assert!(errors.is_empty(), "{file}: {errors:?}\n{example}");
                seen += 1;
            }
        }
        assert!(seen >= 8, "found only {seen} examples; did the docs move?");
    }

    /// Manifests ketch refuses, each for a reason the schema can state. The
    /// schema must refuse them too, or an editor would pass a file that
    /// fails at install.
    #[test]
    fn the_schema_refuses_what_ketch_refuses() {
        let schema = manifest_schema();
        let cases = [
            "source = \"o/r\"\nsourse = \"x\"",
            "source = \"ripgrep\"",
            "source = \"github:\"",
            "source = \"o/r\"\nkind = \"library\"",
            "name = \"../evil\"\nsource = \"o/r\"",
            "name = \"-tool\"\nsource = \"o/r\"",
            "source = \"o/r\"\nbin = [{}]",
            "source = \"o/r\"\nbin = [{ name = \"a/b\" }]",
            "source = \"o/r\"\nstrip_prefix = 9",
            "source = \"o/r\"\nstrip_prefix = -1",
            "source = \"o/r\"\nprovides = [\"r g\"]",
            "source = \"o/r\"\nprovides = [\"\"]",
            "source = \"o/r\"\nextra_paths = [{ path = \"x.1\", kind = \"doc\" }]",
            "source = \"o/r\"\nextra_paths = [{ path = \"x.1\", kind = \"man\", sect = \"1\" }]",
            "source = \"o/r\"\n[trust]\nverifier = \"cosign\"",
            "source = \"o/r\"\n[trust]\nverifier = \"minisign\"\nmode = \"ignore\"",
            "source = \"o/r\"\n[trust]\nverifier = \"minisign\"\nkey = \"x\"",
            "source = \"o/r\"\n[hooks]\nafter_instal = \"x\"",
            "source = \"o/r\"\n[hooks]\nafter_install = \" \"",
        ];
        for case in cases {
            let with_name = if case.starts_with("name") {
                case.to_string()
            } else {
                format!("name = \"tool\"\n{case}")
            };
            assert!(!ketch_accepts(&with_name), "ketch takes\n{with_name}");
            assert!(
                !schema_errors(&schema, case).is_empty(),
                "the schema takes\n{case}"
            );
        }
    }

    /// Strings worth trying against the patterns: whatever proptest picks,
    /// plus the characters where ECMA-262 and Rust disagree about whitespace,
    /// and the ones `sanitize_component` and `PackageRef::parse` single out.
    fn tricky_text() -> impl proptest::strategy::Strategy<Value = String> {
        use proptest::prelude::*;
        let tricky = proptest::sample::select(vec![
            ' ', '\t', '\n', '\r', '\u{0b}', '\u{85}', '\u{a0}', '\u{feff}', '\u{2028}',
            '\u{3000}', '\u{180e}', '/', '\\', ':', '.', '-', '_', '\0', '\u{1f}', '\u{7f}',
            '\u{9f}', '\u{202e}', '\u{2066}', '\u{200b}', 'a', 'Z', '0', 'é',
        ]);
        proptest::collection::vec(prop_oneof![any::<char>(), tricky], 0..6)
            .prop_map(|chars| chars.into_iter().collect())
    }

    /// A one-pattern string schema, compiled by each engine a JSON Schema
    /// validator might use: ECMA-262 semantics over `fancy-regex`, and the
    /// plain `regex` crate.
    fn pattern_validators(pattern: &str) -> [jsonschema::Validator; 2] {
        let schema = serde_json::json!({ "type": "string", "pattern": pattern });
        [
            jsonschema::validator_for(&schema).expect("compile"),
            jsonschema::options()
                .with_pattern_options(jsonschema::PatternOptions::regex())
                .build(&schema)
                .expect("compile"),
        ]
    }

    proptest::proptest! {
        #[test]
        fn schema_patterns_agree_with_the_rust_checks(text in tricky_text()) {
            static PATTERNS: std::sync::LazyLock<[[jsonschema::Validator; 2]; 4]> =
                std::sync::LazyLock::new(|| {
                    [
                        pattern_validators(schema_pattern::PACKAGE_REF),
                        pattern_validators(schema_pattern::FILE_NAME),
                        pattern_validators(schema_pattern::ALIAS),
                        pattern_validators(schema_pattern::NOT_BLANK),
                    ]
                });
            let base = Manifest::inferred(PackageRef::github("o/r"));
            let rust = [
                PackageRef::parse(&text).is_some(),
                Manifest { name: text.clone(), ..base.clone() }.validate().is_ok(),
                Manifest { provides: vec![text.clone()], ..base.clone() }.validate().is_ok(),
                Manifest {
                    hooks: Hooks { after_install: Some(text.clone()), ..Hooks::default() },
                    ..base
                }
                .validate()
                .is_ok(),
            ];
            let instance = serde_json::Value::String(text.clone());
            for (which, (validators, expected)) in PATTERNS.iter().zip(rust).enumerate() {
                for validator in validators {
                    proptest::prop_assert_eq!(
                        validator.is_valid(&instance),
                        expected,
                        "pattern {} on {:?}",
                        which,
                        text
                    );
                }
            }
        }
    }
}
