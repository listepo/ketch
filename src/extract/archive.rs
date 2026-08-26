//! Portable archive formats.
//!
//! Detection is by magic bytes, never by extension. Every entry path goes
//! through `super::safe_member_path` before it is joined onto the destination.

use super::Extractor;
use crate::error::Result;
use std::path::Path;

/// `.tar.gz` / `.tgz`
pub struct TarGzExtractor;

/// `.tar.xz` / `.txz`
pub struct TarXzExtractor;

/// `.tar.bz2`
pub struct TarBz2Extractor;

/// Uncompressed `.tar`
pub struct TarExtractor;

/// `.zip`
pub struct ZipExtractor;

/// `.gz` wrapping a single file rather than a tar stream.
pub struct GzFileExtractor;

/// A bare executable published with no container at all.
///
/// Must be last in the extractor list: it accepts anything the others refused.
pub struct RawBinaryExtractor;

impl Extractor for TarGzExtractor {
    fn id(&self) -> &str {
        "tar.gz"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("gzip magic 1f 8b, and the inflated stream looks like tar")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("flate2 + tar")
    }
}

impl Extractor for TarXzExtractor {
    fn id(&self) -> &str {
        "tar.xz"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("xz magic fd 37 7a 58 5a 00")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("lzma-rs + tar")
    }
}

impl Extractor for TarBz2Extractor {
    fn id(&self) -> &str {
        "tar.bz2"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("bzip2 magic 42 5a 68")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("shell out to /usr/bin/tar — no pure-Rust bzip2 dependency for a rare format")
    }
}

impl Extractor for TarExtractor {
    fn id(&self) -> &str {
        "tar"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("`ustar` at offset 257")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("tar")
    }
}

impl Extractor for ZipExtractor {
    fn id(&self) -> &str {
        "zip"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("PK\\x03\\x04")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("zip crate; preserve unix permission bits so binaries stay executable")
    }
}

impl Extractor for GzFileExtractor {
    fn id(&self) -> &str {
        "gz"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("gzip magic but not a tar stream")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("inflate to dest/<name without .gz>")
    }
}

impl Extractor for RawBinaryExtractor {
    fn id(&self) -> &str {
        "raw"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        true
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("copy into dest and mark executable")
    }
}
