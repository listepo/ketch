//! The GitHub releases source.
//!
//! Works unauthenticated; a token only raises the rate limit and unlocks
//! private repositories.

use super::{ListOpts, Source};
use crate::error::Result;
use crate::http::Http;
use crate::model::{Release, ReleaseAsset, SourceInfo, VersionSpec};
use crate::ui::ProgressSink;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Override for GitHub Enterprise, via `KETCH_GITHUB_API`.
pub const DEFAULT_API: &str = "https://api.github.com";

pub struct GitHubSource {
    _http: Arc<Http>,
}

impl GitHubSource {
    pub fn new(http: Arc<Http>) -> Self {
        GitHubSource { _http: http }
    }
}

impl Source for GitHubSource {
    fn scheme(&self) -> &str {
        "github"
    }

    fn describe(&self, _id: &str) -> Result<Option<SourceInfo>> {
        todo!("GET /repos/<owner>/<repo>")
    }

    fn list_releases(&self, _id: &str, _opts: &ListOpts) -> Result<Vec<Release>> {
        todo!("GET /repos/<owner>/<repo>/releases")
    }

    fn resolve(&self, _id: &str, _want: &VersionSpec, _opts: &ListOpts) -> Result<Release> {
        todo!("use /releases/latest for the common case, else fall back to super::pick")
    }

    fn checksums(&self, _id: &str, _release: &Release) -> Result<BTreeMap<String, String>> {
        todo!("parse SHA256SUMS-style assets and per-asset .sha256 sidecars")
    }

    fn download(
        &self,
        _asset: &ReleaseAsset,
        _dest: &Path,
        _progress: &dyn ProgressSink,
    ) -> Result<String> {
        todo!("stream the asset through Http::download")
    }

    fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SourceInfo>> {
        todo!("GET /search/repositories")
    }

    fn web_url(&self, _id: &str) -> Option<String> {
        todo!("https://github.com/ plus the repo id")
    }
}
