//! Portable archive formats.
//!
//! Detection is by magic bytes, never by extension. Every entry path goes
//! through `super::safe_member_path` before it is joined onto the destination.

use super::{safe_member_path, Extractor};
use crate::error::{Error, Result};
use std::fs::File;
use std::io::{BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

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

const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];
const XZ_MAGIC: &[u8] = &[0xfd, b'7', b'z', b'X', b'Z', 0x00];
const BZ2_MAGIC: &[u8] = b"BZh";
const ZIP_MAGIC: &[u8] = &[b'P', b'K', 0x03, 0x04];

/// The `ustar` marker tar writes at offset 257 of every header block.
fn looks_like_tar(head: &[u8]) -> bool {
    head.len() >= 262 && &head[257..262] == b"ustar"
}

/// Inflate just enough of a gzip stream to see whether a tar header follows.
///
/// This is what separates `tool.tar.gz` from `tool.gz` when the publisher named
/// the file wrongly, which happens often enough to matter.
fn gzip_inner_head(path: &Path) -> Vec<u8> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut decoder = flate2::read::GzDecoder::new(BufReader::new(file));
    read_head_of(&mut decoder, 512)
}

fn read_head_of<R: Read>(reader: &mut R, want: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; want];
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    buffer.truncate(filled);
    buffer
}

/// A link inside the payload may only point at something else inside it.
///
/// Checked lexically, before the link exists: a symlink to `../../../.ssh`
/// would otherwise turn the next `place` into an arbitrary-file copy.
fn check_link_target(member: &Path, target: &Path) -> Result<()> {
    let reject = || {
        Err(Error::msg(format!(
            "refusing archive link {} -> {} that escapes the target directory",
            member.display(),
            target.display()
        )))
    };
    if target.is_absolute() {
        return reject();
    }
    // Depth of the directory holding the link, relative to the payload root.
    let mut depth = member.components().count() as i64 - 1;
    for component in target.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(_) => depth += 1,
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return reject();
                }
            }
            Component::RootDir | Component::Prefix(_) => return reject(),
        }
    }
    Ok(())
}

/// Walk every component of `path` below `dest`, refusing any that is a symlink.
///
/// This is what makes `check_link_target` sound. That guard counts depth from
/// the member *name*, but `create_dir_all` and every write follow links, so a
/// member called `a/link/b` lands wherever `link` points once an earlier member
/// of the same archive planted it — and the surplus `..` in a later target then
/// escapes `dest`. Refusing to traverse any link below `dest` keeps the path we
/// counted and the path the kernel resolves the same one, for files,
/// directories, symlinks and hard links alike.
///
/// `dest` itself is ketch's own directory and is trusted: on macOS it usually
/// *is* reached through a symlink (`/tmp` -> `private/tmp`).
fn walk_inside(dest: &Path, path: &Path, create: bool) -> Result<()> {
    let rel = path.strip_prefix(dest).map_err(|_| {
        Error::msg(format!(
            "refusing archive entry {} outside {}",
            path.display(),
            dest.display()
        ))
    })?;
    let mut built = dest.to_path_buf();
    for component in rel.components() {
        built.push(component);
        match std::fs::symlink_metadata(&built) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::msg(format!(
                    "refusing archive entry that resolves through {}",
                    built.display()
                )))
            }
            // An ordinary directory, or a file at the leaf: both are fine.
            Ok(_) => {}
            Err(_) if create => std::fs::create_dir(&built).map_err(|e| Error::io(&built, e))?,
            // Nothing exists from here down, so nothing below can be a link.
            Err(_) => break,
        }
    }
    Ok(())
}

/// Make `out` safe to write: its directories exist, none of them is a link,
/// and `out` itself is not a link an earlier member planted for us to write
/// through (zip's `File::create` would happily truncate the far end).
fn ensure_parent(dest: &Path, out: &Path) -> Result<()> {
    if let Some(parent) = out.parent() {
        walk_inside(dest, parent, true)?;
    }
    if std::fs::symlink_metadata(out).is_ok_and(|m| m.file_type().is_symlink()) {
        std::fs::remove_file(out).map_err(|e| Error::io(out, e))?;
    }
    Ok(())
}

/// Unpack a tar stream, validating every member path and link target.
///
/// Written out rather than using `Archive::unpack` so the traversal guard is
/// ours and applies identically to files, directories and links.
fn unpack_tar<R: Read>(reader: R, dest: &Path) -> Result<()> {
    use tar::EntryType;

    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(true);
    archive.set_overwrite(true);

    for entry in archive.entries().map_err(|e| Error::io(dest, e))? {
        let mut entry = entry.map_err(|e| Error::io(dest, e))?;
        let raw = entry.path().map_err(|e| Error::io(dest, e))?.into_owned();
        let kind = entry.header().entry_type();

        // Extended headers carry metadata, not payload, and have no real path.
        if kind.is_pax_global_extensions()
            || kind.is_pax_local_extensions()
            || kind.is_gnu_longname()
        {
            continue;
        }
        let safe = safe_member_path(&raw)?;
        let out = dest.join(&safe);

        match kind {
            EntryType::Directory => {
                walk_inside(dest, &out, true)?;
            }
            EntryType::Symlink => {
                let target = entry
                    .link_name()
                    .map_err(|e| Error::io(&out, e))?
                    .ok_or_else(|| {
                        Error::msg(format!("archive symlink {} has no target", raw.display()))
                    })?
                    .into_owned();
                check_link_target(&safe, &target)?;
                ensure_parent(dest, &out)?;
                std::os::unix::fs::symlink(&target, &out).map_err(|e| Error::io(&out, e))?;
            }
            EntryType::Link => {
                // A tar hard link names its target from the archive root.
                let target = entry
                    .link_name()
                    .map_err(|e| Error::io(&out, e))?
                    .ok_or_else(|| {
                        Error::msg(format!("archive hard link {} has no target", raw.display()))
                    })?
                    .into_owned();
                let safe_target = safe_member_path(&target)?;
                // The kernel resolves the source too, so a hard link to
                // `planted-link/.ssh/id_rsa` would pull a file from outside the
                // payload into it.
                let source = dest.join(&safe_target);
                walk_inside(dest, &source, false)?;
                ensure_parent(dest, &out)?;
                std::fs::hard_link(&source, &out).map_err(|e| Error::io(&out, e))?;
            }
            EntryType::Regular | EntryType::Continuous | EntryType::GNUSparse => {
                ensure_parent(dest, &out)?;
                entry.unpack(&out).map_err(|e| Error::io(&out, e))?;
            }
            // Character/block devices and fifos have no place in a release.
            _ => continue,
        }
    }
    Ok(())
}

impl Extractor for TarGzExtractor {
    fn id(&self) -> &str {
        "tar.gz"
    }
    fn detect(&self, path: &Path, head: &[u8]) -> bool {
        head.starts_with(GZIP_MAGIC) && looks_like_tar(&gzip_inner_head(path))
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let file = File::open(src).map_err(|e| Error::io(src, e))?;
        unpack_tar(flate2::read::GzDecoder::new(BufReader::new(file)), dest)
    }
}

impl Extractor for TarXzExtractor {
    fn id(&self) -> &str {
        "tar.xz"
    }
    fn detect(&self, _path: &Path, head: &[u8]) -> bool {
        head.starts_with(XZ_MAGIC)
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let file = File::open(src).map_err(|e| Error::io(src, e))?;
        let mut reader = BufReader::new(file);
        // ponytail: xz is decompressed to memory; release tarballs are tens of
        // MiB. Stream it if ketch ever installs something genuinely large.
        let mut plain = Vec::new();
        lzma_rs::xz_decompress(&mut reader, &mut plain)
            .map_err(|e| Error::parse(src.display().to_string(), e.to_string()))?;
        unpack_tar(std::io::Cursor::new(plain), dest)
    }
}

impl Extractor for TarBz2Extractor {
    fn id(&self) -> &str {
        "tar.bz2"
    }
    fn detect(&self, _path: &Path, head: &[u8]) -> bool {
        head.starts_with(BZ2_MAGIC)
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let file = File::open(src).map_err(|e| Error::io(src, e))?;
        unpack_tar(bzip2::read::BzDecoder::new(BufReader::new(file)), dest)
    }
}

impl Extractor for TarExtractor {
    fn id(&self) -> &str {
        "tar"
    }
    fn detect(&self, _path: &Path, head: &[u8]) -> bool {
        looks_like_tar(head)
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let file = File::open(src).map_err(|e| Error::io(src, e))?;
        unpack_tar(BufReader::new(file), dest)
    }
}

impl Extractor for ZipExtractor {
    fn id(&self) -> &str {
        "zip"
    }
    fn detect(&self, _path: &Path, head: &[u8]) -> bool {
        head.starts_with(ZIP_MAGIC)
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let file = File::open(src).map_err(|e| Error::io(src, e))?;
        let mut zip = zip::ZipArchive::new(BufReader::new(file))
            .map_err(|e| Error::parse(src.display().to_string(), e.to_string()))?;

        for index in 0..zip.len() {
            let mut entry = zip
                .by_index(index)
                .map_err(|e| Error::parse(src.display().to_string(), e.to_string()))?;
            let name = entry.name().to_string();
            // Finder metadata, not payload.
            if name.starts_with("__MACOSX/") || name.ends_with("/.DS_Store") {
                continue;
            }
            let safe = safe_member_path(Path::new(&name))?;
            let out = dest.join(&safe);

            if entry.is_dir() || name.ends_with('/') {
                walk_inside(dest, &out, true)?;
                continue;
            }
            let mode = entry.unix_mode();
            ensure_parent(dest, &out)?;

            // Zip stores a symlink as a regular member whose body is the target.
            if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
                let mut target = String::new();
                entry
                    .read_to_string(&mut target)
                    .map_err(|e| Error::io(&out, e))?;
                let target = PathBuf::from(target.trim());
                check_link_target(&safe, &target)?;
                std::os::unix::fs::symlink(&target, &out).map_err(|e| Error::io(&out, e))?;
                continue;
            }

            let mut file = File::create(&out).map_err(|e| Error::io(&out, e))?;
            std::io::copy(&mut entry, &mut file).map_err(|e| Error::io(&out, e))?;
            // Without this, every binary in a zip lands non-executable.
            if let Some(mode) = mode {
                std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o7777))
                    .map_err(|e| Error::io(&out, e))?;
            }
        }
        Ok(())
    }
}

impl Extractor for GzFileExtractor {
    fn id(&self) -> &str {
        "gz"
    }
    fn detect(&self, path: &Path, head: &[u8]) -> bool {
        head.starts_with(GZIP_MAGIC) && !looks_like_tar(&gzip_inner_head(path))
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let stem = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "payload".to_string());
        let stem = stem.strip_suffix(".gz").unwrap_or(&stem).to_string();
        let out = dest.join(safe_member_path(Path::new(&stem))?);

        let file = File::open(src).map_err(|e| Error::io(src, e))?;
        let mut decoder = flate2::read::GzDecoder::new(BufReader::new(file));
        let mut written = File::create(&out).map_err(|e| Error::io(&out, e))?;
        std::io::copy(&mut decoder, &mut written).map_err(|e| Error::io(&out, e))?;
        drop(written);
        // A lone gzipped file in a release is a program; nothing else is
        // published this way.
        std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::io(&out, e))
    }
}

impl Extractor for RawBinaryExtractor {
    fn id(&self) -> &str {
        "raw"
    }
    fn detect(&self, _path: &Path, _head: &[u8]) -> bool {
        true
    }
    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "payload".to_string());
        let out = dest.join(safe_member_path(Path::new(&name))?);
        std::fs::copy(src, &out).map_err(|e| Error::io(&out, e))?;
        std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| Error::io(&out, e))
    }
}

/// True when the first bytes are a Mach-O image, an ELF image, or a `#!` line.
///
/// Used to tell "a program" from "a README that happens to be marked +x".
pub fn is_program_head(head: &[u8]) -> bool {
    if head.starts_with(b"#!") {
        return true;
    }
    if head.starts_with(b"\x7fELF") {
        return true;
    }
    if head.len() < 4 {
        return false;
    }
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    matches!(
        magic,
        // Mach-O 32/64, both byte orders, plus the fat/universal wrappers.
        0xfeed_face | 0xcefa_edfe | 0xfeed_facf | 0xcffa_edfe | 0xcafe_babe | 0xbebafeca
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn symlink_entry(builder: &mut tar::Builder<Vec<u8>>, name: &str, target: &str) {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        builder.append_link(&mut header, name, target).unwrap();
    }

    /// The escape this guards against is not hypothetical: every member name
    /// below passes `safe_member_path` and `check_link_target`, because both
    /// reason about the name while the kernel resolves the link.
    #[test]
    fn a_planted_symlink_is_never_written_through() {
        let mut builder = tar::Builder::new(Vec::new());
        // 1. A link back to the payload root. Lexical depth 4, four `..` land
        //    on 0, so the target guard accepts it.
        symlink_entry(&mut builder, "a/b/c/d/link", "../../../..");
        // 2. A link named *through* member 1, so its real depth is four rather
        //    than the nine its name claims — and the surplus `..` escapes.
        symlink_entry(
            &mut builder,
            "a/b/c/d/link/e/f/g/h/esc",
            "../../../../../../../../..",
        );
        // 3. The payload, written through member 2.
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(5);
        header.set_mode(0o644);
        builder
            .append_data(
                &mut header,
                "a/b/c/d/link/e/f/g/h/esc/victim",
                &b"pwned"[..],
            )
            .unwrap();
        let archive = builder.into_inner().unwrap();

        let outer = tempfile::tempdir().unwrap();
        let dest = outer.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();

        assert!(
            unpack_tar(&archive[..], &dest).is_err(),
            "an entry named through a planted symlink must be refused"
        );
        // Member 2 would have been created here had the link been followed.
        assert!(!dest.join("e").exists(), "the symlink was resolved anyway");
    }

    #[test]
    fn ordinary_symlinks_inside_the_payload_still_work() {
        let mut builder = tar::Builder::new(Vec::new());
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(2);
        header.set_mode(0o755);
        builder
            .append_data(&mut header, "bin/tool", &b"hi"[..])
            .unwrap();
        symlink_entry(&mut builder, "bin/tool-alias", "tool");
        let archive = builder.into_inner().unwrap();

        let dest = tempfile::tempdir().unwrap();
        unpack_tar(&archive[..], dest.path()).unwrap();
        let alias = dest.path().join("bin/tool-alias");
        assert!(alias.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&alias).unwrap(), b"hi");
    }

    fn tar_gz_with(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, body, mode) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder.append_data(&mut header, name, *body).unwrap();
        }
        let plain = builder.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&plain).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn extracts_a_tar_gz_and_keeps_the_executable_bit() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("t.tar.gz");
        std::fs::write(&src, tar_gz_with(&[("rg-1/rg", b"#!/bin/sh\n", 0o755)])).unwrap();
        let dest = dir.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();

        TarGzExtractor.extract(&src, &dest).unwrap();
        let binary = dest.join("rg-1/rg");
        assert!(binary.is_file());
        assert_ne!(
            std::fs::metadata(&binary).unwrap().permissions().mode() & 0o111,
            0
        );
    }

    #[test]
    fn bzip2_archives_use_the_same_traversal_guard_as_tar() {
        let mut builder = tar::Builder::new(Vec::new());
        symlink_entry(&mut builder, "a/b/c/d/link", "../../../..");
        symlink_entry(
            &mut builder,
            "a/b/c/d/link/e/f/g/h/esc",
            "../../../../../../../../..",
        );
        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_mode(0o644);
        builder
            .append_data(
                &mut header,
                "a/b/c/d/link/e/f/g/h/esc/victim",
                &b"pwned"[..],
            )
            .unwrap();
        let plain = builder.into_inner().unwrap();
        let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        encoder.write_all(&plain).unwrap();
        let archive = encoder.finish().unwrap();

        let outer = tempfile::tempdir().unwrap();
        let src = outer.path().join("payload.tar.bz2");
        std::fs::write(&src, archive).unwrap();
        let dest = outer.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();

        assert!(TarBz2Extractor.extract(&src, &dest).is_err());
        assert!(!outer.path().join("e").exists());
    }

    #[test]
    fn detection_separates_tar_gz_from_a_lone_gz() {
        let dir = tempfile::tempdir().unwrap();

        let tarball = dir.path().join("a.gz");
        std::fs::write(&tarball, tar_gz_with(&[("x", b"y", 0o644)])).unwrap();
        let head = crate::extract::read_head(&tarball).unwrap();
        assert!(TarGzExtractor.detect(&tarball, &head));
        assert!(!GzFileExtractor.detect(&tarball, &head));

        let plain = dir.path().join("jq.gz");
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(b"\x7fELF and then some payload").unwrap();
        std::fs::write(&plain, encoder.finish().unwrap()).unwrap();
        let head = crate::extract::read_head(&plain).unwrap();
        assert!(!TarGzExtractor.detect(&plain, &head));
        assert!(GzFileExtractor.detect(&plain, &head));
    }

    #[test]
    fn refuses_links_that_escape_the_payload() {
        assert!(check_link_target(Path::new("bin/tool"), Path::new("../lib/x.dylib")).is_ok());
        assert!(check_link_target(Path::new("bin/tool"), Path::new("../../../.ssh/id")).is_err());
        assert!(check_link_target(Path::new("tool"), Path::new("/etc/passwd")).is_err());
        assert!(check_link_target(Path::new("tool"), Path::new("../outside")).is_err());
    }

    #[test]
    fn recognises_program_headers() {
        assert!(is_program_head(b"#!/bin/sh"));
        assert!(is_program_head(&[0xcf, 0xfa, 0xed, 0xfe, 0, 0]));
        assert!(is_program_head(b"\x7fELF\x02"));
        assert!(!is_program_head(b"# Readme\n"));
        assert!(!is_program_head(b""));
    }
}
