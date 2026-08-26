//! macOS container formats.
//!
//! Both shell out to the system tools, because reimplementing HFS+/APFS image
//! reading or Apple's flat-package format would be a large amount of code with
//! no upside — `hdiutil` and `pkgutil` ship on every Mac.

use super::Extractor;
use crate::error::Result;
use std::path::Path;

/// `.dmg` — attached read-only with no browse and no auto-open, contents
/// copied out, then detached even if the copy failed.
pub struct DmgExtractor;

/// `.pkg` / `.mpkg` — expanded and unpacked into the store rather than
/// installed system-wide, so ketch stays the only thing that owns the files and
/// `ketch uninstall` can actually undo it.
pub struct PkgExtractor;

impl Extractor for DmgExtractor {
    fn id(&self) -> &str {
        "dmg"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("koly trailer, or the .dmg extension")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("hdiutil attach -nobrowse -noautoopen -mountrandom, copy, hdiutil detach in all paths")
    }
}

impl Extractor for PkgExtractor {
    fn id(&self) -> &str {
        "pkg"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        todo!("xar magic `xar!`")
    }
    fn extract(&self, _src: &Path, _dest: &Path) -> Result<()> {
        todo!("pkgutil --expand-full, then lift Payload contents out of Root/")
    }
}
