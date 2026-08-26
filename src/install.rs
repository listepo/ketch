//! The install pipeline.
//!
//! resolve manifest → resolve release → score and pick an asset → download →
//! verify checksum → extract → verify trust → place → record state.
//!
//! Every step is expressed against the `Source`, `Platform` and `Extractor`
//! traits, so this file contains no GitHub-specific and no macOS-specific code.

use crate::config::Config;
use crate::error::Result;
use crate::model::{InstalledPackage, PackageSpec, Release, ReleaseAsset, Version};
use crate::platform::AssetScore;
use crate::source::SourceRegistry;
use crate::state::State;

/// One install, fully specified.
#[derive(Debug, Clone)]
pub struct InstallRequest {
    pub spec: PackageSpec,
    /// Reinstall even when the resolved version is already present.
    pub force: bool,
    pub prerelease: bool,
    /// Create links in the bin dir (and copy `.app`s) after unpacking.
    pub link: bool,
    /// Fail rather than record a first-seen hash when no checksum is published.
    pub require_checksum: bool,
    /// Exact asset file name, bypassing scoring.
    pub asset_override: Option<String>,
}

impl InstallRequest {
    pub fn new(spec: PackageSpec) -> Self {
        InstallRequest {
            spec,
            force: false,
            prerelease: false,
            link: true,
            require_checksum: false,
            asset_override: None,
        }
    }
}

/// What an install actually did.
#[derive(Debug, Clone)]
pub struct Installed {
    pub package: InstalledPackage,
    /// The version that was replaced, when this was an upgrade or reinstall.
    pub replaced: Option<Version>,
}

/// An asset and the score that won it the selection.
#[derive(Debug, Clone)]
pub struct ScoredAsset {
    pub asset: ReleaseAsset,
    pub score: AssetScore,
}

/// Run the pipeline. Mutates `state` in memory; the caller saves it, so a batch
/// install writes `state.json` once.
pub fn install(
    _cfg: &Config,
    _sources: &SourceRegistry,
    _state: &mut State,
    _req: &InstallRequest,
) -> Result<Installed> {
    todo!("install pipeline")
}

/// Remove links and the store directory, then drop the state entry.
pub fn uninstall(_cfg: &Config, _state: &mut State, _name: &str) -> Result<InstalledPackage> {
    todo!("uninstall")
}

/// Re-create links for an already-installed package.
pub fn relink(_cfg: &Config, _state: &mut State, _name: &str) -> Result<()> {
    todo!("relink")
}

/// Remove links but keep the package installed.
pub fn unlink(_cfg: &Config, _state: &mut State, _name: &str) -> Result<()> {
    todo!("unlink")
}

/// Newest release available for an installed package, for `outdated`/`upgrade`.
pub fn latest_release(
    _sources: &SourceRegistry,
    _pkg: &InstalledPackage,
    _prerelease: bool,
) -> Result<Release> {
    todo!("latest_release")
}

/// Rank a release's assets for this platform, best first. Assets the platform
/// rejects are dropped, so an empty result means nothing here is installable.
pub fn score_assets(
    _cfg: &Config,
    _platform: &dyn crate::platform::Platform,
    _release: &Release,
    _selector: &crate::model::AssetSelector,
) -> Vec<ScoredAsset> {
    todo!("score_assets")
}
