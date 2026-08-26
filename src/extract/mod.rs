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

/// Drop `count` leading path components, as `tar --strip-components` does.
pub fn strip_components(path: &Path, count: usize) -> Option<PathBuf> {
    if count == 0 {
        return Some(path.to_path_buf());
    }
    let mut parts = path.components();
    for _ in 0..count {
        parts.next()?;
    }
    let rest: PathBuf = parts.collect();
    if rest.as_os_str().is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// If the payload is a single wrapper directory, return it.
///
/// Almost every release tarball unpacks to `tool-1.2.3-target/`; treating that
/// wrapper as the payload root is what makes `bin` paths in manifests short and
/// stable across versions.
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
        return Ok(entries[0].path());
    }
    Ok(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
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
    fn strips_leading_components() {
        let p = Path::new("rg-14.1.0/bin/rg");
        assert_eq!(strip_components(p, 1).unwrap(), PathBuf::from("bin/rg"));
        assert_eq!(strip_components(p, 0).unwrap(), p);
        assert!(strip_components(Path::new("only"), 1).is_none());
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
