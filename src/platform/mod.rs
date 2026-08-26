//! Per-operating-system behaviour.
//!
//! Everything that differs between macOS, Linux and Windows lives behind this
//! trait: which release asset is even installable, how a payload becomes
//! something on PATH, and what "is this code trustworthy" means locally.
//!
//! Only macOS is implemented today. Adding Linux means adding a file here and
//! one line in `host()` — no changes anywhere else in the codebase.

#[cfg(target_os = "macos")]
pub mod macos;

use crate::config::Config;
use crate::error::Result;
use crate::model::{Arch, BinSpec, LinkRecord, PackageKind, TargetSpec};
use std::path::Path;
use std::sync::Arc;

/// Why an asset was chosen, and at what cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetScore {
    /// Higher wins. Only compared between assets of the same release.
    pub score: i32,
    /// Architecture this asset actually provides.
    pub arch: Arch,
    /// True when it runs only under emulation (x86_64 on Apple Silicon).
    pub emulated: bool,
    /// Short explanation, shown with `--verbose` and in `ketch info`.
    pub reason: String,
}

/// Everything the platform needs to place an extracted payload.
pub struct Placement<'a> {
    pub name: &'a str,
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
    pub version: &'a str,
    /// Directory holding the extracted release payload.
    pub payload_dir: &'a Path,
    /// Final home of this version inside the store.
    pub store_dir: &'a Path,
    pub bin_dir: &'a Path,
    pub apps_dir: &'a Path,
    pub kind: PackageKind,
    /// Explicit binaries from the manifest. Empty means "discover them".
    pub bin_specs: &'a [BinSpec],
    /// Links recorded for the version being replaced, which still exist:
    /// placement runs before the old version is retired. A destination listed
    /// here is ketch's own to overwrite. Anything else occupying a destination
    /// belongs to another package or to the user.
    pub replacing: &'a [LinkRecord],
    /// Symlink `.app` bundles rather than copying them.
    pub link_apps: bool,
    /// Create user-visible links. False still moves the payload into the
    /// store, so `ketch relink` can expose it later without re-downloading.
    pub link: bool,
}

/// Result of a local trust check on downloaded code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustVerdict {
    /// Validly signed and accepted by the system policy.
    Trusted { authority: String },
    /// Signed, but the system would still warn (ad-hoc, or unnotarized).
    Weak { detail: String },
    /// No usable signature.
    Untrusted { detail: String },
    /// This platform does not do signature checks.
    NotApplicable,
}

impl TrustVerdict {
    /// Whether it is safe to remove the quarantine flag without silently
    /// disabling a protection the user is relying on.
    pub fn may_strip_quarantine(&self) -> bool {
        matches!(self, TrustVerdict::Trusted { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

/// One line of `ketch doctor` output.
#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub name: String,
    pub status: CheckStatus,
    pub detail: String,
    pub fix: Option<String>,
}

impl DoctorCheck {
    pub fn ok(name: impl Into<String>, detail: impl Into<String>) -> Self {
        DoctorCheck {
            name: name.into(),
            status: CheckStatus::Ok,
            detail: detail.into(),
            fix: None,
        }
    }
    pub fn warn(
        name: impl Into<String>,
        detail: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        DoctorCheck {
            name: name.into(),
            status: CheckStatus::Warn,
            detail: detail.into(),
            fix: Some(fix.into()),
        }
    }
    pub fn fail(
        name: impl Into<String>,
        detail: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        DoctorCheck {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
            fix: Some(fix.into()),
        }
    }
}

/// Present so `ketch doctor` can colour a summary without re-deriving it.
pub fn worst_status(checks: &[DoctorCheck]) -> CheckStatus {
    if checks.iter().any(|c| c.status == CheckStatus::Fail) {
        CheckStatus::Fail
    } else if checks.iter().any(|c| c.status == CheckStatus::Warn) {
        CheckStatus::Warn
    } else {
        CheckStatus::Ok
    }
}

/// The host operating system's rules.
pub trait Platform: Send + Sync {
    /// Stable identifier, e.g. `macos`.
    #[allow(dead_code)]
    fn id(&self) -> &str;

    fn target(&self) -> TargetSpec;

    /// Rate an asset by file name alone.
    ///
    /// `None` means "cannot run here" and the asset is discarded. This is the
    /// single most important function for install quality: it is what stops
    /// ketch grabbing a Linux tarball or a `.sha256` sidecar.
    fn score_asset(&self, asset_name: &str, allow_emulation: bool) -> Option<AssetScore>;

    /// Extractors this platform can use, most specific first.
    fn extractors(&self) -> Vec<Box<dyn crate::extract::Extractor>>;

    /// Move the payload into the store and create user-visible links.
    fn place(&self, plan: &Placement<'_>) -> Result<Vec<LinkRecord>>;

    /// Undo `place`. Must tolerate links that are already gone.
    fn unplace(&self, links: &[LinkRecord]) -> Result<()>;

    /// Inspect downloaded code before it is exposed to the user.
    fn verify_trust(&self, _path: &Path) -> Result<TrustVerdict> {
        Ok(TrustVerdict::NotApplicable)
    }

    /// Clear the OS "downloaded from the internet" mark. Only called when the
    /// trust verdict allows it.
    fn clear_quarantine(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    /// Is this file something we can execute and link onto PATH?
    fn is_executable(&self, path: &Path) -> bool;

    /// Shell snippet that puts `bin_dir` on PATH.
    fn path_setup_hint(&self, bin_dir: &Path) -> String;

    /// Files this platform treats as app bundles rather than executables.
    #[allow(dead_code)]
    fn app_bundle_extension(&self) -> Option<&str> {
        None
    }

    /// Environment checks for `ketch doctor`.
    fn doctor(&self, cfg: &Config) -> Vec<DoctorCheck>;
}

/// The platform for the machine we are on.
///
/// Unsupported hosts fail here with one clear message rather than misbehaving
/// deeper in the install pipeline.
pub fn host() -> Result<Arc<dyn Platform>> {
    #[cfg(target_os = "macos")]
    {
        Ok(Arc::new(macos::MacOsPlatform::new()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(crate::error::Error::msg(format!(
            "ketch {} supports macOS only. Linux and Windows backends are planned; \
             see ROADMAP.md — implementing `Platform` in src/platform/ is all that is required.",
            env!("CARGO_PKG_VERSION")
        )))
    }
}

/// Tokens in an asset name that mean "this is not a program".
///
/// Shared by every platform, because signature and checksum sidecars look the
/// same everywhere.
pub const SIDECAR_SUFFIXES: &[&str] = &[
    ".sha256",
    ".sha512",
    ".sha1",
    ".md5",
    ".asc",
    ".sig",
    ".sigstore",
    ".pem",
    ".crt",
    ".sbom",
    ".sbom.json",
    ".spdx.json",
    ".intoto.jsonl",
    ".pubkey",
    ".minisig",
    ".cert",
];

/// Substrings that mark a file as source code or metadata, not a build.
pub const NON_BINARY_TOKENS: &[&str] = &[
    "checksum",
    "checksums",
    "sha256sums",
    "sha512sums",
    "source-code",
    "sources",
    "src.tar",
    "-src-",
    "vendor",
    "manifest",
    "provenance",
    "attestation",
    "changelog",
    "release-notes",
];

/// Extensions that never contain a runnable macOS/Linux payload.
pub const REJECTED_EXTENSIONS: &[&str] = &[
    ".txt",
    ".md",
    ".json",
    ".yaml",
    ".yml",
    ".xml",
    ".csv",
    ".log",
    ".deb",
    ".rpm",
    ".apk",
    ".msi",
    ".exe",
    ".appimage",
    ".snap",
    ".flatpak",
    ".nupkg",
    ".jar",
    ".war",
    ".whl",
    ".gem",
];

/// True when `name` ends with any known sidecar suffix.
pub fn is_sidecar(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SIDECAR_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_sidecars() {
        assert!(is_sidecar("rg-14.tar.gz.sha256"));
        assert!(is_sidecar("tool.dmg.asc"));
        assert!(is_sidecar("bundle.intoto.jsonl"));
        assert!(!is_sidecar("rg-14.tar.gz"));
    }
}
