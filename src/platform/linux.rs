//! Linux.
//!
//! Asset scoring understands GNU/musl triples; placement is bin-dir symlinks
//! only. There is no `.app` equivalent and no quarantine to clear.

use super::unix::{place_cli, unplace, writable};
use super::{AssetScore, DoctorCheck, Placement, Platform};
use crate::config::Config;
use crate::error::Result;
use crate::extract::archive::is_program_head;
use crate::extract::Extractor;
use crate::model::{LinkRecord, TargetSpec};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

/// Native Linux CLI releases: tar/zip assets, bin-dir symlinks, no macOS
/// trust or app-bundle behaviour.
pub struct LinuxPlatform {
    target: TargetSpec,
}

impl Default for LinuxPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxPlatform {
    pub fn new() -> Self {
        LinuxPlatform {
            target: TargetSpec::host(),
        }
    }
}

impl Platform for LinuxPlatform {
    fn id(&self) -> &str {
        "linux"
    }

    fn target(&self) -> TargetSpec {
        self.target
    }

    fn score_asset(&self, asset_name: &str, allow_emulation: bool) -> Option<AssetScore> {
        super::scoring::score_linux_asset(
            asset_name,
            self.target.arch,
            allow_emulation,
            cfg!(target_env = "musl"),
        )
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
        place_cli(self, plan)
    }

    fn unplace(&self, links: &[LinkRecord]) -> Result<()> {
        unplace(links)
    }

    fn is_executable(&self, path: &Path) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
            return false;
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
                    format!("mkdir -p {} && chmod u+w {}", dir.display(), dir.display()),
                ),
            });
        }
        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LinkKind, PackageKind};
    use crate::platform::TrustVerdict;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn program(dir: &Path, name: &str, body: &[u8]) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn placement<'a>(
        payload: &'a Path,
        store: &'a Path,
        bin: &'a Path,
        apps: &'a Path,
    ) -> Placement<'a> {
        Placement {
            name: "rg",
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
    fn id_and_target_are_linux() {
        let p = LinuxPlatform::new();
        assert_eq!(p.id(), "linux");
        assert_eq!(p.target().os, crate::model::Os::Linux);
    }

    #[test]
    fn place_symlinks_a_discovered_binary() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        program(&payload.join("bin"), "rg", b"#!/bin/sh\necho rg\n");
        let store = tmp.path().join("store/rg/1.0");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let links = LinuxPlatform::new()
            .place(&placement(&payload, &store, &bin, tmp.path()))
            .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].kind, LinkKind::Symlink);
        assert_eq!(links[0].link, bin.join("rg"));
        assert!(bin.join("rg").is_symlink());
    }

    #[test]
    fn unplace_leaves_a_file_the_user_put_where_a_link_was() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store/pkg/1.0");
        let target = program(&store, "tool", b"#!/bin/sh\n");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join("tool");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let record = LinkRecord {
            link: link.clone(),
            target,
            kind: LinkKind::Symlink,
        };
        let p = LinuxPlatform::new();
        p.unplace(std::slice::from_ref(&record)).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());

        std::fs::write(&link, b"mine").unwrap();
        p.unplace(&[record]).unwrap();
        assert_eq!(std::fs::read(&link).unwrap(), b"mine");
    }

    #[test]
    fn occupied_destination_that_is_not_ours_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        program(&payload.join("bin"), "tool", b"#!/bin/sh\necho new\n");
        let store = tmp.path().join("store/pkg/1.0");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("tool"), b"mine").unwrap();

        let err = LinuxPlatform::new()
            .place(&placement(&payload, &store, &bin, tmp.path()))
            .unwrap_err();
        assert!(err.to_string().contains("already exists"));
        assert_eq!(std::fs::read(bin.join("tool")).unwrap(), b"mine");
    }

    #[test]
    fn verify_trust_is_not_applicable() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            LinuxPlatform::new().verify_trust(tmp.path()).unwrap(),
            TrustVerdict::NotApplicable
        );
    }
}
