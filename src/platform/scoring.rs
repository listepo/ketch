//! Shared asset-name scoring helpers.
//!
//! Compiled on every OS so Linux CI can exercise token matching, rejection
//! rules and container tie-breaks without pulling in a platform backend.

use super::AssetScore;
use crate::model::{Arch, Os};

/// Does `needle` appear in `haystack` as a whole token?
///
/// Plain `contains` is not usable here: `darwin` contains `win`, `install`
/// contains `all`, and either would misroute an asset to the wrong platform.
pub(crate) fn token_at(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(needle).any(|(start, matched)| {
        let end = start + matched.len();
        let left = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let right = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        left && right
    })
}

/// First token from `tokens` that appears whole in `haystack`.
pub(crate) fn find_token(haystack: &str, tokens: &[&'static str]) -> Option<&'static str> {
    tokens.iter().copied().find(|t| token_at(haystack, t))
}

/// Bonus and label for the container format, read off the file name.
///
/// The spread is deliberately small: it only breaks ties between assets that
/// already agree on OS and architecture.
pub(crate) fn container_bonus(lower: &str) -> (i32, &'static str) {
    const KNOWN: &[(&str, i32, &str)] = &[
        (".tar.gz", 8, "tar.gz"),
        (".tgz", 8, "tar.gz"),
        (".tar.xz", 7, "tar.xz"),
        (".txz", 7, "tar.xz"),
        (".tar.bz2", 5, "tar.bz2"),
        (".tar", 6, "tar"),
        (".zip", 6, "zip"),
        (".exe", 7, "exe"),
        (".gz", 5, "gz"),
        (".dmg", 3, "dmg"),
        (".pkg", 2, "pkg"),
    ];
    for (suffix, bonus, label) in KNOWN {
        if lower.ends_with(suffix) {
            return (*bonus, label);
        }
    }
    // No recognised container: most likely the bare executable.
    (4, "raw")
}

/// True when the lower-cased asset name is never installable.
///
/// `extra_tokens` carries platform-specific by-product markers (for example
/// macOS debug symbols) that should reject an asset when they appear as whole
/// tokens.
pub(crate) fn is_rejected(lower: &str, extra_tokens: &[&str]) -> bool {
    super::is_sidecar(lower)
        || super::NON_BINARY_TOKENS.iter().any(|t| lower.contains(t))
        || super::REJECTED_EXTENSIONS
            .iter()
            .any(|e| lower.ends_with(e))
        || extra_tokens.iter().any(|t| token_at(lower, t))
}

/// A raw binary asset lands under the asset's own file name — `jq-macos-arm64`
/// — which is not what anyone wants on PATH. Rename to the package name only
/// when the discovered name plainly carries build metadata and there is no
/// second binary that the rename could collide with.
pub(crate) fn looks_like_build_artifact(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let platform_token = [
        Os::MacOs.tokens(),
        Os::Linux.tokens(),
        Os::Windows.tokens(),
        Arch::Aarch64.tokens(),
        Arch::X86_64.tokens(),
        Arch::Universal.tokens(),
    ]
    .iter()
    .flat_map(|set| set.iter())
    .any(|t| token_at(&lower, t));

    platform_token || has_version_run(&lower)
}

/// True for names carrying something like `1.2` or `v3`.
fn has_version_run(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    bytes
        .windows(3)
        .any(|w| w[0].is_ascii_digit() && w[1] == b'.' && w[2].is_ascii_digit())
        || bytes
            .windows(2)
            .any(|w| w[0] == b'v' && w[1].is_ascii_digit())
}

/// Extra tokens that mark a macOS asset as a build by-product.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BYPRODUCT_TOKENS: &[&str] = &["dsym", "debuginfo", "symbols"];

/// Score a release asset name for macOS without constructing a platform backend.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn score_macos_asset(
    asset_name: &str,
    host_arch: Arch,
    allow_emulation: bool,
) -> Option<AssetScore> {
    let lower = asset_name.trim().to_ascii_lowercase();
    if lower.is_empty() || is_rejected(&lower, BYPRODUCT_TOKENS) || lower.ends_with(".exe") {
        return None;
    }

    // Anything that names a foreign OS is not ours, whatever else it says.
    if find_token(&lower, Os::Linux.tokens()).is_some()
        || find_token(&lower, Os::Windows.tokens()).is_some()
    {
        return None;
    }

    let mut score = 0;
    let mut reason = Vec::new();
    match find_token(&lower, Os::MacOs.tokens()) {
        Some(token) => {
            score += 50;
            reason.push(token.to_string());
        }
        // No OS in the name at all: single-platform projects do this, so it
        // stays a candidate but loses to anything explicit.
        None => score += 15,
    }

    let host = host_arch;
    let (arch, emulated) = if find_token(&lower, host.tokens()).is_some() {
        score += 40;
        (host, false)
    } else if find_token(&lower, Arch::Universal.tokens()).is_some() {
        score += 35;
        (Arch::Universal, false)
    } else if host == Arch::Aarch64 && find_token(&lower, Arch::X86_64.tokens()).is_some() {
        score += 10;
        (Arch::X86_64, true)
    } else if find_token(&lower, Arch::Aarch64.tokens()).is_some()
        || find_token(&lower, Arch::X86_64.tokens()).is_some()
    {
        // Names a real architecture, just not one this machine can run.
        return None;
    } else {
        score += 18;
        (Arch::Universal, false)
    };

    if emulated {
        if !allow_emulation {
            return None;
        }
        reason.push("x86_64 under Rosetta".to_string());
    } else {
        reason.push(arch.to_string());
    }

    let (bonus, container) = container_bonus(&lower);
    score += bonus;
    reason.push(container.to_string());

    Some(AssetScore {
        score,
        arch,
        emulated,
        reason: reason.join(" / "),
    })
}

/// Score a release asset name for Linux without constructing a platform backend.
///
/// `musl_host` is whether this machine's libc is musl. A glibc (`gnu`) asset is
/// refused there because it will not load; a musl asset on a glibc host is
/// accepted but ranks below a native glibc build.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn score_linux_asset(
    asset_name: &str,
    host_arch: Arch,
    allow_emulation: bool,
    musl_host: bool,
) -> Option<AssetScore> {
    let lower = asset_name.trim().to_ascii_lowercase();
    if lower.is_empty() || is_rejected(&lower, &["debuginfo", "dbg"]) || lower.ends_with(".exe") {
        return None;
    }
    if find_token(&lower, Os::MacOs.tokens()).is_some()
        || find_token(&lower, Os::Windows.tokens()).is_some()
    {
        return None;
    }

    let mut score = 0;
    let mut reason = Vec::new();
    let has_linux = find_token(&lower, &["linux"]).is_some();
    let has_musl = find_token(&lower, &["musl"]).is_some();
    let has_gnu = find_token(&lower, &["gnu"]).is_some();

    if has_linux || has_musl || has_gnu {
        score += 50;
        if has_linux {
            reason.push("linux".to_string());
        }
    } else {
        score += 15;
    }

    if musl_host && has_gnu && !has_musl {
        return None;
    }
    match (musl_host, has_musl, has_gnu) {
        (true, true, _) => {
            score += 12;
            reason.push("musl".to_string());
        }
        (false, false, true) => {
            score += 12;
            reason.push("gnu".to_string());
        }
        (false, true, _) => {
            score += 6;
            reason.push("musl".to_string());
        }
        _ => {}
    }

    let host = host_arch;
    let (arch, emulated) = if find_token(&lower, host.tokens()).is_some() {
        score += 40;
        (host, false)
    } else if host == Arch::Aarch64 && find_token(&lower, Arch::X86_64.tokens()).is_some() {
        score += 10;
        (Arch::X86_64, true)
    } else if find_token(&lower, Arch::Aarch64.tokens()).is_some()
        || find_token(&lower, Arch::X86_64.tokens()).is_some()
    {
        return None;
    } else {
        score += 18;
        (host, false)
    };

    if emulated {
        if !allow_emulation {
            return None;
        }
        reason.push("x86_64 under emulation".to_string());
    } else {
        reason.push(arch.to_string());
    }

    let (bonus, container) = container_bonus(&lower);
    score += bonus;
    reason.push(container.to_string());

    Some(AssetScore {
        score,
        arch,
        emulated,
        reason: reason.join(" / "),
    })
}

/// Score a release asset name for Windows without constructing a platform backend.
///
/// `gnu` is not treated as foreign: mingw triples (`pc-windows-gnu`) are
/// Windows. `linux` and `musl` are.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn score_windows_asset(asset_name: &str, host_arch: Arch) -> Option<AssetScore> {
    let lower = asset_name.trim().to_ascii_lowercase();
    if lower.is_empty() || is_rejected(&lower, &[]) {
        return None;
    }
    if find_token(&lower, Os::MacOs.tokens()).is_some()
        || find_token(&lower, &["linux"]).is_some()
        || find_token(&lower, &["musl"]).is_some()
    {
        return None;
    }

    let mut score = 0;
    let mut reason = Vec::new();
    match find_token(&lower, Os::Windows.tokens()) {
        Some(token) => {
            score += 50;
            reason.push(token.to_string());
        }
        None => score += 15,
    }

    let host = host_arch;
    let arch = if find_token(&lower, host.tokens()).is_some() {
        score += 40;
        host
    } else if find_token(&lower, Arch::Aarch64.tokens()).is_some()
        || find_token(&lower, Arch::X86_64.tokens()).is_some()
    {
        return None;
    } else {
        score += 18;
        host
    };
    reason.push(arch.to_string());

    let (bonus, container) = container_bonus(&lower);
    score += bonus;
    reason.push(container.to_string());

    Some(AssetScore {
        score,
        arch,
        emulated: false,
        reason: reason.join(" / "),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matching_respects_word_boundaries() {
        // The whole reason `contains` is not good enough.
        assert!(!token_at("x86_64-apple-darwin", "win"));
        assert!(token_at("tool-windows-amd64.zip", "windows"));
        assert!(!token_at("tool-install.tar.gz", "all"));
        assert!(token_at("tool-universal-all.zip", "all"));
    }

    #[test]
    fn is_rejected_without_extra_tokens_rejects_sidecars_and_foreign_formats() {
        for name in [
            "rg-14.1.0-aarch64-apple-darwin.tar.gz.sha256",
            "checksums.txt",
            "ripgrep_14.1.0_amd64.deb",
        ] {
            let lower = name.to_ascii_lowercase();
            assert!(is_rejected(&lower, &[]), "should have rejected {name}");
        }
    }

    #[test]
    fn is_rejected_with_extra_tokens_rejects_byproducts() {
        let lower = "tool-macos-arm64.dsym.zip".to_ascii_lowercase();
        assert!(is_rejected(&lower, &["dsym"]));
    }

    #[test]
    fn container_bonus_prefers_tar_gz_over_zip() {
        let (tar_gz, _) = container_bonus("tool-1.0-aarch64-apple-darwin.tar.gz");
        let (zip, _) = container_bonus("tool-1.0-aarch64-apple-darwin.zip");
        assert!(tar_gz > zip);
    }

    #[test]
    fn recognises_build_metadata_in_a_binary_name() {
        assert!(looks_like_build_artifact("jq-macos-arm64"));
        assert!(looks_like_build_artifact("jq-linux-amd64"));
        assert!(looks_like_build_artifact("tool-x86_64-pc-windows-msvc.exe"));
        assert!(looks_like_build_artifact("tool-v1.2.3"));
        assert!(!looks_like_build_artifact("rg"));
        assert!(!looks_like_build_artifact("fd"));
    }

    #[test]
    fn rejects_foreign_platforms_and_sidecars() {
        for name in [
            "rg-14.1.0-x86_64-unknown-linux-musl.tar.gz",
            "tool-windows-amd64.zip",
            "rg-14.1.0-aarch64-apple-darwin.tar.gz.sha256",
            "ripgrep_14.1.0_amd64.deb",
            "checksums.txt",
            "tool-macos-arm64.dSYM.zip",
        ] {
            assert!(
                score_macos_asset(name, Arch::Aarch64, true).is_none(),
                "should have rejected {name}"
            );
        }
    }

    #[test]
    fn prefers_the_native_architecture_over_emulation() {
        let native =
            score_macos_asset("rg-14.1.0-aarch64-apple-darwin.tar.gz", Arch::Aarch64, true)
                .unwrap();
        let rosetta =
            score_macos_asset("rg-14.1.0-x86_64-apple-darwin.tar.gz", Arch::Aarch64, true).unwrap();
        assert!(native.score > rosetta.score);
        assert!(rosetta.emulated && !native.emulated);
        // Emulation is a choice, not a default the user cannot refuse.
        assert!(
            score_macos_asset("rg-14.1.0-x86_64-apple-darwin.tar.gz", Arch::Aarch64, false)
                .is_none()
        );
    }

    #[test]
    fn universal_builds_are_accepted_on_any_mac() {
        let universal = score_macos_asset(
            "tool-1.0-universal2-apple-darwin.tar.gz",
            Arch::Aarch64,
            true,
        )
        .unwrap();
        assert_eq!(universal.arch, Arch::Universal);
        assert!(!universal.emulated);
    }

    #[test]
    fn names_without_an_os_still_qualify_but_rank_lower() {
        let explicit =
            score_macos_asset("tool_1.0_darwin_arm64.tar.gz", Arch::Aarch64, true).unwrap();
        let bare = score_macos_asset("tool_1.0_arm64.tar.gz", Arch::Aarch64, true).unwrap();
        assert!(explicit.score > bare.score);
    }

    #[test]
    fn linux_rejects_foreign_platforms_and_sidecars() {
        for name in [
            "rg-14.1.0-aarch64-apple-darwin.tar.gz",
            "tool-windows-amd64.zip",
            "rg-14.1.0-x86_64-unknown-linux-musl.tar.gz.sha256",
            "ripgrep_14.1.0_amd64.deb",
            "tool.exe",
        ] {
            assert!(
                score_linux_asset(name, Arch::X86_64, true, false).is_none(),
                "should have rejected {name}"
            );
        }
    }

    #[test]
    fn linux_prefers_native_gnu_over_musl_on_a_gnu_host() {
        let gnu = score_linux_asset(
            "rg-14.1.0-x86_64-unknown-linux-gnu.tar.gz",
            Arch::X86_64,
            true,
            false,
        )
        .unwrap();
        let musl = score_linux_asset(
            "rg-14.1.0-x86_64-unknown-linux-musl.tar.gz",
            Arch::X86_64,
            true,
            false,
        )
        .unwrap();
        assert!(gnu.score > musl.score);
        assert!(!gnu.emulated && !musl.emulated);
    }

    #[test]
    fn linux_musl_host_refuses_gnu_and_accepts_musl() {
        assert!(score_linux_asset(
            "rg-14.1.0-x86_64-unknown-linux-gnu.tar.gz",
            Arch::X86_64,
            true,
            true,
        )
        .is_none());
        assert!(score_linux_asset(
            "rg-14.1.0-x86_64-unknown-linux-musl.tar.gz",
            Arch::X86_64,
            true,
            true,
        )
        .is_some());
    }

    #[test]
    fn linux_emulated_x86_64_is_optional_on_aarch64() {
        let native = score_linux_asset(
            "rg-14.1.0-aarch64-unknown-linux-gnu.tar.gz",
            Arch::Aarch64,
            true,
            false,
        )
        .unwrap();
        let emulated = score_linux_asset(
            "rg-14.1.0-x86_64-unknown-linux-gnu.tar.gz",
            Arch::Aarch64,
            true,
            false,
        )
        .unwrap();
        assert!(native.score > emulated.score);
        assert!(emulated.emulated && !native.emulated);
        assert!(score_linux_asset(
            "rg-14.1.0-x86_64-unknown-linux-gnu.tar.gz",
            Arch::Aarch64,
            false,
            false,
        )
        .is_none());
    }

    #[test]
    fn windows_rejects_unix_assets_and_installers() {
        for name in [
            "rg-14.1.0-x86_64-unknown-linux-gnu.tar.gz",
            "rg-14.1.0-aarch64-apple-darwin.tar.gz",
            "tool-x86_64-unknown-linux-musl.tar.gz",
            "setup.msi",
            "tool.nupkg",
        ] {
            assert!(
                score_windows_asset(name, Arch::X86_64).is_none(),
                "should have rejected {name}"
            );
        }
    }

    #[test]
    fn windows_accepts_exe_and_mingw_triples() {
        let exe = score_windows_asset("rg.exe", Arch::X86_64).unwrap();
        let mingw =
            score_windows_asset("rg-14.1.0-x86_64-pc-windows-gnu.zip", Arch::X86_64).unwrap();
        let msvc =
            score_windows_asset("rg-14.1.0-x86_64-pc-windows-msvc.zip", Arch::X86_64).unwrap();
        assert_eq!(exe.arch, Arch::X86_64);
        assert!(!mingw.emulated);
        assert!(msvc.score > exe.score);
    }

    #[test]
    fn windows_prefers_native_architecture() {
        assert!(score_windows_asset("tool-aarch64-pc-windows-msvc.zip", Arch::X86_64).is_none());
        assert!(score_windows_asset("tool-x86_64-pc-windows-msvc.zip", Arch::X86_64).is_some());
    }
}
