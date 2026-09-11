//! Packages that already live on the local filesystem.
//!
//! A `local:<path>` reference points at an archive, a bare binary, a symlink to
//! either, or a macOS `.app` bundle. There is no remote release stream: the
//! source synthesises a single release so the rest of the install pipeline can
//! stay source-agnostic. Paths are resolved to absolute form when recorded so
//! `ketch list` and `ketch info` keep working after the working directory moves.

use super::{ListOpts, Source};
use crate::error::{Error, Result};
use crate::extract::{is_bundle_name, read_head};
use crate::http;
use crate::model::{
    LocalKind, PackageRef, Release, ReleaseAsset, SourceInfo, Version, VersionSpec,
};
use crate::ui::ProgressSink;
use std::fs;
use std::path::{Path, PathBuf};

/// Tag / version of the synthetic release every local install records.
pub const LOCAL_TAG: &str = "local";
pub const LOCAL_VERSION: &str = "0.0.0-local";

/// The built-in `local` source.
pub struct LocalSource;

impl LocalSource {
    pub fn new() -> Self {
        LocalSource
    }
}

impl Default for LocalSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Source for LocalSource {
    fn scheme(&self) -> &str {
        "local"
    }

    fn describe(&self, id: &str) -> Result<Option<SourceInfo>> {
        let path = resolve_path(id)?;
        let kind = classify(&path)?;
        Ok(Some(SourceInfo {
            id: path_id(&path),
            name: file_label(&path),
            description: Some(format!("local {} at {}", kind.as_str(), path.display())),
            homepage: file_url(&path),
            stars: None,
            license: None,
            archived: false,
        }))
    }

    fn list_releases(&self, id: &str, _opts: &ListOpts) -> Result<Vec<Release>> {
        Ok(vec![synthetic_release(id)?])
    }

    fn resolve(&self, id: &str, want: &VersionSpec, _opts: &ListOpts) -> Result<Release> {
        // One synthetic release: exact requests for anything else fail the same
        // way a missing GitHub tag would. There is never a prerelease listing
        // to widen, so `_opts` is unused on purpose.
        let release = synthetic_release(id)?;
        match want {
            VersionSpec::Latest => Ok(release),
            VersionSpec::Exact(tag)
                if tag.eq_ignore_ascii_case(LOCAL_TAG)
                    || tag.eq_ignore_ascii_case(LOCAL_VERSION)
                    || release.version.matches_request(tag) =>
            {
                Ok(release)
            }
            VersionSpec::Exact(tag) => Err(Error::NoRelease(format!("{id}@{tag}"))),
        }
    }

    fn download(
        &self,
        asset: &ReleaseAsset,
        dest: &Path,
        progress: &dyn ProgressSink,
    ) -> Result<String> {
        let src = Path::new(&asset.url);
        if !src.exists() {
            return Err(Error::msg(format!(
                "local path does not exist: {}",
                src.display()
            )));
        }
        // Directories are handled in `install::prepare` (`.app` bundles). A
        // bare directory reaching download is a programming error upstream.
        if src.is_dir() {
            return Err(Error::msg(format!(
                "local path is a directory, not an installable file: {}",
                src.display()
            )));
        }
        let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        progress.start(Some(size), &asset.name);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        // `fs::copy` follows symlinks for content, which is what we want: the
        // payload is the target's bytes; the kind recorded on the package is
        // what remembered that the origin was a link.
        fs::copy(src, dest).map_err(|e| Error::io(dest, e))?;
        progress.advance(size);
        progress.finish("copied");
        http::sha256_file(dest)
    }

    fn web_url(&self, id: &str) -> Option<String> {
        resolve_path(id).ok().and_then(|p| file_url(&p))
    }
}

fn synthetic_release(id: &str) -> Result<Release> {
    let path = resolve_path(id)?;
    let kind = classify(&path)?;
    if matches!(kind, LocalKind::App) || path.is_dir() {
        // `.app` still needs a release so resolve/info work; download itself
        // refuses directories and `prepare` copies the bundle instead.
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "app".into());
        return Ok(Release {
            version: Version::parse(LOCAL_VERSION),
            tag: LOCAL_TAG.into(),
            prerelease: false,
            draft: false,
            published_at: None,
            notes: Some(format!("local {} at {}", kind.as_str(), path.display())),
            assets: vec![ReleaseAsset {
                name,
                url: path_id(&path),
                size: 0,
                content_type: None,
                digest: None,
                headers: Default::default(),
            }],
        });
    }
    let meta = fs::symlink_metadata(&path).map_err(|e| Error::io(&path, e))?;
    // For a symlink, report the link's name but size of the target when we can.
    let size = if meta.file_type().is_symlink() {
        fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    } else {
        meta.len()
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "payload".into());
    Ok(Release {
        version: Version::parse(LOCAL_VERSION),
        tag: LOCAL_TAG.into(),
        prerelease: false,
        draft: false,
        published_at: None,
        notes: Some(format!("local {} at {}", kind.as_str(), path.display())),
        assets: vec![ReleaseAsset {
            name,
            url: path_id(&path),
            size,
            content_type: None,
            digest: None,
            headers: Default::default(),
        }],
    })
}

/// Turn a `local:` id into an absolute path, without requiring it to exist yet
/// when only normalising a user argument. Prefer [`resolve_path`] at use sites
/// that need the file to be there.
pub fn absolute_path(id: &str) -> Result<PathBuf> {
    let raw = id.trim();
    if raw.is_empty() {
        return Err(Error::msg("local path is empty"));
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Ok(path);
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|e| Error::io(Path::new("."), e))
}

/// Absolute path that must exist (as a file, symlink, or `.app` directory).
pub fn resolve_path(id: &str) -> Result<PathBuf> {
    let path = absolute_path(id)?;
    let meta = match fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::msg(format!(
                "local path does not exist: {}",
                path.display()
            )));
        }
        Err(e) => return Err(Error::io(&path, e)),
    };
    // `exists` follows the link; a dangling symlink still has metadata, and
    // following it at download time would only produce a worse error.
    if meta.file_type().is_symlink() && !path.exists() {
        return Err(Error::msg(format!(
            "local symlink is dangling: {}",
            path.display()
        )));
    }
    Ok(path)
}

/// Build a `PackageRef` whose id is the absolute form of `path`.
pub fn package_ref(path: &Path) -> PackageRef {
    PackageRef::new("local", path_id(path))
}

/// Classify what a local path is, for display and for the install branch.
pub fn classify(path: &Path) -> Result<LocalKind> {
    let meta = fs::symlink_metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.file_type().is_symlink() {
        return Ok(LocalKind::Symlink);
    }
    if meta.is_dir() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_bundle_name(&name) {
            return Ok(LocalKind::App);
        }
        return Err(Error::msg(format!(
            "refusing to install local directory `{}`; only a file, archive, or `.app` bundle is supported",
            path.display()
        )));
    }
    let head = read_head(path)?;
    if looks_like_archive(path, &head) {
        Ok(LocalKind::Archive)
    } else {
        Ok(LocalKind::Binary)
    }
}

fn looks_like_archive(_path: &Path, head: &[u8]) -> bool {
    // Magic-byte detection mirrors `extract::archive`, without accepting the
    // catch-all raw-binary extractor.
    const GZIP: &[u8] = &[0x1f, 0x8b];
    const XZ: &[u8] = &[0xfd, b'7', b'z', b'X', b'Z', 0x00];
    const BZ2: &[u8] = b"BZh";
    const ZIP: &[u8] = &[b'P', b'K', 0x03, 0x04];
    if head.starts_with(GZIP)
        || head.starts_with(XZ)
        || head.starts_with(BZ2)
        || head.starts_with(ZIP)
    {
        return true;
    }
    if head.len() >= 262 && &head[257..262] == b"ustar" {
        return true;
    }
    false
}

fn path_id(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path_id(path))
}

fn file_url(path: &Path) -> Option<String> {
    Some(format!("file://{}", path.display()))
}

/// Copy a directory tree into `dest`, preserving relative structure. Used for
/// local `.app` bundles so they reach `place` without a fake archive round-trip.
pub fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| Error::msg(format!("walking {}: {e}", src.display())))?;
        let rel = entry.path().strip_prefix(src).map_err(|_| {
            Error::msg(format!(
                "path {} escaped {}",
                entry.path().display(),
                src.display()
            ))
        })?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(rel);
        let ft = entry.file_type();
        if ft.is_dir() {
            fs::create_dir_all(&target).map_err(|e| Error::io(&target, e))?;
        } else if ft.is_symlink() {
            let link = fs::read_link(entry.path()).map_err(|e| Error::io(entry.path(), e))?;
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(&link, &target).map_err(|e| Error::io(&target, e))?;
            }
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            fs::copy(entry.path(), &target).map_err(|e| Error::io(&target, e))?;
        }
    }
    Ok(())
}

/// Stable digest of a directory's contents, for the sha256 field of a local `.app`.
pub fn sha256_tree(root: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut paths = Vec::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(|e| Error::io(root, std::io::Error::other(e.to_string())))?;
        if entry.file_type().is_file() {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    for path in paths {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update([0]);
        let bytes = fs::read(&path).map_err(|e| Error::io(&path, e))?;
        hasher.update(&bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn two_identical_trees_hash_the_same() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for dir in [a.path(), b.path()] {
            std::fs::create_dir(dir.join("Contents")).unwrap();
            std::fs::write(dir.join("Contents/Info.plist"), b"x").unwrap();
        }
        assert_eq!(
            sha256_tree(a.path()).unwrap(),
            sha256_tree(b.path()).unwrap()
        );
    }

    #[test]
    fn a_changed_file_changes_the_tree_hash() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), b"one").unwrap();
        let first = sha256_tree(dir.path()).unwrap();
        std::fs::write(dir.path().join("a"), b"two").unwrap();
        assert_ne!(first, sha256_tree(dir.path()).unwrap());
    }

    #[test]
    fn classifies_a_bare_binary() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("tool");
        fs::write(&bin, b"#!/bin/sh\necho hi\n").unwrap();
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(classify(&bin).unwrap(), LocalKind::Binary);
    }

    #[test]
    fn classifies_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("tool");
        fs::write(&bin, b"#!/bin/sh\necho hi\n").unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&bin, &link).unwrap();
        assert_eq!(classify(&link).unwrap(), LocalKind::Symlink);
    }

    #[test]
    fn refuses_a_plain_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("stuff");
        fs::create_dir(&nested).unwrap();
        assert!(classify(&nested)
            .unwrap_err()
            .to_string()
            .contains("directory"));
    }

    #[test]
    fn classifies_an_app_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("Thing.app");
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        assert_eq!(classify(&app).unwrap(), LocalKind::App);
    }

    #[test]
    fn absolute_path_resolves_relative() {
        // Do not touch the process cwd: unit tests share it and a parallel
        // suite that also reads cwd would race.
        let got = absolute_path("rel/tool").unwrap();
        assert_eq!(got, std::env::current_dir().unwrap().join("rel/tool"));
        let abs = absolute_path("/tmp/ketch-local-abs-tool").unwrap();
        assert_eq!(abs, PathBuf::from("/tmp/ketch-local-abs-tool"));
    }
}
