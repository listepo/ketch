//! The install pipeline.
//!
//! resolve manifest → resolve release → score and pick an asset → download →
//! verify checksum → extract → verify trust → place → record state.
//!
//! Every step is expressed against the `Source`, `Platform` and `Extractor`
//! traits, so this file contains no GitHub-specific and no macOS-specific code.

use crate::config::{sanitize_component, Config};
use crate::error::{Error, Result};
use crate::manifest::Resolver;
use crate::model::{
    glob_match, now_unix, AssetSelector, InstalledPackage, LinkRecord, PackageSpec, Release,
    ReleaseAsset, Version, VersionSpec,
};
use crate::platform::{AssetScore, Placement, Platform, TrustVerdict};
use crate::source::{ListOpts, SourceRegistry};
use crate::state::State;
use crate::ui;
use std::path::{Path, PathBuf};

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
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
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
    cfg: &Config,
    sources: &SourceRegistry,
    state: &mut State,
    req: &InstallRequest,
) -> Result<Installed> {
    let platform = crate::platform::host()?;
    let (manifest, origin) = Resolver::new(cfg)?.resolve(&req.spec)?;
    let source = sources.for_ref(&manifest.source)?;

    let opts = ListOpts {
        include_prerelease: req.prerelease || cfg.prerelease || manifest.prerelease,
        ..Default::default()
    };
    ui::step(
        "resolving",
        &format!("{} ({})", manifest.name, manifest.source),
    );
    let release = source.resolve(&manifest.source.id, &req.spec.version, &opts)?;

    // Nothing is downloaded until we know the install is actually wanted.
    let existing = state.get(&manifest.name).cloned();
    if let Some(old) = &existing {
        if old.pinned && !matches!(req.spec.version, VersionSpec::Exact(_)) {
            return Err(Error::Pinned {
                name: old.name.clone(),
                version: old.version.to_string(),
            });
        }
        if old.tag == release.tag && !req.force {
            return Err(Error::AlreadyInstalled {
                name: old.name.clone(),
                version: old.version.to_string(),
            });
        }
    }

    let chosen = choose_asset(cfg, platform.as_ref(), &release, &manifest, req)?;
    let asset = chosen.asset;
    ui::debug(&format!(
        "selected {} — {}",
        asset.name, chosen.score.reason
    ));
    if chosen.score.emulated {
        ui::warn(&format!(
            "{} is an {} build and will run under emulation",
            asset.name, chosen.score.arch
        ));
    }

    // --- download -----------------------------------------------------------
    std::fs::create_dir_all(&cfg.cache_dir).map_err(|e| Error::io(&cfg.cache_dir, e))?;
    let download_path = cfg.cache_dir.join(format!(
        "{}-{}",
        sanitize_component(&manifest.name),
        sanitize_component(&asset.name)
    ));
    let progress = ui::progress();
    let sha256 = source.download(&asset, &download_path, progress.as_ref())?;
    // The cache entry has served its purpose once the payload is in the store.
    let _cleanup = ScopedFile(download_path.clone());

    // --- checksum -----------------------------------------------------------
    let checksum_verified = verify_checksum(
        source.as_ref(),
        &manifest.source.id,
        &release,
        &asset,
        &sha256,
        req.require_checksum || cfg.require_checksums,
    )?;

    // --- extract ------------------------------------------------------------
    let unpack = tempfile::tempdir_in(&cfg.cache_dir).map_err(|e| Error::io(&cfg.cache_dir, e))?;
    let format =
        crate::extract::extract_auto(&download_path, unpack.path(), &platform.extractors())?;
    ui::debug(&format!("unpacked {} as {format}", asset.name));
    let payload = payload_root(unpack.path(), manifest.strip_prefix)?;

    check_trust(platform.as_ref(), cfg, &payload, &manifest.name);

    // --- place --------------------------------------------------------------
    let version = release.version.to_string();
    let store_dir = cfg.package_dir(&manifest.name, &version);
    // Reinstalling the same version writes into the directory the current
    // install already occupies, and failing there must not delete it.
    let in_place = existing.as_ref().is_some_and(|p| p.prefix == store_dir);
    let mut orphan = ScopedDir((!in_place).then(|| store_dir.clone()));
    let links = platform.place(&Placement {
        name: &manifest.name,
        version: &version,
        payload_dir: &payload,
        store_dir: &store_dir,
        bin_dir: &cfg.bin_dir,
        apps_dir: &cfg.apps_dir,
        kind: manifest.kind,
        bin_specs: &manifest.bin,
        replacing: existing.as_ref().map(|p| p.links.as_slice()).unwrap_or(&[]),
        link_apps: cfg.link_apps,
        link: req.link,
    })?;

    // --- retire the version we replaced -------------------------------------
    if let Some(old) = &existing {
        let stale: Vec<LinkRecord> = old
            .links
            .iter()
            .filter(|l| !links.iter().any(|new| new.link == l.link))
            .cloned()
            .collect();
        // A failure here leaves a dangling link, not a broken install, so it is
        // reported rather than propagated.
        if let Err(e) = platform.unplace(&stale) {
            ui::warn(&format!("could not remove old links for {}: {e}", old.name));
        }
        if old.prefix != store_dir {
            remove_store_dir(cfg, &old.prefix);
        }
    }

    let package = InstalledPackage {
        name: manifest.name.clone(),
        version: release.version.clone(),
        source: manifest.source.clone(),
        tag: release.tag.clone(),
        target: platform.target(),
        asset_name: asset.name.clone(),
        sha256,
        checksum_verified,
        installed_at: now_unix(),
        prefix: store_dir,
        links,
        pinned: existing.as_ref().is_some_and(|p| p.pinned),
        origin,
        manifest: Some(manifest),
    };
    state.insert(package.clone());
    orphan.keep();

    Ok(Installed {
        package,
        replaced: existing.map(|p| p.version),
    })
}

/// Remove links and the store directory, then drop the state entry.
pub fn uninstall(cfg: &Config, state: &mut State, name: &str) -> Result<InstalledPackage> {
    let pkg = state
        .find(name)
        .cloned()
        .ok_or_else(|| Error::NotInstalled(name.to_string()))?;
    let platform = crate::platform::host()?;
    platform.unplace(&pkg.links)?;
    remove_store_dir(cfg, &pkg.prefix);
    state.remove(&pkg.name);
    Ok(pkg)
}

/// Re-create links for an already-installed package.
pub fn relink(cfg: &Config, state: &mut State, name: &str) -> Result<()> {
    let pkg = state
        .find(name)
        .cloned()
        .ok_or_else(|| Error::NotInstalled(name.to_string()))?;
    if !pkg.prefix.is_dir() {
        return Err(Error::EmptyPayload(pkg.prefix.clone()));
    }
    let platform = crate::platform::host()?;
    platform.unplace(&pkg.links)?;

    let manifest = pkg.manifest.clone();
    let version = pkg.version.to_string();
    let links = platform.place(&Placement {
        name: &pkg.name,
        version: &version,
        // Already in the store: placement is idempotent over its own output.
        payload_dir: &pkg.prefix,
        store_dir: &pkg.prefix,
        bin_dir: &cfg.bin_dir,
        apps_dir: &cfg.apps_dir,
        kind: manifest.as_ref().map(|m| m.kind).unwrap_or_default(),
        bin_specs: manifest.as_ref().map(|m| m.bin.as_slice()).unwrap_or(&[]),
        // `unplace` above only removed links that still pointed at us, so
        // anything left over is ours to reclaim.
        replacing: &pkg.links,
        link_apps: cfg.link_apps,
        link: true,
    })?;

    if let Some(entry) = state.get_mut(&pkg.name) {
        entry.links = links;
    }
    Ok(())
}

/// Remove links but keep the package installed.
pub fn unlink(_cfg: &Config, state: &mut State, name: &str) -> Result<()> {
    let pkg = state
        .find(name)
        .cloned()
        .ok_or_else(|| Error::NotInstalled(name.to_string()))?;
    crate::platform::host()?.unplace(&pkg.links)?;
    if let Some(entry) = state.get_mut(&pkg.name) {
        entry.links.clear();
    }
    Ok(())
}

/// Newest release available for an installed package, for `outdated`/`upgrade`.
pub fn latest_release(
    sources: &SourceRegistry,
    pkg: &InstalledPackage,
    prerelease: bool,
) -> Result<Release> {
    let source = sources.for_ref(&pkg.source)?;
    let opts = ListOpts {
        include_prerelease: prerelease,
        ..Default::default()
    };
    source.resolve(&pkg.source.id, &VersionSpec::Latest, &opts)
}

/// Rank a release's assets for this platform, best first. Assets the platform
/// rejects are dropped, so an empty result means nothing here is installable.
pub fn score_assets(
    cfg: &Config,
    platform: &dyn Platform,
    release: &Release,
    selector: &AssetSelector,
) -> Vec<ScoredAsset> {
    let target_pattern = selector.target.get(&cfg.target.to_string());
    let mut out: Vec<ScoredAsset> = Vec::new();

    for asset in &release.assets {
        if selector.exclude.iter().any(|p| glob_match(p, &asset.name)) {
            continue;
        }

        // A per-target pattern is the user naming the file outright, so it
        // overrides the platform's opinion rather than filtering it.
        if let Some(pattern) = target_pattern {
            if glob_match(pattern, &asset.name) {
                out.push(ScoredAsset {
                    asset: asset.clone(),
                    score: AssetScore {
                        score: i32::MAX,
                        arch: cfg.target.arch,
                        emulated: false,
                        reason: format!("manifest pins `{pattern}` for {}", cfg.target),
                    },
                });
            }
            continue;
        }

        if !selector.include.is_empty()
            && !selector.include.iter().any(|p| glob_match(p, &asset.name))
        {
            continue;
        }
        if let Some(score) = platform.score_asset(&asset.name, cfg.allow_emulation) {
            out.push(ScoredAsset {
                asset: asset.clone(),
                score,
            });
        }
    }

    // Name is the tie-break so repeated runs pick the same asset.
    out.sort_by(|a, b| {
        b.score
            .score
            .cmp(&a.score.score)
            .then_with(|| a.asset.name.cmp(&b.asset.name))
    });
    out
}

// ---------------------------------------------------------------------------
// Steps
// ---------------------------------------------------------------------------

fn choose_asset(
    cfg: &Config,
    platform: &dyn Platform,
    release: &Release,
    manifest: &crate::model::Manifest,
    req: &InstallRequest,
) -> Result<ScoredAsset> {
    if let Some(wanted) = &req.asset_override {
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == *wanted)
            .ok_or_else(|| {
                Error::msg(format!(
                    "release `{}` has no asset named `{wanted}`",
                    release.tag
                ))
            })?;
        return Ok(ScoredAsset {
            asset: asset.clone(),
            score: AssetScore {
                score: i32::MAX,
                arch: cfg.target.arch,
                emulated: false,
                reason: "chosen with --asset".to_string(),
            },
        });
    }

    score_assets(cfg, platform, release, &manifest.asset)
        .into_iter()
        .next()
        .ok_or_else(|| Error::NoCompatibleAsset {
            id: manifest.source.to_string(),
            tag: release.tag.clone(),
            target: cfg.target.to_string(),
        })
}

/// Returns whether the hash was confirmed against a published checksum.
pub(crate) fn verify_checksum(
    source: &dyn crate::source::Source,
    id: &str,
    release: &Release,
    asset: &ReleaseAsset,
    actual: &str,
    require: bool,
) -> Result<bool> {
    let published = match &asset.digest {
        Some(digest) => Some(digest.hex.clone()),
        // Only worth the extra requests when the asset carries no digest.
        None => source
            .checksums(id, release, &asset.name)
            .unwrap_or_else(|e| {
                ui::debug(&format!("could not read published checksums: {e}"));
                Default::default()
            })
            .get(&asset.name)
            .cloned(),
    };

    match published {
        Some(expected) if expected.eq_ignore_ascii_case(actual) => Ok(true),
        Some(expected) => Err(Error::ChecksumMismatch {
            name: asset.name.clone(),
            expected,
            actual: actual.to_string(),
        }),
        None if require => Err(Error::ChecksumMissing(asset.name.clone())),
        None => {
            ui::debug(&format!(
                "{} publishes no checksum; recording {} on first use",
                asset.name,
                &actual[..actual.len().min(12)]
            ));
            Ok(false)
        }
    }
}

/// Apply `strip_prefix`, or unwrap the single wrapper directory most tarballs
/// use, so the payload root is where the files actually are.
fn payload_root(unpacked: &Path, strip: Option<usize>) -> Result<PathBuf> {
    match strip {
        Some(0) | None => crate::extract::unwrap_single_dir(unpacked),
        Some(n) => {
            let mut root = unpacked.to_path_buf();
            for _ in 0..n {
                root = crate::extract::unwrap_single_dir(&root)?;
            }
            Ok(root)
        }
    }
}

/// Inspect the payload and strip quarantine only when the platform says the
/// code is genuinely trusted. A failed check never blocks an install the user
/// explicitly asked for; it is reported instead.
fn check_trust(platform: &dyn Platform, cfg: &Config, payload: &Path, name: &str) {
    let verdict = match platform.verify_trust(payload) {
        Ok(v) => v,
        Err(e) => {
            ui::debug(&format!("trust check failed for {name}: {e}"));
            return;
        }
    };
    match &verdict {
        TrustVerdict::Trusted { authority } => ui::debug(&format!("signed by {authority}")),
        TrustVerdict::Weak { detail } => ui::debug(&format!("weak signature: {detail}")),
        TrustVerdict::Untrusted { detail } => ui::debug(&format!("unsigned: {detail}")),
        TrustVerdict::NotApplicable => {}
    }
    if cfg.strip_quarantine && verdict.may_strip_quarantine() {
        if let Err(e) = platform.clear_quarantine(payload) {
            ui::debug(&format!("could not clear quarantine: {e}"));
        }
    }
}

/// Delete a store directory, and its now-empty package parent.
///
/// Refuses anything outside the store: a corrupted state file must never turn
/// an uninstall into a `rm -rf` of somewhere else.
fn remove_store_dir(cfg: &Config, prefix: &Path) {
    if !prefix.starts_with(&cfg.store_dir) || prefix == cfg.store_dir {
        ui::warn(&format!(
            "refusing to remove {} — it is not inside the ketch store",
            prefix.display()
        ));
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(prefix) {
        if e.kind() != std::io::ErrorKind::NotFound {
            ui::warn(&format!("could not remove {}: {e}", prefix.display()));
        }
    }
    if let Some(parent) = prefix.parent().filter(|p| *p != cfg.store_dir) {
        let _ = std::fs::remove_dir(parent); // only succeeds when empty
    }
}

/// Deletes a path when dropped, so a failure mid-pipeline does not leave the
/// downloaded archive behind.
struct ScopedFile(PathBuf);

impl Drop for ScopedFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Deletes a store directory when dropped, unless the install got far enough to
/// keep it.
///
/// Placement moves the payload into the store before creating any link, so a
/// failure after that point — a binary name another package already owns, a
/// payload with nothing runnable in it — leaves a full store directory that no
/// state entry mentions: invisible to `ketch list`, out of reach of `ketch
/// uninstall`, and taken for a finished install by the next run.
struct ScopedDir(Option<PathBuf>);

impl ScopedDir {
    fn keep(&mut self) {
        self.0 = None;
    }
}

impl Drop for ScopedDir {
    fn drop(&mut self) {
        if let Some(path) = &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Arch, Os, ReleaseAsset, TargetSpec};
    use std::collections::BTreeMap;

    struct FakePlatform;

    impl Platform for FakePlatform {
        fn id(&self) -> &str {
            "fake"
        }
        fn target(&self) -> TargetSpec {
            TargetSpec {
                os: Os::MacOs,
                arch: Arch::Aarch64,
            }
        }
        fn score_asset(&self, name: &str, _emu: bool) -> Option<AssetScore> {
            // Stands in for a real platform: macOS assets only, longer names
            // never outrank shorter ones by accident.
            name.contains("darwin").then(|| AssetScore {
                score: 50,
                arch: Arch::Aarch64,
                emulated: false,
                reason: "fake".into(),
            })
        }
        fn extractors(&self) -> Vec<Box<dyn crate::extract::Extractor>> {
            Vec::new()
        }
        fn place(&self, _plan: &Placement<'_>) -> Result<Vec<LinkRecord>> {
            Ok(Vec::new())
        }
        fn unplace(&self, _links: &[LinkRecord]) -> Result<()> {
            Ok(())
        }
        fn is_executable(&self, _path: &Path) -> bool {
            true
        }
        fn doctor(&self, _cfg: &Config) -> Vec<crate::platform::DoctorCheck> {
            Vec::new()
        }
    }

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            url: format!("https://example.invalid/{name}"),
            size: 1,
            content_type: None,
            digest: None,
            headers: BTreeMap::new(),
        }
    }

    fn release(names: &[&str]) -> Release {
        Release {
            version: Version::parse("1.0.0"),
            tag: "v1.0.0".into(),
            prerelease: false,
            draft: false,
            published_at: None,
            notes: None,
            assets: names.iter().map(|n| asset(n)).collect(),
        }
    }

    fn config() -> Config {
        let mut cfg = Config::load(Some(std::env::temp_dir().join("ketch-test-root"))).unwrap();
        cfg.target = TargetSpec {
            os: Os::MacOs,
            arch: Arch::Aarch64,
        };
        cfg
    }

    #[test]
    fn drops_assets_the_platform_cannot_run() {
        let picked = score_assets(
            &config(),
            &FakePlatform,
            &release(&["tool-linux.tar.gz", "tool-darwin.tar.gz"]),
            &AssetSelector::default(),
        );
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].asset.name, "tool-darwin.tar.gz");
    }

    #[test]
    fn exclude_wins_over_include_and_over_the_target_pin() {
        let cfg = config();
        let selector = AssetSelector {
            include: vec!["*darwin*".into()],
            exclude: vec!["*.dmg".into()],
            target: BTreeMap::from([("macos-aarch64".to_string(), "*.dmg".to_string())]),
        };
        let picked = score_assets(
            &cfg,
            &FakePlatform,
            &release(&["tool-darwin.dmg", "tool-darwin.tar.gz"]),
            &selector,
        );
        // The pin would have taken the dmg; the exclusion removes it first, and
        // with a pin present the tarball is not considered either.
        assert!(picked.is_empty());
    }

    #[test]
    fn a_target_pin_overrides_platform_scoring() {
        let cfg = config();
        let selector = AssetSelector {
            target: BTreeMap::from([(
                "macos-aarch64".to_string(),
                "*-mac-universal.zip".to_string(),
            )]),
            ..Default::default()
        };
        // `mac` alone would score None from FakePlatform; the pin still wins.
        let picked = score_assets(
            &cfg,
            &FakePlatform,
            &release(&["tool-darwin.tar.gz", "tool-mac-universal.zip"]),
            &selector,
        );
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].asset.name, "tool-mac-universal.zip");
    }

    #[test]
    fn refuses_to_delete_outside_the_store() {
        let cfg = config();
        let outside = std::env::temp_dir().join("ketch-not-the-store");
        std::fs::create_dir_all(&outside).unwrap();
        remove_store_dir(&cfg, &outside);
        assert!(outside.is_dir(), "a path outside the store must survive");
        std::fs::remove_dir_all(&outside).ok();
    }
}
