//! Where packages come from.
//!
//! A `Source` turns an opaque id into releases and downloadable assets. GitHub
//! is built in; anything else can be added as an external plugin executable
//! without recompiling ketch (see `plugin.rs` and `docs/PLUGINS.md`).

pub mod github;
pub mod plugin;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::http::Http;
use crate::model::{PackageRef, Release, ReleaseAsset, SourceInfo, VersionSpec};
use crate::ui::ProgressSink;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Knobs that apply to listing releases, independent of the source.
#[derive(Debug, Clone)]
pub struct ListOpts {
    pub include_prerelease: bool,
    /// Upper bound on releases fetched. Sources may return fewer.
    pub limit: usize,
}

impl Default for ListOpts {
    fn default() -> Self {
        ListOpts {
            include_prerelease: false,
            limit: 30,
        }
    }
}

/// A backend that can enumerate and fetch releases.
///
/// Implementations must be usable from multiple threads; ketch keeps one
/// instance per scheme for the life of the process.
pub trait Source: Send + Sync {
    /// The scheme this source answers to, e.g. `github`. Must be stable — it
    /// appears in user input and in recorded state.
    fn scheme(&self) -> &str;

    /// Repository-level metadata. Optional: return `Ok(None)` when the source
    /// has nothing beyond releases.
    fn describe(&self, _id: &str) -> Result<Option<SourceInfo>> {
        Ok(None)
    }

    /// Releases, newest first. Drafts must be excluded; prereleases are
    /// included only when `opts.include_prerelease` is set.
    fn list_releases(&self, id: &str, opts: &ListOpts) -> Result<Vec<Release>>;

    /// Resolve a version request to one release.
    ///
    /// The default walks `list_releases`, which is correct for every source.
    /// Override only to use a cheaper endpoint (GitHub does, for `latest`).
    fn resolve(&self, id: &str, want: &VersionSpec, opts: &ListOpts) -> Result<Release> {
        let releases = self.list_releases(id, opts)?;
        pick(id, releases, want, opts)
    }

    /// Checksums published alongside a release, keyed by asset file name.
    fn checksums(&self, _id: &str, _release: &Release) -> Result<BTreeMap<String, String>> {
        Ok(BTreeMap::new())
    }

    /// Download one asset to `dest`, returning its SHA-256 as lowercase hex.
    fn download(
        &self,
        asset: &ReleaseAsset,
        dest: &Path,
        progress: &dyn ProgressSink,
    ) -> Result<String>;

    /// Free-text search. Sources that cannot search return an empty list.
    fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SourceInfo>> {
        Ok(Vec::new())
    }

    /// A browsable URL for humans, when one exists.
    fn web_url(&self, _id: &str) -> Option<String> {
        None
    }
}

/// Shared release-selection logic, so every source picks versions the same way.
pub fn pick(
    id: &str,
    mut releases: Vec<Release>,
    want: &VersionSpec,
    opts: &ListOpts,
) -> Result<Release> {
    releases.retain(|r| !r.draft);
    match want {
        VersionSpec::Exact(tag) => releases
            .into_iter()
            // An exact request is explicit consent to install a prerelease, so
            // the prerelease filter is deliberately not applied here.
            .find(|r| r.tag.eq_ignore_ascii_case(tag) || r.version.matches_request(tag))
            .ok_or_else(|| Error::NoRelease(format!("{id}@{tag}"))),
        VersionSpec::Latest => {
            if !opts.include_prerelease {
                let stable: Vec<Release> = releases
                    .iter()
                    .filter(|r| !r.prerelease && !r.version.is_prerelease())
                    .cloned()
                    .collect();
                if !stable.is_empty() {
                    releases = stable;
                }
            }
            releases
                .into_iter()
                .max_by(|a, b| a.version.cmp(&b.version))
                .ok_or_else(|| Error::NoRelease(id.to_string()))
        }
    }
}

/// Every source available this run, resolved by scheme.
pub struct SourceRegistry {
    sources: Vec<Arc<dyn Source>>,
}

impl SourceRegistry {
    /// Built-in sources plus every discovered plugin. Plugin discovery failures
    /// are reported as warnings rather than aborting the command: a broken
    /// third-party plugin must not make `ketch install owner/repo` fail.
    pub fn load(cfg: &Config) -> Self {
        let http = Arc::new(Http::new(cfg));
        let mut sources: Vec<Arc<dyn Source>> =
            vec![Arc::new(github::GitHubSource::new(http.clone()))];

        for found in plugin::discover(cfg) {
            match found {
                Ok(p) => {
                    crate::ui::debug(&format!("plugin `{}` provides `{}`", p.name(), p.scheme()));
                    sources.push(Arc::new(p));
                }
                Err(e) => crate::ui::warn(&format!("ignoring plugin: {e}")),
            }
        }
        SourceRegistry { sources }
    }

    /// Only the built-in GitHub source. Used by self-update, which must not
    /// depend on third-party plugins.
    pub fn builtin_only(cfg: &Config) -> Self {
        let http = Arc::new(Http::new(cfg));
        SourceRegistry {
            sources: vec![Arc::new(github::GitHubSource::new(http))],
        }
    }

    pub fn get(&self, scheme: &str) -> Result<Arc<dyn Source>> {
        self.sources
            .iter()
            .find(|s| s.scheme().eq_ignore_ascii_case(scheme))
            .cloned()
            .ok_or_else(|| Error::UnknownScheme(scheme.to_string()))
    }

    pub fn for_ref(&self, reference: &PackageRef) -> Result<Arc<dyn Source>> {
        self.get(&reference.scheme)
    }

    pub fn schemes(&self) -> Vec<&str> {
        self.sources.iter().map(|s| s.scheme()).collect()
    }

    pub fn all(&self) -> &[Arc<dyn Source>] {
        &self.sources
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Version;

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            version: Version::parse(tag),
            tag: tag.to_string(),
            prerelease,
            draft: false,
            published_at: None,
            notes: None,
            assets: Vec::new(),
        }
    }

    #[test]
    fn latest_prefers_highest_stable() {
        let releases = vec![
            release("v1.2.0", false),
            release("v2.0.0-rc.1", true),
            release("v1.10.0", false),
        ];
        let got = pick("x", releases, &VersionSpec::Latest, &ListOpts::default()).unwrap();
        assert_eq!(got.tag, "v1.10.0");
    }

    #[test]
    fn latest_uses_prerelease_when_asked() {
        let releases = vec![release("v1.2.0", false), release("v2.0.0-rc.1", true)];
        let opts = ListOpts {
            include_prerelease: true,
            ..Default::default()
        };
        let got = pick("x", releases, &VersionSpec::Latest, &opts).unwrap();
        assert_eq!(got.tag, "v2.0.0-rc.1");
    }

    #[test]
    fn latest_falls_back_to_prerelease_when_no_stable_exists() {
        let releases = vec![release("v0.1.0-alpha", true)];
        let got = pick("x", releases, &VersionSpec::Latest, &ListOpts::default()).unwrap();
        assert_eq!(got.tag, "v0.1.0-alpha");
    }

    #[test]
    fn exact_matches_with_or_without_v_prefix() {
        let releases = vec![release("v1.2.0", false)];
        let got = pick(
            "x",
            releases.clone(),
            &VersionSpec::Exact("1.2.0".into()),
            &ListOpts::default(),
        )
        .unwrap();
        assert_eq!(got.tag, "v1.2.0");
        assert!(pick(
            "x",
            releases,
            &VersionSpec::Exact("9.9.9".into()),
            &ListOpts::default()
        )
        .is_err());
    }

    #[test]
    fn drafts_are_never_selected() {
        let mut draft = release("v3.0.0", false);
        draft.draft = true;
        let releases = vec![draft, release("v1.0.0", false)];
        let got = pick("x", releases, &VersionSpec::Latest, &ListOpts::default()).unwrap();
        assert_eq!(got.tag, "v1.0.0");
    }
}
