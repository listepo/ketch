//! The package registry: a GitHub repository laid out one folder per package.
//!
//! Every top-level folder names a package and holds a `ketch.toml` describing
//! it. Anything else in the repository — README, licence, CI config — has no
//! `ketch.toml` and is simply not a package, so the registry needs no index
//! file that could drift out of step with its contents.
//!
//! `ketch update` downloads the repository and replaces the local copy under
//! `<root>/registry`. Nothing fetches it implicitly: a package that resolves
//! today must keep resolving offline tomorrow.

use crate::changelog;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::extract::{archive::TarGzExtractor, unwrap_single_dir, Extractor};
use crate::http::Http;
use crate::model::{normalize_name, Manifest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file inside a package folder. The folder already names the package, so
/// the file does not have to repeat it.
pub const PACKAGE_FILE: &str = "ketch.toml";

/// True once a copy has been fetched.
pub fn exists(cfg: &Config) -> bool {
    cfg.registry_dir.is_dir()
}

/// Every package in the local copy, each paired with the file it came from.
pub fn load(cfg: &Config) -> Vec<(Manifest, PathBuf)> {
    load_dir(&cfg.registry_dir)
}

/// Fetch the registry and swap it in, returning how many packages it holds.
///
/// The download is staged and only moved into place once it parses, so a bad
/// or truncated fetch leaves the working copy alone.
pub fn update(cfg: &Config) -> Result<usize> {
    let repo = &cfg.registry;
    crate::ui::stage("registry", crate::ui::ProgressStage::Downloading);
    crate::ui::step("updating", &format!("registry {repo}"));

    let staging = tempfile::tempdir_in(&cfg.root).map_err(|e| Error::io(&cfg.root, e))?;
    let tarball = staging.path().join("registry.tar.gz");
    let url = tarball_url(repo);
    // The API tarball endpoint follows the default branch and honours the
    // token, which keeps unauthenticated rate limits out of the way. It answers
    // 415 to the octet-stream `Accept` that asset downloads use, so ask for the
    // API media type and let it redirect to the gzip.
    let accept = [(
        "Accept".to_string(),
        "application/vnd.github+json".to_string(),
    )];
    let headers = BTreeMap::from(accept);
    Http::new(cfg).download(
        &url,
        &tarball,
        &headers,
        true,
        crate::ui::progress().as_ref(),
    )?;

    let unpacked = staging.path().join("tree");
    std::fs::create_dir_all(&unpacked).map_err(|e| Error::io(&unpacked, e))?;
    crate::ui::stage("registry", crate::ui::ProgressStage::Extracting);
    TarGzExtractor.extract(&tarball, &unpacked)?;
    // GitHub wraps the tree in one `owner-repo-<sha>` directory.
    let root = unwrap_single_dir(&unpacked)?;

    swap_in(cfg, &root, repo)
}

/// Where the registry's tarball is fetched from: the same API base every
/// other GitHub request honors, so `KETCH_GITHUB_API` stands in an Enterprise
/// host here too.
fn tarball_url(repo: &str) -> String {
    format!("{}/repos/{repo}/tarball", crate::source::github::api_base())
}

/// Move a freshly-unpacked tree into place, returning its package count.
///
/// A tree with no packages is refused: a repository that moved, emptied or
/// answered with something unexpected must not wipe a working registry.
fn swap_in(cfg: &Config, tree: &Path, repo: &str) -> Result<usize> {
    let packages = load_dir(tree);
    for problem in collisions(&packages) {
        crate::ui::warn(&problem);
    }
    let count = packages.len();
    if count == 0 {
        return Err(Error::msg(format!(
            "{repo} has no package folders containing `{PACKAGE_FILE}` \
             — leaving the current registry in place"
        )));
    }
    if cfg.registry_dir.exists() {
        // Move the working copy aside first. Deleting it before the new tree
        // is in place would leave no registry if the second rename failed
        // (EXDEV, EACCES, ENOSPC) or if another ketch ran in the gap.
        let aside = aside_path(&cfg.registry_dir);
        if aside.exists() {
            std::fs::remove_dir_all(&aside).map_err(|e| Error::io(&aside, e))?;
        }
        std::fs::rename(&cfg.registry_dir, &aside).map_err(|e| Error::io(&cfg.registry_dir, e))?;
        if let Err(e) = std::fs::rename(tree, &cfg.registry_dir) {
            let _ = std::fs::rename(&aside, &cfg.registry_dir);
            return Err(Error::io(&cfg.registry_dir, e));
        }
        let _ = std::fs::remove_dir_all(&aside);
    } else {
        std::fs::rename(tree, &cfg.registry_dir).map_err(|e| Error::io(&cfg.registry_dir, e))?;
    }
    write_meta(
        cfg,
        &UpdateMeta {
            repo: repo.to_string(),
            revision: git_revision(tree),
            etag: None,
            fetched_at: crate::model::now_unix(),
        },
    )?;
    Ok(count)
}

/// Sibling of the live registry, unique per process so a leftover aside from a
/// crashed run is not the path this swap uses.
fn aside_path(registry_dir: &Path) -> PathBuf {
    let name = registry_dir
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    registry_dir.with_file_name(format!(".{name}.aside-{}", std::process::id()))
}

/// One package file that failed [`check_tree`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The `ketch.toml` path, or the registry root for tree-wide problems.
    pub path: String,
    /// What went wrong.
    pub message: String,
}

/// Outcome of validating a registry tree offline.
#[derive(Debug, Clone)]
pub struct Report {
    /// Packages that parsed and passed [`Manifest::validate`].
    pub packages: usize,
    /// Every parse, validation and collision problem found.
    pub errors: Vec<ValidationError>,
    /// Entries that parsed, including ones that later collide on a name.
    pub parsed: Vec<(Manifest, PathBuf)>,
}

/// Validate every package folder under `dir` the way registry CI should.
///
/// Unlike [`load_dir`], nothing is skipped: each broken `ketch.toml` and every
/// name collision is collected, and a tree that cannot be read at all is a
/// report with an error in it rather than a failure — the caller can print that
/// as JSON for a machine either way.
pub fn check_tree(dir: &Path) -> Report {
    if !dir.is_dir() {
        return unreadable(dir, "no such directory");
    }

    let folders = candidate_package_dirs(dir);
    if folders.is_empty() {
        return unreadable(
            dir,
            &format!("no package folders containing `{PACKAGE_FILE}`"),
        );
    }

    let mut packages = Vec::new();
    let mut errors = Vec::new();
    for folder in folders {
        let path = folder.join(PACKAGE_FILE);
        if !is_package_file(&path) {
            errors.push(ValidationError {
                path: path.display().to_string(),
                message: format!("`{PACKAGE_FILE}` must be a regular file, not a symlink"),
            });
            continue;
        }
        let name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match read_package(&path, &name) {
            Ok(manifest) => packages.push((manifest, path)),
            Err(e) => errors.push(ValidationError {
                path: path.display().to_string(),
                message: message_without_path(&path, &e),
            }),
        }
    }

    for message in collisions(&packages) {
        errors.push(ValidationError {
            path: dir.display().to_string(),
            message,
        });
    }

    Report {
        packages: packages.len(),
        errors,
        parsed: packages,
    }
}

/// A report holding one problem with the tree itself.
fn unreadable(dir: &Path, message: &str) -> Report {
    Report {
        packages: 0,
        errors: vec![ValidationError {
            path: dir.display().to_string(),
            message: message.to_string(),
        }],
        parsed: Vec::new(),
    }
}

/// What went wrong, without the path [`Report`] prints in front of it.
///
/// [`read_package`] builds its errors around the file they are about — `Io`
/// leads with the path, `parse` embeds it — and a caller that shows the path
/// as well would otherwise say everything twice.
fn message_without_path(path: &Path, error: &Error) -> String {
    if let Error::Parse { detail, .. } = error {
        return detail.clone();
    }
    let message = error.to_string();
    let prefix = format!("{}: ", path.display());
    match message.strip_prefix(&prefix) {
        Some(rest) => rest.to_string(),
        None => message,
    }
}

/// Names that two packages both answer to.
///
/// Nothing else can catch this: each folder is valid on its own, the loser is
/// shadowed silently, and which one loses depends on sort order. Callers
/// decide the severity: [`load_dir`] and [`update`] warn so an already-published
/// registry keeps resolving, and [`check_tree`] turns the same messages into
/// errors so they cannot merge.
pub(crate) fn collisions(packages: &[(Manifest, PathBuf)]) -> Vec<String> {
    /// What a folder is called, for a message that has to tell two packages
    /// with the same name apart.
    fn folder(path: &Path) -> String {
        path.parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string())
    }

    // The claim is keyed by the path that made it, not by the package name:
    // two folders can both declare `name = "foo"` (a `.git` suffix and its
    // plain sibling, or a hand edit), and comparing names would read that as
    // one package claiming its own name.
    let mut claimed: BTreeMap<String, (String, PathBuf)> = BTreeMap::new();
    let mut out = Vec::new();
    for (manifest, path) in packages {
        for name in std::iter::once(&manifest.name).chain(manifest.provides.iter()) {
            let owner = claimed
                .entry(normalize_name(name))
                .or_insert_with(|| (manifest.name.clone(), path.clone()));
            if owner.1 == *path {
                continue;
            }
            out.push(if owner.0 == manifest.name {
                format!(
                    "`{name}` is claimed by two packages named `{}`, in `{}` and `{}`; only the first will resolve",
                    owner.0,
                    folder(&owner.1),
                    folder(path)
                )
            } else {
                format!(
                    "`{name}` is claimed by both `{}` and `{}`; only `{}` will resolve",
                    owner.0, manifest.name, owner.0
                )
            });
        }
    }
    out
}

/// What `ketch update` recorded about the local registry copy.
///
/// Lives under the ketch root as `registry.meta.toml`, not inside the swapped
/// package tree, so a fetch record cannot be mistaken for a package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateMeta {
    /// `owner/repo` the tarball was fetched from.
    pub repo: String,
    /// Commit SHA from the GitHub tarball wrapper directory, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// HTTP ETag from the fetch, when the transport exposed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// Unix seconds when the local copy was swapped in.
    pub fetched_at: u64,
}

/// How old a fetch is, for `registry status` and doctor. No network.
pub fn age_phrase(fetched_at: u64) -> String {
    let now = crate::model::now_unix();
    let secs = now.saturating_sub(fetched_at);
    if now < fetched_at || secs < 60 {
        return "just now".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{mins}m ago");
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    format!("{}d ago", hours / 24)
}

/// The last successful `ketch update`, if one was recorded.
pub fn load_meta(cfg: &Config) -> Result<Option<UpdateMeta>> {
    match std::fs::read_to_string(&cfg.registry_meta) {
        Ok(text) => {
            let meta: UpdateMeta = toml::from_str(&text).map_err(|e| {
                Error::parse(cfg.registry_meta.display().to_string(), e.to_string())
            })?;
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(&cfg.registry_meta, e)),
    }
}

fn write_meta(cfg: &Config, meta: &UpdateMeta) -> Result<()> {
    let body = toml::to_string_pretty(meta)
        .map_err(|e| Error::parse(cfg.registry_meta.display().to_string(), e.to_string()))?;
    let text = format!("# Written by `ketch update`. Do not edit.\n{body}");
    std::fs::write(&cfg.registry_meta, text).map_err(|e| Error::io(&cfg.registry_meta, e))
}

/// Commit SHA GitHub puts on the tarball wrapper, `owner-repo-<40 hex>`.
fn git_revision(tree: &Path) -> Option<String> {
    let name = tree.file_name()?.to_string_lossy();
    let sha = name.rsplit_once('-')?.1;
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| sha.to_ascii_lowercase())
}

/// The local asset that stands in for `name` during an offline install.
///
/// A file named `name`, or a folder `name/` holding exactly one regular file.
pub fn fixture_payload(fixture: &Path, name: &str) -> Result<PathBuf> {
    let entry = fixture.join(name);
    let meta = match std::fs::symlink_metadata(&entry) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::msg(format!("no fixture for `{name}`")));
        }
        Err(e) => return Err(Error::io(&entry, e)),
    };
    if meta.file_type().is_file() {
        return Ok(entry);
    }
    if !meta.file_type().is_dir() {
        return Err(Error::msg(format!(
            "fixture `{name}` must be a file or a directory of one file"
        )));
    }
    let mut files = Vec::new();
    for child in std::fs::read_dir(&entry).map_err(|e| Error::io(&entry, e))? {
        let child = child.map_err(|e| Error::io(&entry, e))?;
        let path = child.path();
        let child_meta = std::fs::symlink_metadata(&path).map_err(|e| Error::io(&path, e))?;
        if child_meta.file_type().is_file() {
            files.push(path);
        }
    }
    files.sort();
    match files.as_slice() {
        [one] => Ok(one.clone()),
        [] => Err(Error::msg(format!(
            "fixture `{name}` is an empty directory"
        ))),
        _ => Err(Error::msg(format!(
            "fixture `{name}` has more than one file"
        ))),
    }
}

fn load_dir(dir: &Path) -> Vec<(Manifest, PathBuf)> {
    let mut out = Vec::new();
    for folder in candidate_package_dirs(dir) {
        let path = folder.join(PACKAGE_FILE);
        let name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if !is_package_file(&path) {
            crate::ui::warn(&format!(
                "ignoring registry package `{}`: `{PACKAGE_FILE}` is not a regular file",
                changelog::sanitize(&name)
            ));
            continue;
        }
        match read_package(&path, &name) {
            Ok(manifest) => out.push((manifest, path)),
            // One broken entry must not hide the rest of the registry.
            Err(e) => crate::ui::warn(&format!(
                "ignoring registry package `{}`: {e}",
                changelog::sanitize(&name)
            )),
        }
    }
    out
}

/// Top-level folders that contain a `ketch.toml`, including a symlink one.
///
/// [`check_tree`] must see those links so it can fail closed; [`load_dir`]
/// warns and skips them.
fn candidate_package_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && std::fs::symlink_metadata(p.join(PACKAGE_FILE)).is_ok())
        .collect();
    folders.sort();
    folders
}

/// Whether `path` is a real `ketch.toml` rather than a link to one.
///
/// The tree is somebody else's: a folder whose package file is a symlink would
/// have ketch read — and quote in a parse error, in a warning, in a CI log — a
/// file from outside the tree, and a dangling link would leave that folder
/// never validated while the run still passes. The fetched registry is guarded
/// on the way in (`extract::check_link_target`), so this covers the trees that
/// arrive as files: a checkout, a working copy, registry CI.
fn is_package_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

/// Rules a registry entry must pass beyond [`Manifest::validate`].
///
/// Shared by [`read_package`] and [`crate::push::load`] so push refuses the
/// same manifest CI would reject.
pub(crate) fn validate_registry_entry(
    what: &str,
    folder: Option<&str>,
    declared_name: Option<&str>,
    manifest: &Manifest,
) -> Result<()> {
    if let (Some(folder), Some(declared)) = (folder, declared_name) {
        if normalize_name(declared) != normalize_name(folder) {
            return Err(Error::parse(
                what,
                format!("declares name `{declared}` but sits in folder `{folder}`"),
            ));
        }
    }
    // A registry entry names a release anyone can fetch. `local:` would make a
    // shared entry install from — or, for a path that is not a plain file,
    // hang on — the disk of whoever installs it, which is not a promise the
    // registry can make. User manifests and `ketch install local:…` are
    // unaffected: they never pass through here.
    if manifest.source.scheme == "local" {
        return Err(Error::parse(
            what,
            "a registry package cannot install from a local path; use `github:owner/repo`"
                .to_string(),
        ));
    }
    Ok(())
}

/// Parse one package folder.
///
/// The folder is the package name, so `name` in the file is optional — and
/// when it is present it must agree, or the package would be unreachable under
/// the name its folder advertises.
pub(crate) fn read_package(path: &Path, folder: &str) -> Result<Manifest> {
    let what = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let mut value: toml::Value =
        toml::from_str(&text).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    let table = value.as_table_mut().ok_or_else(|| {
        Error::parse(
            what.as_str(),
            "expected a table of package fields".to_string(),
        )
    })?;

    let declared_name = table
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if declared_name.is_none() {
        table.insert("name".into(), toml::Value::String(folder.to_string()));
    }
    let manifest =
        Manifest::deserialize(value).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    manifest
        .validate()
        .map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    validate_registry_entry(
        what.as_str(),
        Some(folder),
        declared_name.as_deref(),
        &manifest,
    )?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, folder: &str, body: &str) {
        let package = dir.join(folder);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join(PACKAGE_FILE), body).unwrap();
    }

    #[test]
    fn the_folder_names_the_package() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "ripgrep",
            "source = \"github:BurntSushi/ripgrep\"\n",
        );
        let found = load_dir(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "ripgrep");
        assert_eq!(found[0].0.source.id, "BurntSushi/ripgrep");
    }

    #[test]
    fn a_declared_name_must_match_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "fzf",
            "name = \"fzy\"\nsource = \"github:junegunn/fzf\"\n",
        );
        assert!(load_dir(tmp.path()).is_empty());
    }

    #[test]
    fn non_package_folders_and_broken_entries_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".github/workflows")).unwrap();
        std::fs::write(tmp.path().join("README.md"), "hi").unwrap();
        write(tmp.path(), "broken", "source = 12\n");
        write(tmp.path(), "jq", "source = \"github:jqlang/jq\"\n");
        let found = load_dir(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "jq");
    }

    #[test]
    fn a_name_two_packages_claim_is_reported_once_against_its_first_owner() {
        let tmp = tempfile::tempdir().unwrap();
        // `fd` provides its own name, which is not a collision with itself.
        write(
            tmp.path(),
            "fd",
            "source = \"github:sharkdp/fd\"\nprovides = [\"fd\"]\n",
        );
        write(tmp.path(), "rg", "source = \"github:BurntSushi/ripgrep\"\n");
        assert!(collisions(&load_dir(tmp.path())).is_empty());

        write(
            tmp.path(),
            "zfd",
            "source = \"github:someone/zfd\"\nprovides = [\"fd\"]\n",
        );
        let found = collisions(&load_dir(tmp.path()));
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("both `fd` and `zfd`"), "{}", found[0]);
    }

    #[test]
    fn an_empty_tree_never_replaces_a_working_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(Some(tmp.path().to_path_buf())).unwrap();
        std::fs::create_dir_all(&cfg.registry_dir).unwrap();
        write(&cfg.registry_dir, "jq", "source = \"github:jqlang/jq\"\n");

        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(swap_in(&cfg, &empty, "someone/registry").is_err());
        assert_eq!(load(&cfg).len(), 1, "the old registry must still be there");

        let fresh = tmp.path().join("fresh");
        write(&fresh, "fd", "source = \"github:sharkdp/fd\"\n");
        write(&fresh, "rg", "source = \"github:BurntSushi/ripgrep\"\n");
        assert_eq!(swap_in(&cfg, &fresh, "someone/registry").unwrap(), 2);
        let names: Vec<String> = load(&cfg).into_iter().map(|(m, _)| m.name).collect();
        assert_eq!(names, ["fd", "rg"]);
    }

    #[test]
    fn a_package_that_would_install_outside_the_store_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "evil",
            "name = \"evil\"\nsource = \"github:a/b\"\n\
             bin = [{ name = \"../../../.zshrc\" }]\n",
        );
        write(
            tmp.path(),
            "typo",
            "source = \"github:a/b\"\nbinary = \"x\"\n",
        );
        write(tmp.path(), "ok", "source = \"github:a/b\"\n");
        let found = load_dir(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "ok");
    }

    #[test]
    fn check_tree_accepts_a_valid_package_folder() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "tool", "source = \"github:a/b\"\n");
        let report = check_tree(tmp.path());
        assert!(report.errors.is_empty());
        assert_eq!(report.packages, 1);
    }

    #[test]
    fn check_tree_collects_parse_and_validation_errors() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "evil",
            "name = \"evil\"\nsource = \"github:a/b\"\n\
             bin = [{ name = \"../../../.zshrc\" }]\n",
        );
        write(tmp.path(), "broken", "source = 12\n");
        let report = check_tree(tmp.path());
        assert_eq!(report.packages, 0);
        assert_eq!(report.errors.len(), 2);
    }

    #[test]
    fn check_tree_names_a_broken_file_once() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "broken", "source = 12\n");
        let report = check_tree(tmp.path());
        assert_eq!(report.errors.len(), 1);
        let error = &report.errors[0];
        assert!(
            std::path::Path::new(&error.path).ends_with("broken/ketch.toml"),
            "{}",
            error.path
        );
        assert!(
            !error.message.contains(&error.path),
            "the report prints the path in front of the message, so the message must not repeat it: {}",
            error.message
        );
    }

    #[test]
    fn check_tree_treats_name_collisions_as_errors() {
        // Name-only collision detection reads two `foo` packages as one package
        // claiming its own name and stays silent. `check_tree` must turn the
        // path-keyed collision into a registry-root error instead.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "foo", "source = \"github:a/foo\"\n");
        write(
            tmp.path(),
            "foo.git",
            "name = \"foo\"\nsource = \"github:b/foo\"\n",
        );
        let report = check_tree(tmp.path());
        assert_eq!(report.packages, 2);
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert_eq!(
            report.errors[0].path,
            tmp.path().display().to_string(),
            "collisions are reported against the registry root"
        );
        assert!(
            report.errors[0].message.contains("`foo.git`"),
            "name-only collision detection would stay silent: {}",
            report.errors[0].message
        );
    }

    #[test]
    fn check_tree_refuses_an_empty_tree() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("README.md"), "hi").unwrap();
        let report = check_tree(tmp.path());
        assert_eq!(report.packages, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].message.contains(PACKAGE_FILE));
    }

    #[test]
    fn check_tree_errors_when_the_path_is_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        let report = check_tree(&missing);
        assert_eq!(report.packages, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(
            report.errors[0].message.contains("no such directory"),
            "{}",
            report.errors[0].message
        );
    }

    #[test]
    fn check_tree_sees_two_folders_that_land_on_the_same_name() {
        // `read_package` accepts `name = "foo"` in a `foo.git` folder, so two
        // folders can end up answering to `foo`. Comparing package names would
        // read that as one package claiming its own name, and stay silent while
        // one of the two is shadowed for good.
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "foo", "source = \"github:a/foo\"\n");
        write(
            tmp.path(),
            "foo.git",
            "name = \"foo\"\nsource = \"github:b/foo\"\n",
        );
        let report = check_tree(tmp.path());
        assert_eq!(report.packages, 2);
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(
            report.errors[0].message.contains("`foo.git`"),
            "the message must say which folders collide: {}",
            report.errors[0].message
        );
    }

    #[test]
    fn check_tree_refuses_a_registry_package_that_installs_from_a_local_path() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "sneaky", "source = \"local:/etc/passwd\"\n");
        let report = check_tree(tmp.path());
        assert_eq!(report.packages, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(
            report.errors[0].message.contains("local path"),
            "{}",
            report.errors[0].message
        );
    }

    #[cfg(unix)]
    #[test]
    fn check_tree_never_reads_through_a_symlinked_package_file() {
        // The tree is somebody else's: a link here would have ketch read — and
        // quote in an error, in a warning, in a CI log — a file from outside it.
        let tmp = tempfile::tempdir().unwrap();
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "root:x:0:0:root:/root:/bin/sh\n").unwrap();
        write(tmp.path(), "good", "source = \"github:a/b\"\n");
        let sneaky = tmp.path().join("sneaky");
        std::fs::create_dir_all(&sneaky).unwrap();
        std::os::unix::fs::symlink(&secret, sneaky.join(PACKAGE_FILE)).unwrap();

        let report = check_tree(tmp.path());
        assert_eq!(report.packages, 1);
        assert_eq!(report.errors.len(), 1, "{:?}", report.errors);
        assert!(
            report.errors[0].message.contains("not a symlink"),
            "{}",
            report.errors[0].message
        );
        assert!(
            std::path::Path::new(&report.errors[0].path).ends_with("sneaky/ketch.toml"),
            "{}",
            report.errors[0].path
        );
    }

    #[test]
    fn tarball_url_honours_ketch_github_api() {
        // Same env override every other GitHub request uses (follow-up 9).
        let previous = std::env::var("KETCH_GITHUB_API").ok();
        std::env::set_var("KETCH_GITHUB_API", "https://ghe.example/api/v3");
        assert_eq!(
            tarball_url("acme/registry"),
            "https://ghe.example/api/v3/repos/acme/registry/tarball"
        );
        match previous {
            Some(v) => std::env::set_var("KETCH_GITHUB_API", v),
            None => std::env::remove_var("KETCH_GITHUB_API"),
        }
        assert_eq!(
            tarball_url("acme/registry"),
            format!(
                "{}/repos/acme/registry/tarball",
                crate::source::github::DEFAULT_API
            )
        );
    }

    #[test]
    fn git_revision_reads_the_tarball_wrapper_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let tree = tmp.path().join(format!("listepo-ketch-registry-{sha}"));
        std::fs::create_dir(&tree).unwrap();
        assert_eq!(git_revision(&tree).as_deref(), Some(sha));
        assert!(git_revision(tmp.path().join("fresh").as_path()).is_none());
    }

    #[test]
    fn a_successful_swap_records_fetch_metadata_beside_the_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(Some(tmp.path().to_path_buf())).unwrap();
        let sha = "abcdef0123456789abcdef0123456789abcdef01";
        let fresh = tmp.path().join(format!("owner-registry-{sha}"));
        write(&fresh, "jq", "source = \"github:jqlang/jq\"\n");
        assert_eq!(swap_in(&cfg, &fresh, "someone/registry").unwrap(), 1);
        let meta = load_meta(&cfg).unwrap().expect("meta written");
        assert_eq!(meta.repo, "someone/registry");
        assert_eq!(meta.revision.as_deref(), Some(sha));
        assert!(meta.fetched_at > 0);
        assert!(cfg.registry_meta.is_file());
        let body = std::fs::read_to_string(&cfg.registry_meta).unwrap();
        assert!(body.starts_with("# Written by `ketch update`. Do not edit."));
    }

    #[test]
    fn fixture_payload_takes_a_folder_with_one_file() {
        let tmp = tempfile::tempdir().unwrap();
        let fixture = tmp.path();
        let pkg = fixture.join("tool");
        std::fs::create_dir(&pkg).unwrap();
        let asset = pkg.join("tool.tar.gz");
        std::fs::write(&asset, b"bytes").unwrap();
        assert_eq!(fixture_payload(fixture, "tool").unwrap(), asset);
    }

    #[test]
    fn fixture_payload_refuses_an_ambiguous_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("tool");
        std::fs::create_dir(&pkg).unwrap();
        std::fs::write(pkg.join("a.tar.gz"), b"a").unwrap();
        std::fs::write(pkg.join("b.tar.gz"), b"b").unwrap();
        let err = fixture_payload(tmp.path(), "tool").unwrap_err().to_string();
        assert!(err.contains("more than one file"), "{err}");
    }

    #[test]
    fn age_phrase_uses_whole_units() {
        let now = crate::model::now_unix();
        assert_eq!(age_phrase(now), "just now");
        assert_eq!(age_phrase(now.saturating_sub(120)), "2m ago");
        assert_eq!(age_phrase(now.saturating_sub(7200)), "2h ago");
        assert_eq!(age_phrase(now.saturating_sub(48 * 3600)), "2d ago");
    }
}
