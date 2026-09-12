//! Unix symlink, mode and ownership helpers shared by the macOS and Linux
//! backends. macOS-only app placement stays in `macos.rs`.
#![cfg(unix)]

use super::Platform;
use crate::error::{Error, Result};
use crate::model::{glob_match, BinSpec, LinkKind, LinkRecord};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use super::scoring::looks_like_build_artifact;
#[cfg(target_os = "linux")]
use super::Placement;
#[cfg(target_os = "linux")]
use crate::model::PackageKind;
#[cfg(target_os = "linux")]
use crate::source::local::copy_tree;
#[cfg(target_os = "linux")]
use std::collections::HashSet;

/// Directories inside a payload that never hold the program itself.
pub(crate) const NOISE_DIRS: &[&str] = &[
    "share",
    "doc",
    "docs",
    "man",
    "completions",
    "complete",
    "etc",
    "lib",
    "include",
    "licenses",
    "_internal",
    "resources",
];

/// Remove whatever is at `path` — file, symlink or directory — treating "it
/// was not there" as success.
pub(crate) fn remove_any(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Add the bits a program needs to run, leaving every other bit alone.
///
/// Owner read comes with the execute bits: an archive can name a script member
/// `0o200`, and the kernel wants to read a script before it runs it — an
/// execute-only payload installs, records and then fails with "Permission
/// denied" the first time it is used.
pub(crate) fn ensure_executable(path: &Path) -> Result<()> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let mode = meta.permissions().mode();
    if mode & 0o500 == 0o500 {
        return Ok(());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o500))
        .map_err(|e| Error::io(path, e))
}

pub(crate) fn symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).map_err(|e| Error::io(link, e))
}

/// Whether what is at a recorded path is still what ketch put there.
///
/// A symlink carries its own evidence: it has to point where the record says.
/// A copied `.app` leaves no mark on disk at all, which is why the record
/// exists — but only a directory can be one, and a matching record must never
/// authorize deleting a file the user put in its place.
pub(crate) fn still_placed(record: &LinkRecord) -> bool {
    match record.kind {
        LinkKind::Symlink | LinkKind::LinkedApp => {
            std::fs::read_link(&record.link).is_ok_and(|target| target == record.target)
        }
        LinkKind::CopiedFile => {
            std::fs::symlink_metadata(&record.link).is_ok_and(|meta| meta.is_file())
        }
        LinkKind::CopiedApp => {
            std::fs::symlink_metadata(&record.link).is_ok_and(|meta| meta.is_dir())
                && record.target.exists()
        }
    }
}

/// Whether an occupied destination is this package's own to replace.
///
/// Two kinds of evidence. A symlink pointing into `owned` — the package's
/// directory in the store, covering every version of it — was made by ketch for
/// this package. A copied `.app` leaves no mark on disk at all, so the only
/// evidence there is the record written when it was placed.
///
/// Either way the disk has the last word: a record whose path now holds
/// something else is stale — the user replaced it, or another package took the
/// name over — and replacing that would destroy work ketch never made.
///
/// Anything else is somebody else's: another package that claims the same
/// binary name, or an application the user installed themselves. Taking one
/// over silently means uninstalling this package later deletes it.
pub(crate) fn is_ours(link: &Path, owned: &Path, recorded: &[LinkRecord]) -> bool {
    if recorded
        .iter()
        .any(|record| record.link == link && still_placed(record))
    {
        return true;
    }
    // Lexical `starts_with` alone would treat `owned/../../elsewhere` as ours.
    let Ok(target) = std::fs::read_link(link) else {
        return false;
    };
    if target
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return false;
    }
    target.starts_with(owned)
}

/// Clear a destination, or explain who already has it.
pub(crate) fn clear_destination(link: &Path, owned: &Path, recorded: &[LinkRecord]) -> Result<()> {
    destination_available(link, owned, recorded)?;
    remove_any(link).map_err(|e| Error::io(link, e))
}

pub(crate) fn destination_available(
    link: &Path,
    owned: &Path,
    recorded: &[LinkRecord],
) -> Result<()> {
    match std::fs::symlink_metadata(link) {
        Ok(_) if !is_ours(link, owned, recorded) => Err(Error::msg(format!(
            "{} already exists and was not installed by ketch for this package; \
             move it aside first",
            link.display()
        ))),
        Ok(_) => Ok(()),
        Err(_) => Ok(()),
    }
}

pub(crate) fn writable(dir: &Path) -> std::result::Result<(), String> {
    if !dir.exists() {
        return Err("does not exist".to_string());
    }
    tempfile::Builder::new()
        .prefix(".ketch-probe")
        .tempfile_in(dir)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
pub(crate) fn sibling(original: &Path, suffix: &str) -> PathBuf {
    let name = original
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pkg".into());
    original.with_file_name(format!("{name}{suffix}"))
}

/// Move the extracted payload to its final home, falling back to a copy when
/// the cache and the store are on different filesystems.
///
/// The replacement is assembled beside the destination and swapped in last.
/// Deleting the old directory first would mean an upgrade that fails while
/// copying leaves the user with no working version of a package they already
/// had installed.
#[cfg(target_os = "linux")]
pub(crate) fn move_into_store(payload: &Path, store: &Path) -> Result<()> {
    if payload == store {
        return Ok(());
    }
    if let Some(parent) = store.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let staged = sibling(store, ".incoming");
    let _ = remove_any(&staged);
    if std::fs::rename(payload, &staged).is_err() {
        copy_tree(payload, &staged)?;
    }

    if store.symlink_metadata().is_err() {
        return std::fs::rename(&staged, store).map_err(|e| Error::io(store, e));
    }
    let retired = sibling(store, ".old");
    let _ = remove_any(&retired);
    std::fs::rename(store, &retired).map_err(|e| Error::io(store, e))?;
    if let Err(e) = std::fs::rename(&staged, store) {
        let _ = std::fs::rename(&retired, store);
        let _ = remove_any(&staged);
        return Err(Error::io(store, e));
    }
    let _ = remove_any(&retired);
    Ok(())
}

/// Keep a payload symlink if its target is a regular file inside the tree.
fn payload_executable_entry(entry: &walkdir::DirEntry, root: &Path) -> Option<PathBuf> {
    let path = entry.path();
    if entry.file_type().is_file() {
        return Some(path.to_path_buf());
    }
    if !entry.file_type().is_symlink() {
        return None;
    }
    let target = std::fs::read_link(path).ok()?;
    let resolved = if target.is_absolute() {
        target
    } else {
        path.parent()?.join(target)
    };
    let rel = resolved.strip_prefix(root).ok()?;
    if rel
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    std::fs::metadata(&resolved)
        .ok()
        .filter(|meta| meta.is_file())
        .map(|_| path.to_path_buf())
}

/// Every executable file in the payload that is a plausible entry point.
pub(crate) fn discover_executables(platform: &dyn Platform, root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            e.path() == root
                || !(name.ends_with(".app")
                    || name.ends_with(".framework")
                    || NOISE_DIRS.contains(&name.as_str()))
        })
        .filter_map(|e| e.ok())
        .filter_map(|e| payload_executable_entry(&e, root))
        .filter(|p| platform.is_executable(p))
        .collect();

    let in_bin: Vec<PathBuf> = found
        .iter()
        .filter(|p| {
            p.parent()
                .and_then(|d| d.file_name())
                .is_some_and(|n| n == "bin")
        })
        .cloned()
        .collect();
    if !in_bin.is_empty() {
        found = in_bin;
    }
    found.sort();
    found
}

/// Resolve the manifest's explicit binary list against the extracted payload.
pub(crate) fn resolve_bin_specs(root: &Path, specs: &[BinSpec]) -> Result<Vec<(PathBuf, String)>> {
    let candidates: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(6)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .collect();

    let mut out = Vec::new();
    for spec in specs {
        let matched = match &spec.path {
            Some(pattern) => candidates.iter().find(|p| {
                p.strip_prefix(root)
                    .ok()
                    .is_some_and(|rel| glob_match(pattern, &rel.to_string_lossy()))
            }),
            None => {
                let want = spec.name.as_deref().unwrap_or_default();
                candidates
                    .iter()
                    .find(|p| p.file_name().is_some_and(|n| n == want))
            }
        };
        let path = matched.ok_or_else(|| {
            Error::msg(format!(
                "manifest expects `{}` but the release payload does not contain it",
                spec.path
                    .clone()
                    .or_else(|| spec.name.clone())
                    .unwrap_or_else(|| "<unnamed>".into())
            ))
        })?;
        let name = spec.name.clone().unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        out.push((path.clone(), name));
    }
    Ok(out)
}

pub(crate) fn link_binary(
    target: &Path,
    bin_dir: &Path,
    name: &str,
    owned: &Path,
    recorded: &[LinkRecord],
) -> Result<LinkRecord> {
    std::fs::create_dir_all(bin_dir).map_err(|e| Error::io(bin_dir, e))?;
    let link = bin_dir.join(name);
    clear_destination(&link, owned, recorded)?;
    ensure_executable(target)?;
    symlink(target, &link)?;
    Ok(LinkRecord {
        link,
        target: target.to_path_buf(),
        kind: LinkKind::Symlink,
    })
}

#[cfg(target_os = "linux")]
fn cli_targets(
    platform: &dyn Platform,
    root: &Path,
    plan: &Placement<'_>,
) -> Result<Vec<(PathBuf, String)>> {
    if plan.kind == PackageKind::App {
        return Ok(Vec::new());
    }
    if plan.bin_specs.is_empty() {
        let found = discover_executables(platform, root);
        let sole = found.len() == 1;
        Ok(found
            .into_iter()
            .map(|path| {
                let file_name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let link_name = if sole && looks_like_build_artifact(&file_name) {
                    plan.name.to_string()
                } else {
                    file_name
                };
                (path, link_name)
            })
            .collect())
    } else {
        resolve_bin_specs(root, plan.bin_specs)
    }
}

#[cfg(target_os = "linux")]
fn preflight_cli(platform: &dyn Platform, plan: &Placement<'_>, owned: &Path) -> Result<()> {
    let names: Vec<String> = cli_targets(platform, plan.payload_dir, plan)?
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    let mut destinations = HashSet::new();
    for name in names {
        let link = plan.bin_dir.join(name);
        if !destinations.insert(link.clone()) {
            return Err(Error::msg(format!(
                "multiple payload entries want to create {}",
                link.display()
            )));
        }
        destination_available(&link, owned, plan.replacing)?;
    }
    if destinations.is_empty() {
        return Err(Error::EmptyPayload(plan.payload_dir.to_path_buf()));
    }
    Ok(())
}

/// Place CLI binaries only: bin-dir symlinks, never an `.app`.
#[cfg(target_os = "linux")]
pub(crate) fn place_cli(platform: &dyn Platform, plan: &Placement<'_>) -> Result<Vec<LinkRecord>> {
    let package_dir = plan.store_dir.parent().unwrap_or(plan.store_dir);
    if plan.link {
        preflight_cli(platform, plan, package_dir)?;
    }
    move_into_store(plan.payload_dir, plan.store_dir)?;
    if !plan.link {
        return Ok(Vec::new());
    }
    let mut links = Vec::new();
    for (target, name) in cli_targets(platform, plan.store_dir, plan)? {
        links.push(link_binary(
            &target,
            plan.bin_dir,
            &name,
            package_dir,
            plan.replacing,
        )?);
    }
    if links.is_empty() {
        return Err(Error::EmptyPayload(plan.store_dir.to_path_buf()));
    }
    Ok(links)
}

/// Undo `place`. Must tolerate links that are already gone.
pub(crate) fn unplace(links: &[LinkRecord]) -> Result<()> {
    for record in links {
        if std::fs::symlink_metadata(&record.link).is_err() {
            continue;
        }
        if !still_placed(record) {
            crate::ui::debug(&format!(
                "leaving {}: it is no longer what ketch placed there",
                record.link.display()
            ));
            continue;
        }
        remove_any(&record.link).map_err(|e| Error::io(&record.link, e))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LinkKind, LinkRecord};
    use std::path::PathBuf;

    #[test]
    fn making_a_binary_executable_leaves_every_other_bit_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("tool");
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        ensure_executable(&path).unwrap();

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        // Owner read and execute, and nothing for anyone else.
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn making_a_binary_executable_adds_the_read_a_script_needs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("tool");
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        // What a tar member with a write-only header mode unpacks as.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o200)).unwrap();

        ensure_executable(&path).unwrap();

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o700, "an execute-only script cannot run");
    }

    #[test]
    fn empty_destination_is_available() {
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(&owned).unwrap();
        let link = tmp.path().join("bin/tool");
        assert!(destination_available(&link, &owned, &[]).is_ok());
    }

    #[test]
    fn a_symlink_into_owned_is_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(owned.join("1.0")).unwrap();
        let target = owned.join("1.0/tool");
        std::fs::write(&target, b"#!/bin/sh\n").unwrap();
        let link = tmp.path().join("bin/tool");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(is_ours(&link, &owned, &[]));
        assert!(destination_available(&link, &owned, &[]).is_ok());
    }

    #[test]
    fn occupied_destination_that_is_not_ours_returns_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(&owned).unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::write(&elsewhere, b"#!/bin/sh\n").unwrap();
        let link = tmp.path().join("bin/tool");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        assert!(!is_ours(&link, &owned, &[]));
        assert!(destination_available(&link, &owned, &[]).is_err());
    }

    #[test]
    fn a_symlink_target_with_dotdot_is_not_treated_as_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(&owned).unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::write(&elsewhere, b"x").unwrap();
        let link = tmp.path().join("bin/tool");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        // Lexically under `owned`, but resolves outside via `..`.
        let sneaky = owned
            .join("1.0")
            .join("..")
            .join("..")
            .join("..")
            .join("elsewhere");
        assert!(
            sneaky.starts_with(&owned),
            "precondition: lexical starts_with alone would allow this"
        );
        std::os::unix::fs::symlink(&sneaky, &link).unwrap();
        assert!(
            !is_ours(&link, &owned, &[]),
            "`..` in a symlink target must not count as owned"
        );
        assert!(destination_available(&link, &owned, &[]).is_err());
    }

    /// Occupant at the destination path for table-driven ownership cases.
    enum Occupant {
        /// Plain file with no symlink mark — like a copied binary.
        RegularFile,
        /// Directory tree — like a copied `.app` bundle.
        Directory,
    }

    fn prepare_destination(tmp: &tempfile::TempDir, occupant: Occupant) -> (PathBuf, PathBuf) {
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(&owned).unwrap();
        let link = tmp.path().join("dest");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        match occupant {
            Occupant::RegularFile => std::fs::write(&link, b"x").unwrap(),
            Occupant::Directory => {
                std::fs::create_dir_all(link.join("Contents")).unwrap();
                std::fs::write(link.join("Contents/Info.plist"), b"x").unwrap();
            }
        }
        (owned, link)
    }

    #[test]
    fn a_recorded_link_is_ours_only_while_the_disk_still_agrees() {
        // A symlink record whose link still points at the recorded target.
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(owned.join("1.0")).unwrap();
        let target = owned.join("1.0/tool");
        std::fs::write(&target, b"#!/bin/sh\n").unwrap();
        let link = tmp.path().join("bin/tool");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let recorded = [LinkRecord {
            link: link.clone(),
            target,
            kind: LinkKind::Symlink,
        }];
        assert!(is_ours(&link, &owned, &recorded));
        assert!(destination_available(&link, &owned, &recorded).is_ok());

        // A copied bundle leaves no symlink behind, so the record is the only
        // evidence there can be — and only while the store copy is still there.
        let tmp = tempfile::tempdir().unwrap();
        let (owned, link) = prepare_destination(&tmp, Occupant::Directory);
        let target = owned.join("1.0/App.app");
        std::fs::create_dir_all(&target).unwrap();
        let recorded = [LinkRecord {
            link: link.clone(),
            target: target.clone(),
            kind: LinkKind::CopiedApp,
        }];
        assert!(is_ours(&link, &owned, &recorded));
        assert!(destination_available(&link, &owned, &recorded).is_ok());

        std::fs::remove_dir_all(&target).unwrap();
        assert!(
            !is_ours(&link, &owned, &recorded),
            "a CopiedApp record must not outlive the store copy it names"
        );
    }

    #[test]
    fn a_recorded_link_whose_place_now_holds_a_file_is_not_ours() {
        // The user replaced ketch's link with a copy of their own. A stale
        // record must not authorize deleting it.
        let tmp = tempfile::tempdir().unwrap();
        let (owned, link) = prepare_destination(&tmp, Occupant::RegularFile);
        let recorded = [LinkRecord {
            link: link.clone(),
            target: owned.join("1.0/tool"),
            kind: LinkKind::Symlink,
        }];

        assert!(!is_ours(&link, &owned, &recorded));
        assert!(destination_available(&link, &owned, &recorded).is_err());
        assert!(clear_destination(&link, &owned, &recorded).is_err());
        assert!(link.is_file(), "the user's file must survive");
    }

    #[test]
    fn a_recorded_symlink_that_points_somewhere_else_is_not_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(owned.join("1.0")).unwrap();
        let elsewhere = tmp.path().join("elsewhere");
        std::fs::write(&elsewhere, b"x").unwrap();
        let link = tmp.path().join("bin/tool");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        let recorded = [LinkRecord {
            link: link.clone(),
            target: owned.join("1.0/tool"),
            kind: LinkKind::Symlink,
        }];

        assert!(!is_ours(&link, &owned, &recorded));
        assert!(clear_destination(&link, &owned, &recorded).is_err());
    }

    #[test]
    fn a_regular_file_without_a_matching_record_is_not_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let (owned, link) = prepare_destination(&tmp, Occupant::RegularFile);
        assert!(!is_ours(&link, &owned, &[]));
        assert!(destination_available(&link, &owned, &[]).is_err());
    }

    #[test]
    fn a_directory_without_a_matching_record_is_not_ours() {
        let tmp = tempfile::tempdir().unwrap();
        let (owned, link) = prepare_destination(&tmp, Occupant::Directory);
        assert!(!is_ours(&link, &owned, &[]));
        assert!(destination_available(&link, &owned, &[]).is_err());
    }

    #[test]
    fn clear_destination_removes_an_owned_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let owned = tmp.path().join("store/pkg");
        std::fs::create_dir_all(owned.join("1.0")).unwrap();
        let target = owned.join("1.0/tool");
        std::fs::write(&target, b"#!/bin/sh\n").unwrap();
        let link = tmp.path().join("bin/tool");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        clear_destination(&link, &owned, &[]).unwrap();
        assert!(
            std::fs::symlink_metadata(&link).is_err(),
            "owned symlink should be gone"
        );
    }

    #[test]
    fn clear_destination_refuses_a_not_ours_occupant_and_leaves_it_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let (owned, link) = prepare_destination(&tmp, Occupant::RegularFile);

        assert!(clear_destination(&link, &owned, &[]).is_err());
        assert!(link.is_file(), "must not be deleted");
    }
}
