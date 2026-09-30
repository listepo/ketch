//! The GitHub releases source.
//!
//! Works unauthenticated; a token only raises the rate limit and unlocks
//! private repositories.

use super::{ListOpts, Source};
use crate::config::validate_repo;
use crate::error::{Error, Result};
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

/// Upper bound on listing pages walked when an exact tag missed the tag
/// endpoint. GitHub's list is newest-first and 100 items per page at most;
/// a tag older than this is treated as missing rather than looping forever
/// if the API keeps returning full pages.
const MAX_RELEASE_PAGES: u32 = 100;

pub struct GitHubSource {
    http: Arc<Http>,
    api: String,
}

/// The API base every GitHub request is built on: `KETCH_GITHUB_API` when
/// set, so an Enterprise host — or a test double — can stand in for github.com.
pub fn api_base() -> String {
    std::env::var("KETCH_GITHUB_API")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API.to_string())
        .trim_end_matches('/')
        .to_string()
}

impl GitHubSource {
    pub fn new(http: Arc<Http>) -> Self {
        GitHubSource {
            http,
            api: api_base(),
        }
    }

    fn repo_url(&self, id: &str, suffix: &str) -> Result<String> {
        let repo = validate_repo("GitHub repository", id.to_string())?;
        Ok(format!("{}/repos/{}{}", self.api, repo, suffix))
    }

    fn fetch_releases_page(&self, id: &str, opts: &ListOpts, page: u32) -> Result<Vec<GhRelease>> {
        let per_page = opts.limit.clamp(1, 100);
        let url = self.repo_url(id, &format!("/releases?per_page={per_page}&page={page}"))?;
        self.http.get_json(&url, true)
    }

    /// Walk listed releases until `want` matches or the list is exhausted.
    ///
    /// The tag endpoint is exact; a different spelling only shows up here, and
    /// a busy repository's first page is just the newest releases.
    fn resolve_from_listing(
        &self,
        id: &str,
        want: &VersionSpec,
        opts: &ListOpts,
    ) -> Result<Release> {
        let per_page = opts.limit.clamp(1, 100);
        for page in 1..=MAX_RELEASE_PAGES {
            let raw = self.fetch_releases_page(id, opts, page)?;
            let last = raw.len() < per_page;
            match super::pick(id, published_releases(raw, opts), want, opts) {
                Ok(release) => return Ok(release),
                Err(err) if last => return Err(err),
                Err(_) => {}
            }
        }
        match want {
            VersionSpec::Exact(tag) => Err(Error::NoRelease(format!("{id}@{tag}"))),
            VersionSpec::Latest => Err(Error::NoRelease(id.to_string())),
        }
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
            // The public URL rather than the API one: the API redirects to a
            // different host, and while ureq drops the Authorization header on
            // that hop, this URL is also what a browser and `curl -L` use, so a
            // private repository needs the token on the request itself.
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

/// Drafts never appear; prereleases stay only when asked, or when they are
/// all the repository has published.
fn published_releases(raw: Vec<GhRelease>, opts: &ListOpts) -> Vec<Release> {
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
    releases
}

fn parse_digest(raw: &str) -> Option<String> {
    let hex = raw.strip_prefix("sha256:")?.trim();
    is_sha256(hex).then(|| hex.to_ascii_lowercase())
}

/// `1.2.3` as `v1.2.3`, when the request did not already carry a prefix.
///
/// Only the one spelling is tried: a repository tagging `release-1.2.3` is not
/// guessable from the version, and the listing fallback still covers it.
fn v_prefixed(tag: &str) -> Option<String> {
    let tag = tag.trim();
    (!tag.is_empty() && !tag.starts_with(['v', 'V'])).then(|| format!("v{tag}"))
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

/// The asset a `.sha256` sidecar carries the checksum for.
///
/// The extension is matched without regard to case: releases publish
/// `Tool.zip.SHA256` as readily as the lowercase spelling, and a sidecar that
/// goes unrecognised is a published checksum ketch silently never checks.
fn sidecar_target(name: &str) -> Option<&str> {
    const EXT: &str = ".sha256";
    name.to_ascii_lowercase()
        .ends_with(EXT)
        .then(|| &name[..name.len() - EXT.len()])
}

/// Parse the `sha256sum` output format: `<hex><space><space|*><name>`.
///
/// Names may carry a leading `./` or a directory prefix, so only the file name
/// is kept — that is what the asset list is keyed by.
pub(crate) fn parse_checksum_file(body: &str) -> BTreeMap<String, String> {
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
        let raw = self.fetch_releases_page(id, opts, 1)?;
        Ok(published_releases(raw, opts))
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
        // `@1.2.3` for a repository that tags `v1.2.3` is the common spelling of
        // the same request. Other spellings still have to be found in the list.
        if let VersionSpec::Exact(tag) = want {
            if let Some(prefixed) = v_prefixed(tag) {
                let suffix = format!("/releases/tags/{}", urlencode_path_segment(&prefixed));
                let found: Option<GhRelease> =
                    self.http.get_json_opt(&self.repo_url(id, &suffix)?, true)?;
                if let Some(release) = found.filter(|r| !r.draft) {
                    return Ok(Release::from(release));
                }
            }
        }
        let opts = &super::opts_for(want, opts);
        if matches!(want, VersionSpec::Exact(_)) {
            return self.resolve_from_listing(id, want, opts);
        }
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
        let mut candidates: Vec<&ReleaseAsset> = release.assets.iter().collect();
        candidates.sort_by_key(|a| sidecar_target(&a.name) != Some(wanted));

        let mut fetches = 0;
        let mut fetch_error: Option<Error> = None;
        for asset in candidates {
            let sidecar = sidecar_target(&asset.name);
            if sidecar.is_none() && !is_aggregate_checksum_file(&asset.name) {
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
            // Authenticated like the assets themselves: a private repository
            // publishes its sidecars privately too.
            let body = match self.http.get_text(&asset.url, true) {
                Ok(body) => body,
                // A missing checksum file is ordinary; a dead network or a 403
                // is not, and must not be reported as "no published checksum".
                Err(Error::Http { status: 404, .. }) => continue,
                Err(e) => {
                    fetch_error = Some(Error::msg(format!(
                        "could not fetch checksum file {}: {e}",
                        asset.name
                    )));
                    continue;
                }
            };
            match sidecar {
                Some(target) => {
                    if let Some(hex) =
                        parse_checksum_file(&body).into_values().next().or_else(|| {
                            let first = body.split_whitespace().next().unwrap_or("");
                            is_sha256(first).then(|| first.to_ascii_lowercase())
                        })
                    {
                        out.entry(target.to_string()).or_insert(hex);
                    }
                }
                None => {
                    for (name, hex) in parse_checksum_file(&body) {
                        out.entry(name).or_insert(hex);
                    }
                }
            }
        }
        if !out.contains_key(wanted) {
            if let Some(err) = fetch_error {
                return Err(err);
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
        // The token goes on the request, not on the redirect: ureq is built
        // with `RedirectAuthHeaders::Never`, so the cross-host hop to the CDN
        // that actually serves the bytes carries no Authorization header. What
        // it does carry is the difference between a private repository
        // installing and answering 404.
        self.http
            .download(&asset.url, dest, &asset.headers, true, progress)
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
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

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
    fn names_the_asset_a_sidecar_belongs_to_whatever_case_it_is_written_in() {
        assert_eq!(sidecar_target("rg-14.tar.gz.sha256"), Some("rg-14.tar.gz"));
        assert_eq!(sidecar_target("Tool.zip.SHA256"), Some("Tool.zip"));
        assert_eq!(sidecar_target("Tool.zip.Sha256"), Some("Tool.zip"));
        // Not a sidecar: no extension to strip, and a checksum list is the
        // other branch.
        assert_eq!(sidecar_target("SHA256SUMS"), None);
        assert_eq!(sidecar_target("sha256"), None);
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
    fn a_checksum_fetch_failure_is_not_reported_as_a_missing_file() {
        use crate::model::{Release, ReleaseAsset, Version};
        use std::collections::BTreeMap;

        let source = GitHubSource {
            http: Arc::new(Http::anonymous()),
            api: DEFAULT_API.to_string(),
        };
        let release = Release {
            version: Version::parse("1.0.0"),
            tag: "v1.0.0".into(),
            prerelease: false,
            draft: false,
            published_at: None,
            notes: None,
            assets: vec![
                ReleaseAsset {
                    name: "tool.tar.gz".into(),
                    url: "https://example.invalid/tool.tar.gz".into(),
                    size: 0,
                    content_type: None,
                    digest: None,
                    headers: BTreeMap::new(),
                },
                ReleaseAsset {
                    name: "tool.tar.gz.sha256".into(),
                    url: "http://127.0.0.1:1/unreachable".into(),
                    size: 0,
                    content_type: None,
                    digest: None,
                    headers: BTreeMap::new(),
                },
            ],
        };
        let err = source
            .checksums("owner/repo", &release, "tool.tar.gz")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("could not fetch checksum file"),
            "unexpected error: {err}"
        );
        assert!(
            !err.contains("no published checksum"),
            "fetch failure looked like a missing checksum: {err}"
        );
        assert!(
            !err.contains("does not exist"),
            "fetch failure looked like a missing file: {err}"
        );
    }

    #[test]
    fn encodes_release_tags_as_one_path_segment() {
        assert_eq!(
            urlencode_path_segment("release/v1 beta?"),
            "release%2Fv1%20beta%3F"
        );
    }

    #[test]
    fn an_exact_tag_not_on_the_first_list_page_is_still_resolved() {
        let mock = ReleaseListMock::spawn();
        let source = GitHubSource {
            http: Arc::new(Http::anonymous()),
            api: mock.api.clone(),
        };
        let got = source
            .resolve(
                "acme/busy",
                &VersionSpec::Exact("old-0.1.0".into()),
                &ListOpts::default(),
            )
            .unwrap();
        assert_eq!(got.tag, "old-0.1.0");
    }

    struct ReleaseListMock {
        api: String,
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl ReleaseListMock {
        fn spawn() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock github");
            let api = format!(
                "http://127.0.0.1:{}",
                listener.local_addr().expect("addr").port()
            );
            listener
                .set_nonblocking(true)
                .expect("nonblocking mock github");
            let stop = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&stop);
            let handle = thread::spawn(move || loop {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => serve_release_list(&mut stream),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("mock github accept: {e}"),
                }
            });
            ReleaseListMock {
                api,
                stop,
                handle: Some(handle),
            }
        }
    }

    impl Drop for ReleaseListMock {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    fn serve_release_list(stream: &mut TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        let mut buf = Vec::new();
        let mut tmp = [0u8; 1024];
        loop {
            match stream.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(_) => break,
            }
            if buf.len() > 64 * 1024 {
                break;
            }
        }
        let req = String::from_utf8_lossy(&buf);
        let path = req
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");
        let (path_only, query) = path.split_once('?').unwrap_or((path, ""));
        let (status, body) = if path_only.contains("/releases/tags/") {
            ("404 Not Found", r#"{"message":"Not Found"}"#.to_string())
        } else if path_only.ends_with("/releases") {
            let page = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("page="))
                .unwrap_or("1");
            let per_page: usize = query
                .split('&')
                .find_map(|pair| pair.strip_prefix("per_page="))
                .and_then(|v| v.parse().ok())
                .unwrap_or(30);
            let body = match page {
                "1" => {
                    // A full first page, so a one-shot list would miss page 2.
                    let items: Vec<String> = (0..per_page)
                        .map(|i| {
                            format!(
                                r#"{{"tag_name":"v9.9.{i}","prerelease":false,"draft":false,"assets":[]}}"#
                            )
                        })
                        .collect();
                    format!("[{}]", items.join(","))
                }
                "2" => r#"[{"tag_name":"old-0.1.0","prerelease":false,"draft":false,"assets":[]}]"#
                    .to_string(),
                _ => "[]".to_string(),
            };
            ("200 OK", body)
        } else {
            ("404 Not Found", r#"{"message":"Not Found"}"#.to_string())
        };
        let resp = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(resp.as_bytes());
        let _ = stream.flush();
    }
}
