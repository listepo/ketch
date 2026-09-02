//! Turning a downloaded file into a directory of files.
//!
//! Extractors are selected by sniffing content, not by trusting the file name:
//! release assets are routinely named `.tar.gz` while being a plain binary, or
//! `.zip` while being a tarball.

pub mod archive;
pub mod macos;

use crate::error::{Error, Result};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

/// One archive format.
pub trait Extractor: Send + Sync {
    /// Stable identifier used in logs and `--verbose` output.
    fn id(&self) -> &str;

    /// Can this extractor handle the file? `head` is the first 512 bytes,
    /// already read, so implementations do not each re-open the file.
    fn detect(&self, path: &Path, head: &[u8]) -> bool;

    /// Unpack `src` into the existing directory `dest`.
    fn extract(&self, src: &Path, dest: &Path) -> Result<()>;
}

/// Pick an extractor and run it. Returns the id of the one that ran.
pub fn extract_auto(src: &Path, dest: &Path, extractors: &[Box<dyn Extractor>]) -> Result<String> {
    let head = read_head(src)?;
    std::fs::create_dir_all(dest).map_err(|e| Error::io(dest, e))?;
    for extractor in extractors {
        if extractor.detect(src, &head) {
            crate::ui::debug(&format!(
                "extracting {} with `{}`",
                src.display(),
                extractor.id()
            ));
            extractor.extract(src, dest)?;
            return Ok(extractor.id().to_string());
        }
    }
    Err(Error::UnsupportedArchive(src.to_path_buf()))
}

/// First 512 bytes of a file, or fewer if it is shorter.
pub fn read_head(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut buffer = vec![0u8; 512];
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::io(path, e)),
        }
    }
    buffer.truncate(filled);
    Ok(buffer)
}

/// Reject archive member paths that would write outside the destination.
///
/// This is the guard against "zip slip" / tar traversal: a member named
/// `../../../.zshrc` must never be honoured, no matter who published it.
/// Absolute paths and `..` components are refused outright; the result is
/// always a relative path safe to join onto the destination.
pub fn safe_member_path(raw: &Path) -> Result<PathBuf> {
    let mut safe = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                // Windows drive-relative and NTFS stream syntax are rejected
                // too, so archives built on Windows cannot smuggle a path.
                if text.contains(':') || text.contains('\\') {
                    return Err(Error::msg(format!(
                        "refusing archive entry with unsafe name: {}",
                        raw.display()
                    )));
                }
                safe.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Error::msg(format!(
                    "refusing archive entry that escapes the target directory: {}",
                    raw.display()
                )));
            }
        }
    }
    if safe.as_os_str().is_empty() {
        return Err(Error::msg("refusing archive entry with an empty name"));
    }
    Ok(safe)
}

/// If the payload is a single wrapper directory, return it.
///
/// Almost every release tarball unpacks to `tool-1.2.3-target/`; treating that
/// wrapper as the payload root is what makes `bin` paths in manifests short and
/// stable across versions.
/// True when a directory name is itself a macOS bundle rather than a container
/// of files.
///
/// A bundle is a directory to everything below the filesystem and a single
/// opaque item to everything above it, which is exactly the distinction
/// `unwrap_single_dir` and app discovery both need.
pub fn is_bundle_name(name: &str) -> bool {
    name.ends_with(".app") || name.ends_with(".framework")
}

pub fn unwrap_single_dir(dir: &Path) -> Result<PathBuf> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))? {
        let entry = entry.map_err(|e| Error::io(dir, e))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Metadata directories macOS and archivers leave behind are not payload.
        if name == "__MACOSX" || name == ".DS_Store" {
            continue;
        }
        entries.push(entry);
    }
    if entries.len() == 1 && entries[0].path().is_dir() {
        // A lone `.app` is the commonest shape a macOS zip or dmg has, and it
        // is the payload — not a wrapper around it. Unwrapping here would hand
        // back the bundle's `Contents`, and nothing downstream would ever see
        // an app to install.
        let name = entries[0].file_name();
        if !is_bundle_name(&name.to_string_lossy()) {
            return Ok(entries[0].path());
        }
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_lone_bundle_is_the_payload_and_is_not_unwrapped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("TestApp.app/Contents/MacOS")).unwrap();

        assert_eq!(unwrap_single_dir(tmp.path()).unwrap(), tmp.path());
    }

    #[test]
    fn a_lone_plain_directory_is_still_unwrapped() {
        let tmp = tempfile::tempdir().unwrap();
        let inner = tmp.path().join("tool-1.0.0");
        std::fs::create_dir_all(inner.join("bin")).unwrap();

        assert_eq!(unwrap_single_dir(tmp.path()).unwrap(), inner);
    }

    use super::*;

    #[test]
    fn rejects_traversal_entries() {
        assert!(safe_member_path(Path::new("../../etc/passwd")).is_err());
        assert!(safe_member_path(Path::new("/etc/passwd")).is_err());
        assert!(safe_member_path(Path::new("a/../../b")).is_err());
        assert!(safe_member_path(Path::new("")).is_err());
    }

    #[test]
    fn accepts_and_normalises_ordinary_entries() {
        assert_eq!(
            safe_member_path(Path::new("./bin/rg")).unwrap(),
            PathBuf::from("bin/rg")
        );
        assert_eq!(
            safe_member_path(Path::new("rg-14/complete/rg.1")).unwrap(),
            PathBuf::from("rg-14/complete/rg.1")
        );
    }

    #[test]
    fn rejects_windows_style_names() {
        assert!(safe_member_path(Path::new("C:windows")).is_err());
        assert!(safe_member_path(Path::new("a\\b")).is_err());
    }

    #[test]
    fn unwraps_a_single_wrapper_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let inner = tmp.path().join("tool-1.2.3");
        std::fs::create_dir_all(inner.join("bin")).unwrap();
        std::fs::write(tmp.path().join(".DS_Store"), b"x").unwrap();
        assert_eq!(unwrap_single_dir(tmp.path()).unwrap(), inner);
    }

    #[test]
    fn keeps_root_when_payload_is_flat() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a"), b"x").unwrap();
        std::fs::write(tmp.path().join("b"), b"x").unwrap();
        assert_eq!(unwrap_single_dir(tmp.path()).unwrap(), tmp.path());
    }
}
