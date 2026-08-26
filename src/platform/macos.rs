//! macOS.
//!
//! Asset scoring understands Apple's naming conventions (`darwin`, `apple`,
//! `universal`, `arm64` vs `aarch64`), placement knows the difference between a
//! CLI binary and a `.app` bundle, and trust checks run `codesign`/`spctl`
//! before any quarantine flag is cleared.

use super::{AssetScore, DoctorCheck, Placement, Platform, TrustVerdict};
use crate::config::Config;
use crate::error::Result;
use crate::extract::Extractor;
use crate::model::{LinkRecord, TargetSpec};
use std::path::Path;

pub struct MacOsPlatform {
    _target: TargetSpec,
}

impl Default for MacOsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsPlatform {
    pub fn new() -> Self {
        MacOsPlatform {
            _target: TargetSpec::host(),
        }
    }
}

impl Platform for MacOsPlatform {
    fn id(&self) -> &str {
        "macos"
    }

    fn target(&self) -> TargetSpec {
        todo!("host target")
    }

    fn score_asset(&self, _asset_name: &str, _allow_emulation: bool) -> Option<AssetScore> {
        todo!("score by os/arch tokens, reject sidecars and foreign platforms")
    }

    fn extractors(&self) -> Vec<Box<dyn Extractor>> {
        todo!("dmg, pkg, tar.*, zip, raw — most specific first")
    }

    fn place(&self, _plan: &Placement<'_>) -> Result<Vec<LinkRecord>> {
        todo!("move payload into the store, symlink binaries, copy or link .app bundles")
    }

    fn unplace(&self, _links: &[LinkRecord]) -> Result<()> {
        todo!("remove links, tolerating ones already gone")
    }

    fn verify_trust(&self, _path: &Path) -> Result<TrustVerdict> {
        todo!("codesign --verify, then spctl --assess")
    }

    fn clear_quarantine(&self, _path: &Path) -> Result<()> {
        todo!("xattr -dr com.apple.quarantine")
    }

    fn is_executable(&self, _path: &Path) -> bool {
        todo!("regular file with any execute bit, and a Mach-O or script header")
    }

    fn path_setup_hint(&self, _bin_dir: &Path) -> String {
        todo!("zsh/bash PATH snippet")
    }

    fn app_bundle_extension(&self) -> Option<&str> {
        Some(".app")
    }

    fn doctor(&self, _cfg: &Config) -> Vec<DoctorCheck> {
        todo!("PATH, writability, Rosetta, xattr/codesign availability, store integrity")
    }
}
