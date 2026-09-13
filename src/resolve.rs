//! Side-effect-free resolution trace.
//!
//! Install and `ketch why` share these functions so an explanation cannot drift
//! from what would actually be installed. Nothing here downloads, fetches
//! checksum files, or talks to the network beyond the single `Source::resolve`
//! call install already makes.

use crate::config::Config;
use crate::error::Result;
use crate::manifest::Resolver;
use crate::model::{
    glob_match, AssetSelector, Manifest, ManifestOrigin, PackageSpec, Release, ReleaseAsset,
};
use crate::platform::{AssetScore, Platform};
use crate::source::{ListOpts, RejectedRelease, Source, SourceRegistry};
use crate::ui;
use serde::Serialize;

/// A release asset ranked for this platform.
#[derive(Debug, Clone)]
pub struct ScoredAsset {
    pub asset: ReleaseAsset,
    pub score: AssetScore,
}

/// An asset that scoring refused, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RejectedAsset {
    pub name: String,
    pub reason: String,
}

/// Ranked assets plus the ones that did not make the cut.
#[derive(Debug, Clone)]
pub struct AssetEvaluation {
    pub scored: Vec<ScoredAsset>,
    pub rejected: Vec<RejectedAsset>,
}

/// Rank a release's assets for this platform, best first.
///
/// Assets the platform or the manifest rejects are listed on `rejected` rather
/// than dropped, so `ketch why` can show the same decision install will make.
pub fn evaluate_assets(
    cfg: &Config,
    platform: &dyn Platform,
    release: &Release,
    selector: &AssetSelector,
) -> AssetEvaluation {
    let target_pattern = selector.target.get(&cfg.target.to_string());
    let mut scored: Vec<ScoredAsset> = Vec::new();
    let mut rejected: Vec<RejectedAsset> = Vec::new();

    for asset in &release.assets {
        if let Some(pattern) = selector.exclude.iter().find(|p| glob_match(p, &asset.name)) {
            rejected.push(RejectedAsset {
                name: asset.name.clone(),
                reason: format!("excluded by `{pattern}`"),
            });
            continue;
        }

        // A per-target pattern is the user naming the file outright, so it
        // overrides the platform's opinion rather than filtering it.
        if let Some(pattern) = target_pattern {
            if glob_match(pattern, &asset.name) {
                scored.push(ScoredAsset {
                    asset: asset.clone(),
                    score: AssetScore {
                        score: i32::MAX,
                        arch: cfg.target.arch,
                        emulated: false,
                        reason: format!("manifest pins `{pattern}` for {}", cfg.target),
                    },
                });
            } else {
                rejected.push(RejectedAsset {
                    name: asset.name.clone(),
                    reason: format!(
                        "does not match the pinned pattern `{pattern}` for {}",
                        cfg.target
                    ),
                });
            }
            continue;
        }

        if !selector.include.is_empty()
            && !selector.include.iter().any(|p| glob_match(p, &asset.name))
        {
            rejected.push(RejectedAsset {
                name: asset.name.clone(),
                reason: "does not match any include pattern".into(),
            });
            continue;
        }
        match platform.score_asset(&asset.name, cfg.allow_emulation) {
            Some(score) => scored.push(ScoredAsset {
                asset: asset.clone(),
                score,
            }),
            None => rejected.push(RejectedAsset {
                name: asset.name.clone(),
                reason: format!("incompatible with {}", cfg.target),
            }),
        }
    }

    // Name is the tie-break so repeated runs pick the same asset.
    scored.sort_by(|a, b| {
        b.score
            .score
            .cmp(&a.score.score)
            .then_with(|| a.asset.name.cmp(&b.asset.name))
    });
    rejected.sort_by(|a, b| a.name.cmp(&b.name));
    AssetEvaluation { scored, rejected }
}

/// Ranked assets only, the shape install and `info --assets` already use.
pub fn score_assets(
    cfg: &Config,
    platform: &dyn Platform,
    release: &Release,
    selector: &AssetSelector,
) -> Vec<ScoredAsset> {
    evaluate_assets(cfg, platform, release, selector).scored
}

/// How a checksum would be handled if this candidate were installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChecksumPolicy {
    /// `require_checksums` from config. Sidecar files are not fetched here.
    pub require: bool,
    /// Algorithm on the chosen asset, when the release already carried one.
    pub asset_digest: Option<String>,
    /// What install would do with that information, without extra requests.
    pub policy: &'static str,
}

/// Local trust checks run after download and never refuse the install.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrustPolicy {
    pub strip_quarantine: bool,
    pub allow_emulation: bool,
    /// Platform trust is advisory: a failed check is reported, not fatal.
    pub blocks_install: bool,
}

/// Where the manifest came from, without embedding untrusted prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManifestStep {
    pub tier: &'static str,
    pub origin: String,
    pub name: String,
    pub source: String,
    /// The alias or reference the user typed, when it differs from `name`.
    pub matched: String,
}

/// The source that will answer for this package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceStep {
    pub scheme: String,
    pub id: String,
}

/// The release `Source::resolve` returned, plus how it was asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionStep {
    pub request: String,
    pub include_prerelease: bool,
    pub selected: Option<SelectedRelease>,
    /// Other listing entries, when a caller walked a listing through `pick`.
    /// Empty after `Source::resolve`: that call is one request, like install.
    pub rejected: Vec<RejectedRelease>,
}

/// The release that won version selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectedRelease {
    pub tag: String,
    pub version: String,
    pub prerelease: bool,
}

/// The asset that would be installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub name: String,
    pub score: i32,
    pub reason: String,
    pub emulated: bool,
}

/// A scored asset, stripped of URLs, headers and other untrusted control text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScoredAssetView {
    pub name: String,
    pub score: i32,
    pub reason: String,
    pub emulated: bool,
}

/// End-to-end explanation of one package decision.
#[derive(Debug, Clone, Serialize)]
pub struct ResolutionTrace {
    pub package: String,
    pub target: String,
    pub manifest: ManifestStep,
    pub source: SourceStep,
    pub version: VersionStep,
    pub assets: AssetTraceView,
    pub checksum: ChecksumPolicy,
    pub trust: TrustPolicy,
    pub candidate: Option<Candidate>,
}

/// Scored and rejected assets as they appear in `ketch why`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssetTraceView {
    pub scored: Vec<ScoredAssetView>,
    pub rejected: Vec<RejectedAsset>,
}

/// The listing options install and `why` both pass into `Source::resolve`.
pub fn list_opts(cfg: &Config, manifest: &Manifest, extra_prerelease: bool) -> ListOpts {
    ListOpts {
        include_prerelease: extra_prerelease || cfg.prerelease || manifest.prerelease,
        ..Default::default()
    }
}

/// Explain a package the way install would resolve it, without downloading.
///
/// One `Source::resolve` call, the same function install uses. Checksum
/// sidecars and trust inspection are described from config and the already
/// fetched release, not performed.
pub fn explain(
    cfg: &Config,
    sources: &SourceRegistry,
    spec: &PackageSpec,
) -> Result<ResolutionTrace> {
    let (manifest, origin) = Resolver::new(cfg)?.resolve(spec)?;
    let source = sources.for_ref(&manifest.source)?;
    explain_with(cfg, source.as_ref(), spec, manifest, origin)
}

fn explain_with(
    cfg: &Config,
    source: &dyn Source,
    spec: &PackageSpec,
    manifest: Manifest,
    origin: ManifestOrigin,
) -> Result<ResolutionTrace> {
    // Same helper install uses. `why` has no `--prerelease` flag, so the extra
    // bit is false; config and the manifest still match a default install.
    let opts = list_opts(cfg, &manifest, false);
    let include_prerelease = opts.include_prerelease;
    let selected = match source.resolve(&manifest.source.id, &spec.version, &opts) {
        Ok(release) => Some(release),
        Err(crate::error::Error::NoRelease(_)) => None,
        Err(e) => return Err(e),
    };

    let platform = crate::platform::host()?;
    let evaluation = match &selected {
        Some(release) => evaluate_assets(cfg, platform.as_ref(), release, &manifest.asset),
        None => AssetEvaluation {
            scored: Vec::new(),
            rejected: Vec::new(),
        },
    };
    let candidate = evaluation.scored.first().map(|s| Candidate {
        name: safe(&s.asset.name),
        score: s.score.score,
        reason: safe(&s.score.reason),
        emulated: s.score.emulated,
    });

    let checksum = checksum_policy(cfg, evaluation.scored.first().map(|s| &s.asset));
    Ok(ResolutionTrace {
        package: safe(&manifest.name),
        target: cfg.target.to_string(),
        manifest: ManifestStep {
            tier: origin.tier(),
            origin: safe(&origin.location()),
            name: safe(&manifest.name),
            source: safe(&manifest.source.to_string()),
            matched: safe(&spec.label()),
        },
        source: SourceStep {
            scheme: safe(&manifest.source.scheme),
            id: safe(&manifest.source.id),
        },
        version: VersionStep {
            request: spec.version.to_string(),
            include_prerelease,
            selected: selected.as_ref().map(|r| SelectedRelease {
                tag: safe(&r.tag),
                version: r.version.to_string(),
                prerelease: r.prerelease,
            }),
            rejected: Vec::new(),
        },
        assets: AssetTraceView {
            scored: evaluation
                .scored
                .iter()
                .map(|s| ScoredAssetView {
                    name: safe(&s.asset.name),
                    score: s.score.score,
                    reason: safe(&s.score.reason),
                    emulated: s.score.emulated,
                })
                .collect(),
            rejected: evaluation
                .rejected
                .into_iter()
                .map(|r| RejectedAsset {
                    name: safe(&r.name),
                    reason: safe(&r.reason),
                })
                .collect(),
        },
        checksum,
        trust: TrustPolicy {
            strip_quarantine: cfg.strip_quarantine,
            allow_emulation: cfg.allow_emulation,
            blocks_install: false,
        },
        candidate,
    })
}

fn checksum_policy(cfg: &Config, asset: Option<&ReleaseAsset>) -> ChecksumPolicy {
    let asset_digest = asset.and_then(|a| {
        a.digest
            .as_ref()
            .and_then(|d| d.algo.eq_ignore_ascii_case("sha256").then(|| safe(&d.algo)))
    });
    let policy = match (&asset_digest, cfg.require_checksums) {
        (None, _) if asset.is_none() => "not-applicable",
        (Some(_), _) => "published-digest",
        (None, true) => "require",
        (None, false) => "trust-on-first-use",
    };
    ChecksumPolicy {
        require: cfg.require_checksums,
        asset_digest,
        policy,
    }
}

fn safe(text: &str) -> String {
    ui::printable(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Arch, Os, TargetSpec, Version};
    use crate::platform::Platform;
    use pretty_assertions::assert_eq;
    use std::collections::BTreeMap;
    use std::path::Path;

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
        fn place(
            &self,
            _plan: &crate::platform::Placement<'_>,
        ) -> Result<Vec<crate::model::LinkRecord>> {
            Ok(Vec::new())
        }
        fn unplace(&self, _links: &[crate::model::LinkRecord]) -> Result<()> {
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
            url: format!("https://example.invalid/{name}?token=SECRET"),
            size: 1,
            content_type: None,
            digest: None,
            headers: BTreeMap::from([("authorization".into(), "secret-token".into())]),
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
        let mut cfg = Config::load(Some(std::env::temp_dir().join("ketch-why-test-root"))).unwrap();
        cfg.target = TargetSpec {
            os: Os::MacOs,
            arch: Arch::Aarch64,
        };
        cfg
    }

    #[test]
    fn evaluate_assets_matches_score_assets_and_keeps_rejects() {
        let eval = evaluate_assets(
            &config(),
            &FakePlatform,
            &release(&["tool-linux.tar.gz", "tool-darwin.tar.gz"]),
            &AssetSelector::default(),
        );
        assert_eq!(eval.scored.len(), 1);
        assert_eq!(eval.scored[0].asset.name, "tool-darwin.tar.gz");
        assert_eq!(
            eval.rejected,
            vec![RejectedAsset {
                name: "tool-linux.tar.gz".into(),
                reason: "incompatible with macos-aarch64".into(),
            }]
        );
        let scored = score_assets(
            &config(),
            &FakePlatform,
            &release(&["tool-linux.tar.gz", "tool-darwin.tar.gz"]),
            &AssetSelector::default(),
        );
        assert_eq!(scored.len(), 1);
        assert_eq!(scored[0].asset.name, eval.scored[0].asset.name);
    }

    #[test]
    fn a_target_pin_is_the_only_scored_asset() {
        let cfg = config();
        let selector = AssetSelector {
            target: BTreeMap::from([(
                "macos-aarch64".to_string(),
                "*-mac-universal.zip".to_string(),
            )]),
            ..Default::default()
        };
        let eval = evaluate_assets(
            &cfg,
            &FakePlatform,
            &release(&["tool-darwin.tar.gz", "tool-mac-universal.zip"]),
            &selector,
        );
        assert_eq!(
            eval.rejected,
            vec![RejectedAsset {
                name: "tool-darwin.tar.gz".into(),
                reason: "does not match the pinned pattern `*-mac-universal.zip` for macos-aarch64"
                    .into(),
            }]
        );
        assert_eq!(eval.scored.len(), 1);
        assert_eq!(eval.scored[0].asset.name, "tool-mac-universal.zip");
        assert_eq!(eval.scored[0].score.score, i32::MAX);
    }

    #[test]
    fn exclude_wins_over_include_and_over_the_target_pin() {
        let cfg = config();
        let selector = AssetSelector {
            include: vec!["*darwin*".into()],
            exclude: vec!["*.dmg".into()],
            target: BTreeMap::from([("macos-aarch64".to_string(), "*.dmg".to_string())]),
        };
        let eval = evaluate_assets(
            &cfg,
            &FakePlatform,
            &release(&["tool-darwin.dmg", "tool-darwin.tar.gz"]),
            &selector,
        );
        assert!(eval.scored.is_empty());
        assert_eq!(
            eval.rejected,
            vec![
                RejectedAsset {
                    name: "tool-darwin.dmg".into(),
                    reason: "excluded by `*.dmg`".into(),
                },
                RejectedAsset {
                    name: "tool-darwin.tar.gz".into(),
                    reason: "does not match the pinned pattern `*.dmg` for macos-aarch64".into(),
                },
            ]
        );
    }

    #[test]
    fn no_compatible_asset_leaves_every_candidate_rejected() {
        let eval = evaluate_assets(
            &config(),
            &FakePlatform,
            &release(&["tool-linux.tar.gz", "SHA256SUMS"]),
            &AssetSelector::default(),
        );
        assert!(eval.scored.is_empty());
        assert_eq!(
            eval.rejected,
            vec![
                RejectedAsset {
                    name: "SHA256SUMS".into(),
                    reason: "incompatible with macos-aarch64".into(),
                },
                RejectedAsset {
                    name: "tool-linux.tar.gz".into(),
                    reason: "incompatible with macos-aarch64".into(),
                },
            ]
        );
    }

    #[test]
    fn include_patterns_reject_assets_that_do_not_match() {
        let selector = AssetSelector {
            include: vec!["*darwin*".into()],
            ..Default::default()
        };
        let eval = evaluate_assets(
            &config(),
            &FakePlatform,
            &release(&["tool-linux.tar.gz", "tool-darwin.tar.gz"]),
            &selector,
        );
        assert_eq!(eval.scored.len(), 1);
        assert_eq!(
            eval.rejected,
            vec![RejectedAsset {
                name: "tool-linux.tar.gz".into(),
                reason: "does not match any include pattern".into(),
            }]
        );
    }

    #[test]
    fn a_trace_view_strips_control_text_and_drops_secrets() {
        let mut rel = release(&["tool-darwin.tar.gz"]);
        rel.assets[0].name = "safe\u{202e}evil-darwin.tar.gz".into();
        let eval = evaluate_assets(&config(), &FakePlatform, &rel, &AssetSelector::default());
        let view = ScoredAssetView {
            name: safe(&eval.scored[0].asset.name),
            score: eval.scored[0].score.score,
            reason: safe(&eval.scored[0].score.reason),
            emulated: false,
        };
        assert!(!view.name.contains('\u{202e}'));
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("SECRET"));
        assert!(!json.contains("secret-token"));
        assert!(!json.contains("example.invalid"));
        assert!(!json.contains("authorization"));
    }

    #[test]
    fn origin_tiers_name_the_four_manifest_sources() {
        use std::path::PathBuf;
        assert_eq!(
            vec![
                ManifestOrigin::User(PathBuf::from("manifests/tool.toml")).tier(),
                ManifestOrigin::Registry(PathBuf::from("registry/tool/ketch.toml")).tier(),
                ManifestOrigin::Builtin.tier(),
                ManifestOrigin::Inferred.tier(),
            ],
            vec!["user", "registry", "builtin", "inferred"]
        );
    }
}
