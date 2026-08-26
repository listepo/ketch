//! The GitHub releases source.
//!
//! Works unauthenticated; a token only raises the rate limit and unlocks
//! private repositories.

use super::{ListOpts, Source};
use crate::config::validate_repo;
use crate::error::Result;
use crate::http::Http;
use crate::model::{Checksum, Release, ReleaseAsset, SourceInfo, Version, VersionSpec};
use crate::ui::ProgressSink;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Override for GitHub Enterprise, via `KETCH_GITHUB_API`.
pub const DEFAULT_API: &str = "https://api.github.com";

/// How many checksum files one release may cost us in requests.
///
/// A release with fifty assets would otherwise mean fifty extra round trips
/// before the first byte of the download.
const MAX_CHECKSUM_FETCHES: usize = 12;

pub struct GitHubSource {
    http: Arc<Http>,
    api: String,
}

impl GitHubSource {
    pub fn new(http: Arc<Http>) -> Self {
        let api = std::env::var("KETCH_GITHUB_API")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_API.to_string());
        GitHubSource {
            http,
            api: api.trim_end_matches('/').to_string(),
        }
    }

    fn repo_url(&self, id: &str, suffix: &str) -> Result<String> {
        let repo = validate_repo("GitHub repository", id.to_string())?;
        Ok(format!("{}/repos/{}{}", self.api, repo, suffix))
    }
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    content_type: Option<String>,
    /// Present on newer releases as `sha256:<hex>`.
    #[serde(default)]
    digest: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhRepo {
    full_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    stargazers_count: Option<u64>,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    license: Option<GhLicense>,
}

#[derive(Debug, Deserialize)]
struct GhLicense {
    #[serde(default)]
    spdx_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GhSearch {
    #[serde(default)]
    items: Vec<GhRepo>,
}

impl From<GhAsset> for ReleaseAsset {
    fn from(asset: GhAsset) -> Self {
        ReleaseAsset {
            name: asset.name,
            // Deliberately the public URL rather than the API one: the API
            // redirects to a different host, and following that redirect with
            // an Authorization header would hand the token to a CDN.
            url: asset.browser_download_url,
            size: asset.size,
            content_type: asset.content_type,
            digest: asset
                .digest
                .as_deref()
                .and_then(parse_digest)
                .map(Checksum::sha256),
            headers: BTreeMap::new(),
        }
    }
}

impl From<GhRelease> for Release {
    fn from(release: GhRelease) -> Self {
        // The tag is the authority; `name` is often decorative ("July build").
        let version = Version::parse(&release.tag_name);
        Release {
            version,
            tag: release.tag_name,
            prerelease: release.prerelease,
            draft: release.draft,
            published_at: release.published_at,
            notes: release.body.or(release.name),
            assets: release.assets.into_iter().map(ReleaseAsset::from).collect(),
        }
    }
}

impl From<GhRepo> for SourceInfo {
    fn from(repo: GhRepo) -> Self {
        SourceInfo {
            name: repo
                .full_name
                .rsplit('/')
                .next()
                .unwrap_or(&repo.full_name)
                .to_string(),
            id: repo.full_name,
            description: repo.description,
            homepage: repo.homepage.filter(|h| !h.trim().is_empty()),
            stars: repo.stargazers_count,
            license: repo.license.and_then(|l| l.spdx_id),
            archived: repo.archived,
        }
    }
}

fn parse_digest(raw: &str) -> Option<String> {
    let hex = raw.strip_prefix("sha256:")?.trim();
    is_sha256(hex).then(|| hex.to_ascii_lowercase())
}

fn is_sha256(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

// ---------------------------------------------------------------------------
// Checksums
// ---------------------------------------------------------------------------

/// Container extensions an aggregate checksum list never has. It is a text
/// file; anything packaged or signed is a different artefact that happens to
/// mention checksums in its name.
const NOT_A_CHECKSUM_LIST: &[&str] = &[
    ".tar", ".gz", ".tgz", ".xz", ".txz", ".bz2", ".zip", ".dmg", ".pkg", ".exe", ".jar", ".7z",
];

/// Names that hold checksums for several assets at once.
///
/// Matching on the name alone is unavoidable — nothing else distinguishes the
/// file before it is downloaded — so the negative half matters as much as the
/// positive one. `checksum-verifier-darwin-arm64.tar.gz` and `checksums.txt.sig`
/// both contain "checksum" and neither is a list of them; fetching either as
/// text costs a whole asset transfer and yields nothing.
fn is_aggregate_checksum_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let claims_checksums = lower.contains("sha256sum")
        || lower.contains("sha256_sums")
        || lower.contains("checksum")
        || lower == "sums.txt";
    claims_checksums
        && !crate::platform::is_sidecar(&lower)
        && !NOT_A_CHECKSUM_LIST.iter().any(|s| lower.ends_with(s))
}

/// Parse the `sha256sum` output format: `<hex><space><space|*><name>`.
///
/// Names may carry a leading `./` or a directory prefix, so only the file name
/// is kept — that is what the asset list is keyed by.
fn parse_checksum_file(body: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(hex), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if !is_sha256(hex) {
            continue;
        }
        let name = name.trim_start_matches('*').trim_start_matches("./");
        let name = name.rsplit('/').next().unwrap_or(name);
        out.insert(name.to_string(), hex.to_ascii_lowercase());
    }
    out
}

// ---------------------------------------------------------------------------

impl Source for GitHubSource {
    fn scheme(&self) -> &str {
        "github"
    }

    fn describe(&self, id: &str) -> Result<Option<SourceInfo>> {
        let repo: Option<GhRepo> = self.http.get_json_opt(&self.repo_url(id, "")?, true)?;
        Ok(repo.map(SourceInfo::from))
    }

    fn list_releases(&self, id: &str, opts: &ListOpts) -> Result<Vec<Release>> {
        let per_page = opts.limit.clamp(1, 100);
        let url = self.repo_url(id, &format!("/releases?per_page={per_page}"))?;
        let raw: Vec<GhRelease> = self.http.get_json(&url, true)?;

        let mut releases: Vec<Release> = raw
            .into_iter()
            .filter(|r| !r.draft)
            .map(Release::from)
            .collect();

        // Prereleases are dropped only when there is something stable to drop
        // them in favour of; plenty of projects have never cut a stable tag,
        // and `pick` handles that fallback if the list still holds them.
        if !opts.include_prerelease && releases.iter().any(|r| !r.prerelease) {
            releases.retain(|r| !r.prerelease);
        }
        Ok(releases)
    }

    fn resolve(&self, id: &str, want: &VersionSpec, opts: &ListOpts) -> Result<Release> {
        // Both fast paths are a single request against an endpoint that does
        // the selection server-side; the listing walk is the fallback.
        let direct = match want {
            VersionSpec::Latest if !opts.include_prerelease => Some("/releases/latest".to_string()),
            VersionSpec::Exact(tag) => {
                Some(format!("/releases/tags/{}", urlencode_path_segment(tag)))
            }
            VersionSpec::Latest => None,
        };
        if let Some(suffix) = direct {
            let found: Option<GhRelease> =
                self.http.get_json_opt(&self.repo_url(id, &suffix)?, true)?;
            if let Some(release) = found.filter(|r| !r.draft) {
                return Ok(Release::from(release));
            }
        }
        let opts = &super::opts_for(want, opts);
        let releases = self.list_releases(id, opts)?;
        super::pick(id, releases, want, opts)
    }

    fn checksums(
        &self,
        _id: &str,
        release: &Release,
        wanted: &str,
    ) -> Result<BTreeMap<String, String>> {
        let mut out = BTreeMap::new();

        // Whatever the API already told us costs nothing.
        for asset in &release.assets {
            if let Some(digest) = &asset.digest {
                out.insert(asset.name.clone(), digest.hex.clone());
            }
        }

        // The sidecar for the asset actually being installed goes first. With a
        // cap on how many are worth fetching, the one file that decides this
        // install must never be the one left out — and a release can easily
        // publish thirty sidecars with ours near the end.
        let wanted_sidecar = format!("{wanted}.sha256");
        let mut candidates: Vec<&ReleaseAsset> = release.assets.iter().collect();
        candidates.sort_by_key(|a| a.name != wanted_sidecar);

        let mut fetches = 0;
        for asset in candidates {
            let sidecar = asset.name.ends_with(".sha256");
            if !sidecar && !is_aggregate_checksum_file(&asset.name) {
                continue;
            }
            // Aggregates count too: the heuristic that spots them is a name
            // match, and an unbounded number of name matches is an unbounded
            // number of downloads.
            if fetches >= MAX_CHECKSUM_FETCHES {
                crate::ui::debug("stopping after the checksum-file fetch limit");
                break;
            }
            fetches += 1;
            // A checksum file that will not download is not a reason to fail
            // the install; it just means we fall back to whatever else we have.
            let Ok(body) = self.http.get_text(&asset.url, false) else {
                crate::ui::debug(&format!("could not read checksums from {}", asset.name));
                continue;
            };
            if sidecar {
                let target = asset.name.trim_end_matches(".sha256").to_string();
                if let Some(hex) = parse_checksum_file(&body).into_values().next().or_else(|| {
                    let first = body.split_whitespace().next().unwrap_or("");
                    is_sha256(first).then(|| first.to_ascii_lowercase())
                }) {
                    out.entry(target).or_insert(hex);
                }
            } else {
                for (name, hex) in parse_checksum_file(&body) {
                    out.entry(name).or_insert(hex);
                }
            }
        }
        Ok(out)
    }

    fn download(
        &self,
        asset: &ReleaseAsset,
        dest: &Path,
        progress: &dyn ProgressSink,
    ) -> Result<String> {
        // Anonymous on purpose: see the note on `ReleaseAsset::from`.
        self.http
            .download(&asset.url, dest, &asset.headers, false, progress)
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<SourceInfo>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{}/search/repositories?q={}&per_page={}",
            self.api,
            urlencode(query),
            limit.clamp(1, 100)
        );
        let found: GhSearch = self.http.get_json(&url, true)?;
        Ok(found.items.into_iter().map(SourceInfo::from).collect())
    }

    fn web_url(&self, id: &str) -> Option<String> {
        let repo = validate_repo("GitHub repository", id.to_string()).ok()?;
        let host = self
            .api
            .strip_prefix("https://api.")
            .map(|rest| format!("https://{rest}"))
            .unwrap_or_else(|| self.api.trim_end_matches("/api/v3").to_string());
        Some(format!("{host}/{repo}"))
    }
}

/// Percent-encode a search query. Only the handful of characters that actually
/// appear in package searches need escaping, so this stays a few lines instead
/// of a dependency.
fn urlencode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Percent-encode a value placed in one URL path segment. Unlike a query,
/// spaces are `%20` and slashes must not become path separators: release tags
/// are allowed to contain both.
fn urlencode_path_segment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sha256sum_files_in_their_usual_shapes() {
        let body = "\
# generated
9f2b1e0000000000000000000000000000000000000000000000000000000abc  rg-14.tar.gz
5c3d000000000000000000000000000000000000000000000000000000000def *./dist/rg-14.zip
not-a-hash                                                          junk.txt
";
        let map = parse_checksum_file(body);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map["rg-14.tar.gz"],
            "9f2b1e0000000000000000000000000000000000000000000000000000000abc"
        );
        // Directory prefixes and the binary-mode star are both stripped.
        assert!(map.contains_key("rg-14.zip"));
    }

    #[test]
    fn reads_the_api_digest_field() {
        assert_eq!(
            parse_digest(&format!("sha256:{}", "a".repeat(64))),
            Some("a".repeat(64))
        );
        assert_eq!(parse_digest("md5:abc"), None);
        assert_eq!(parse_digest("sha256:tooshort"), None);
    }

    #[test]
    fn recognises_aggregate_checksum_assets() {
        assert!(is_aggregate_checksum_file("SHA256SUMS"));
        assert!(is_aggregate_checksum_file("tool_1.0_checksums.txt"));
        assert!(!is_aggregate_checksum_file("rg-14.tar.gz"));
    }

    #[test]
    fn derives_a_browsable_url_from_the_api_base() {
        let source = GitHubSource {
            http: Arc::new(Http::anonymous()),
            api: DEFAULT_API.to_string(),
        };
        assert_eq!(
            source.web_url("BurntSushi/ripgrep").as_deref(),
            Some("https://github.com/BurntSushi/ripgrep")
        );
    }

    #[test]
    fn validates_repository_ids_before_building_api_urls() {
        let source = GitHubSource {
            http: Arc::new(Http::anonymous()),
            api: DEFAULT_API.to_string(),
        };
        assert!(source.repo_url("https://attacker.invalid/x", "").is_err());
    }

    #[test]
    fn encodes_release_tags_as_one_path_segment() {
        assert_eq!(
            urlencode_path_segment("release/v1 beta?"),
            "release%2Fv1%20beta%3F"
        );
    }
}
