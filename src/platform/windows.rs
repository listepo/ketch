//! Windows.
//!
//! Assets are `.exe` and `.zip`. Placement copies into the bin dir rather than
//! linking: creating a symlink needs a privilege most users do not have. Every
//! copy is recorded so uninstall can prove identity before deleting.

use super::scoring::looks_like_build_artifact;
use super::{AssetScore, DoctorCheck, Placement, Platform};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::extract::archive::is_program_head;
use crate::extract::Extractor;
use crate::model::{glob_match, BinSpec, LinkKind, LinkRecord, PackageKind, TargetSpec};
use crate::source::local::copy_tree;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Directories inside a payload that never hold the program itself.
const NOISE_DIRS: &[&str] = &[
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

/// Native Windows CLI releases: copy executables, record every destination.
pub struct WindowsPlatform {
    target: TargetSpec,
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsPlatform {
    pub fn new() -> Self {
        WindowsPlatform {
            target: TargetSpec::host(),
        }
    }
}

fn remove_any(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

fn sibling(original: &Path, suffix: &str) -> PathBuf {
    let name = original
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pkg".into());
    original.with_file_name(format!("{name}{suffix}"))
}

fn move_into_store(payload: &Path, store: &Path) -> Result<()> {
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

fn writable(dir: &Path) -> std::result::Result<(), String> {
    if !dir.exists() {
        return Err("does not exist".to_string());
    }
    tempfile::Builder::new()
        .prefix(".ketch-probe")
        .tempfile_in(dir)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn same_contents(a: &Path, b: &Path) -> bool {
    let Ok(mut fa) = std::fs::File::open(a) else {
        return false;
    };
    let Ok(mut fb) = std::fs::File::open(b) else {
        return false;
    };
    let mut ba = [0u8; 8192];
    let mut bb = [0u8; 8192];
    loop {
        match (fa.read(&mut ba), fb.read(&mut bb)) {
            (Ok(0), Ok(0)) => return true,
            (Ok(na), Ok(nb)) if na == nb && ba[..na] == bb[..nb] => {}
            _ => return false,
        }
    }
}

fn still_placed(record: &LinkRecord) -> bool {
    match record.kind {
        LinkKind::CopiedFile => {
            std::fs::symlink_metadata(&record.link).is_ok_and(|meta| meta.is_file())
                && same_contents(&record.link, &record.target)
        }
        LinkKind::Symlink | LinkKind::LinkedApp => {
            std::fs::read_link(&record.link).is_ok_and(|target| target == record.target)
        }
        LinkKind::CopiedApp => {
            std::fs::symlink_metadata(&record.link).is_ok_and(|meta| meta.is_dir())
        }
    }
}

fn dest_key(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}

fn is_ours(link: &Path, recorded: &[LinkRecord]) -> bool {
    recorded
        .iter()
        .any(|record| dest_key(&record.link) == dest_key(link) && still_placed(record))
}

fn destination_available(link: &Path, recorded: &[LinkRecord]) -> Result<()> {
    match std::fs::symlink_metadata(link) {
        Ok(_) if !is_ours(link, recorded) => Err(Error::msg(format!(
            "{} already exists and was not installed by ketch for this package; \
             move it aside first",
            link.display()
        ))),
        Ok(_) | Err(_) => Ok(()),
    }
}

fn clear_destination(link: &Path, recorded: &[LinkRecord]) -> Result<()> {
    destination_available(link, recorded)?;
    remove_any(link).map_err(|e| Error::io(link, e))
}

/// Keep a Windows executable suffix, or add `.exe` so PATH lookup finds it.
pub(crate) fn windows_bin_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if [".exe", ".cmd", ".bat", ".com", ".ps1"]
        .iter()
        .any(|s| lower.ends_with(s))
    {
        name.to_string()
    } else {
        format!("{name}.exe")
    }
}

fn discover_executables(platform: &WindowsPlatform, root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            e.path() == root || !NOISE_DIRS.contains(&name.as_str())
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
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

fn resolve_bin_specs(root: &Path, specs: &[BinSpec]) -> Result<Vec<(PathBuf, String)>> {
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
                candidates.iter().find(|p| {
                    p.file_name().is_some_and(|n| {
                        n == want || dest_key(Path::new(n)) == dest_key(Path::new(want))
                    })
                })
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
        out.push((path.clone(), windows_bin_name(&name)));
    }
    Ok(out)
}

fn cli_targets(
    platform: &WindowsPlatform,
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
                let stem = if sole && looks_like_build_artifact(&file_name) {
                    plan.name.to_string()
                } else {
                    file_name
                };
                (path, windows_bin_name(&stem))
            })
            .collect())
    } else {
        resolve_bin_specs(root, plan.bin_specs)
    }
}

fn copy_binary(
    target: &Path,
    bin_dir: &Path,
    name: &str,
    recorded: &[LinkRecord],
) -> Result<LinkRecord> {
    std::fs::create_dir_all(bin_dir).map_err(|e| Error::io(bin_dir, e))?;
    let link = bin_dir.join(name);
    // Case-insensitive: Tool.exe and tool.exe are the same destination.
    if let Ok(entries) = std::fs::read_dir(bin_dir) {
        for entry in entries.flatten() {
            if dest_key(&entry.path()) == dest_key(&link) {
                clear_destination(&entry.path(), recorded)?;
                break;
            }
        }
    }
    clear_destination(&link, recorded)?;
    std::fs::copy(target, &link).map_err(|e| Error::io(&link, e))?;
    Ok(LinkRecord {
        link,
        target: target.to_path_buf(),
        kind: LinkKind::CopiedFile,
    })
}

impl Platform for WindowsPlatform {
    fn id(&self) -> &str {
        "windows"
    }

    fn target(&self) -> TargetSpec {
        self.target
    }

    fn score_asset(&self, asset_name: &str, _allow_emulation: bool) -> Option<AssetScore> {
        super::scoring::score_windows_asset(asset_name, self.target.arch)
    }

    fn extractors(&self) -> Vec<Box<dyn Extractor>> {
        use crate::extract::archive::*;
        vec![
            Box::new(TarGzExtractor),
            Box::new(TarXzExtractor),
            Box::new(TarBz2Extractor),
            Box::new(TarExtractor),
            Box::new(ZipExtractor),
            Box::new(GzFileExtractor),
            Box::new(RawBinaryExtractor),
        ]
    }

    fn place(&self, plan: &Placement<'_>) -> Result<Vec<LinkRecord>> {
        if plan.link {
            let found = cli_targets(self, plan.payload_dir, plan)?;
            let mut destinations = HashSet::new();
            for (_, name) in &found {
                let link = plan.bin_dir.join(name);
                if !destinations.insert(dest_key(&link)) {
                    return Err(Error::msg(format!(
                        "multiple payload entries want to create {}",
                        link.display()
                    )));
                }
                destination_available(&link, plan.replacing)?;
            }
            if destinations.is_empty() {
                return Err(Error::EmptyPayload(plan.payload_dir.to_path_buf()));
            }
        }
        move_into_store(plan.payload_dir, plan.store_dir)?;
        if !plan.link {
            return Ok(Vec::new());
        }
        let mut links = Vec::new();
        for (target, name) in cli_targets(self, plan.store_dir, plan)? {
            links.push(copy_binary(&target, plan.bin_dir, &name, plan.replacing)?);
        }
        if links.is_empty() {
            return Err(Error::EmptyPayload(plan.store_dir.to_path_buf()));
        }
        Ok(links)
    }

    fn unplace(&self, links: &[LinkRecord]) -> Result<()> {
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

    fn is_executable(&self, path: &Path) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if [".exe", ".cmd", ".bat", ".com", ".ps1"]
            .iter()
            .any(|s| name.ends_with(s))
        {
            return true;
        }
        crate::extract::read_head(path)
            .map(|head| is_program_head(&head))
            .unwrap_or(false)
    }

    fn doctor(&self, cfg: &Config) -> Vec<DoctorCheck> {
        let mut checks = Vec::new();
        for (label, dir) in [
            ("root", &cfg.root),
            ("store", &cfg.store_dir),
            ("bin", &cfg.bin_dir),
        ] {
            checks.push(match writable(dir) {
                Ok(()) => DoctorCheck::ok(label, format!("{} is writable", dir.display())),
                Err(e) => DoctorCheck::fail(
                    label,
                    format!("{}: {e}", dir.display()),
                    format!("mkdir {}", dir.display()),
                ),
            });
        }
        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PackageKind;

    fn cmd_body(says: &str) -> Vec<u8> {
        format!("@echo off\r\necho {says}\r\n").into_bytes()
    }

    fn placement<'a>(
        payload: &'a Path,
        store: &'a Path,
        bin: &'a Path,
        apps: &'a Path,
    ) -> Placement<'a> {
        Placement {
            name: "tool",
            version: "1.0",
            payload_dir: payload,
            store_dir: store,
            bin_dir: bin,
            apps_dir: apps,
            kind: PackageKind::Binary,
            bin_specs: &[],
            replacing: &[],
            link_apps: false,
            link: true,
        }
    }

    #[test]
    fn windows_bin_name_adds_exe_unless_already_suffixed() {
        assert_eq!(windows_bin_name("rg"), "rg.exe");
        assert_eq!(windows_bin_name("rg.exe"), "rg.exe");
        assert_eq!(windows_bin_name("tool.cmd"), "tool.cmd");
    }

    #[test]
    fn place_copies_a_cmd_script_and_records_it() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        std::fs::create_dir_all(payload.join("bin")).unwrap();
        std::fs::write(payload.join("bin/tool.cmd"), cmd_body("v1")).unwrap();
        let store = tmp.path().join("store/tool/1.0");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let links = WindowsPlatform::new()
            .place(&placement(&payload, &store, &bin, tmp.path()))
            .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkKind::CopiedFile);
        assert_eq!(links[0].link, bin.join("tool.cmd"));
        assert!(bin.join("tool.cmd").is_file());
        assert!(!bin.join("tool.cmd").is_symlink());
    }

    #[test]
    fn case_insensitive_collision_is_the_same_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        std::fs::create_dir_all(payload.join("bin")).unwrap();
        std::fs::write(payload.join("bin/Tool.cmd"), cmd_body("new")).unwrap();
        let store = tmp.path().join("store/tool/1.0");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tool.cmd"), cmd_body("user")).unwrap();

        let err = WindowsPlatform::new()
            .place(&placement(&payload, &store, &bin, tmp.path()))
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert!(std::fs::read(bin.join("tool.cmd"))
            .unwrap()
            .windows(b"user")
            .next()
            .is_some());
    }

    #[test]
    fn unplace_refuses_to_delete_a_replaced_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store/pkg/1.0");
        std::fs::create_dir_all(&store).unwrap();
        let target = store.join("tool.cmd");
        std::fs::write(&target, cmd_body("ours")).unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join("tool.cmd");
        std::fs::copy(&target, &link).unwrap();
        let record = LinkRecord {
            link: link.clone(),
            target,
            kind: LinkKind::CopiedFile,
        };
        let p = WindowsPlatform::new();
        p.unplace(std::slice::from_ref(&record)).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());

        std::fs::write(&link, cmd_body("mine")).unwrap();
        p.unplace(&[record]).unwrap();
        assert!(link.is_file(), "the user's file must survive");
    }
}
