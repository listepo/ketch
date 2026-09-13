//! Per-operating-system behaviour.
//!
//! Everything that differs between macOS, Linux and Windows lives behind this
//! trait: which release asset is even installable, how a payload becomes
//! something on PATH, and what "is this code trustworthy" means locally.
//!
//! macOS, Linux and Windows each have a backend. Adding another OS means adding
//! a file here and one arm in `host()` — no changes anywhere else.

pub mod scoring;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(unix)]
pub mod unix;
#[cfg(target_os = "windows")]
pub mod windows;

use crate::config::Config;
use crate::error::Result;
use crate::extra::ExtraPlacement;
use crate::model::{Arch, BinSpec, CompletionShell, LinkRecord, PackageKind, TargetSpec};
use std::path::{Path, PathBuf};
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
    /// macOS `.app` install root; unused on Linux/Windows placement.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
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
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub link_apps: bool,
    /// Create user-visible links. False still moves the payload into the
    /// store, so `ketch relink` can expose it later without re-downloading.
    pub link: bool,
    /// Man pages and completions already classified and given destinations.
    pub extras: &'a [ExtraPlacement],
}

/// Result of a local trust check on downloaded code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustVerdict {
    /// Validly signed and accepted by the system policy.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Trusted { authority: String },
    /// Signed, but the system would still warn (ad-hoc, or unnotarized).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Weak { detail: String },
    /// No usable signature.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Files this platform treats as app bundles rather than executables.
    #[allow(dead_code)]
    fn app_bundle_extension(&self) -> Option<&str> {
        None
    }

    /// Environment checks for `ketch doctor`.
    fn doctor(&self, cfg: &Config) -> Vec<DoctorCheck>;

    /// User-writable man root. Pages go in `manN/` underneath.
    ///
    /// Defaults to `$XDG_DATA_HOME/man` or `~/.local/share/man`, not
    /// `dirs::data_dir()`, which on macOS is Application Support — a place
    /// `man` never looks.
    fn user_man_root(&self) -> PathBuf {
        data_home().join("man")
    }

    /// Directory this shell searches for user completion scripts.
    fn completion_dir(&self, shell: CompletionShell) -> PathBuf {
        completion_dir_for(shell)
    }
}

/// `$XDG_DATA_HOME`, falling back to `~/.local/share` on every OS.
pub fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local/share")
        })
}

/// `$XDG_CONFIG_HOME`, falling back to `~/.config`.
pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        })
}

/// Unix-style completion directories. Windows overrides PowerShell.
pub fn completion_dir_for(shell: CompletionShell) -> PathBuf {
    match shell {
        CompletionShell::Bash => data_home().join("bash-completion/completions"),
        CompletionShell::Zsh => data_home().join("zsh/site-functions"),
        CompletionShell::Fish => config_home().join("fish/completions"),
        CompletionShell::Elvish => config_home().join("elvish/lib"),
        CompletionShell::Powershell => config_home().join("powershell/Completions"),
    }
}

/// Doctor lines for the destinations `place` will write. Missing directories
/// are reported as ok: install creates them rather than surprising the user
/// with a write they were not shown.
pub fn extra_destination_checks(platform: &dyn Platform) -> Vec<DoctorCheck> {
    let mut checks = vec![dest_check("man", &platform.user_man_root(), true)];
    for shell in CompletionShell::ALL {
        checks.push(dest_check(
            &format!("completions-{}", shell.as_str()),
            &platform.completion_dir(shell),
            false,
        ));
    }
    checks
}

fn dest_check(name: &str, path: &Path, mention_manpath: bool) -> DoctorCheck {
    let mut detail = path.display().to_string();
    if !path.exists() {
        detail.push_str(" — created on install");
    }
    if mention_manpath {
        detail.push_str("; add this directory to MANPATH so `man` finds pages ketch installs");
    }
    if path.exists() && probe_writable(path).is_err() {
        return DoctorCheck::fail(
            name,
            format!("{} is not writable", path.display()),
            format!("chmod u+w {}", path.display()),
        );
    }
    DoctorCheck::ok(name, detail)
}

fn probe_writable(dir: &Path) -> std::result::Result<(), String> {
    tempfile::Builder::new()
        .prefix(".ketch-probe")
        .tempfile_in(dir)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Link classified extras from `root` (the store prefix) to their dests.
pub fn expose_extras(
    extras: &[ExtraPlacement],
    root: &Path,
    owned: &Path,
    recorded: &[LinkRecord],
) -> Result<Vec<LinkRecord>> {
    #[cfg(unix)]
    {
        unix::link_planned_extras(extras, root, owned, recorded)
    }
    #[cfg(windows)]
    {
        let _ = owned;
        windows::link_planned_extras(extras, root, recorded)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (extras, root, owned, recorded);
        Ok(Vec::new())
    }
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
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(linux::LinuxPlatform::new()))
    }
    #[cfg(target_os = "windows")]
    {
        Ok(Arc::new(windows::WindowsPlatform::new()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(crate::error::Error::msg(format!(
            "ketch {} has no backend for this operating system. \
             Implementing `Platform` in src/platform/ is all that is required.",
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
    "src",
    "vendor",
    "manifest",
    "provenance",
    "attestation",
    "changelog",
    "release-notes",
];

/// Filename tokens for operating systems ketch does not support.
///
/// An asset naming one of these is never installable on macOS, Linux or
/// Windows — even when it also carries a recognised architecture token.
pub const FOREIGN_OS_TOKENS: &[&str] = &["freebsd", "netbsd", "openbsd", "plan9", "dragonfly"];

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

    #[test]
    fn extra_destination_checks_name_man_and_each_shell() {
        let host = host().expect("host platform");
        let checks = extra_destination_checks(host.as_ref());
        let names: Vec<_> = checks.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"man"), "{names:?}");
        assert!(names.contains(&"completions-bash"), "{names:?}");
        assert!(names.contains(&"completions-zsh"), "{names:?}");
        assert!(names.contains(&"completions-fish"), "{names:?}");
        assert!(
            checks
                .iter()
                .any(|c| c.name == "man" && c.detail.contains("MANPATH")),
            "{}",
            checks
                .iter()
                .find(|c| c.name == "man")
                .map(|c| c.detail.as_str())
                .unwrap_or("")
        );
    }

    #[test]
    fn user_man_root_is_under_xdg_data_home() {
        assert_eq!(data_home().join("man"), host().unwrap().user_man_root());
        assert_eq!(
            data_home().join("bash-completion/completions"),
            host()
                .unwrap()
                .completion_dir(crate::model::CompletionShell::Bash)
        );
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn host_is_linux() {
        assert_eq!(host().unwrap().id(), "linux");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn host_is_windows() {
        assert_eq!(host().unwrap().id(), "windows");
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    #[test]
    fn host_errs_on_an_os_with_no_backend() {
        let err = host().unwrap_err();
        assert!(err.to_string().contains("no backend"));
    }
}
