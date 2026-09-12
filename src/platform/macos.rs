//! macOS.
//!
//! Asset scoring understands Apple's naming conventions (`darwin`, `apple`,
//! `universal`, `arm64` vs `aarch64`), placement knows the difference between a
//! CLI binary and a `.app` bundle, and trust checks run `codesign`/`spctl`
//! before any quarantine flag is cleared.

use super::scoring::looks_like_build_artifact;
use super::unix::{
    clear_destination, destination_available, discover_executables, link_binary, remove_any,
    resolve_bin_specs, symlink, writable,
};
use super::{AssetScore, DoctorCheck, Placement, Platform, TrustVerdict};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::extract::archive::is_program_head;
use crate::extract::macos::copy_tree;
use crate::extract::Extractor;
use crate::model::{Arch, LinkKind, LinkRecord, PackageKind, TargetSpec};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub struct MacOsPlatform {
    target: TargetSpec,
}

impl Default for MacOsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl MacOsPlatform {
    pub fn new() -> Self {
        MacOsPlatform {
            target: TargetSpec::host(),
        }
    }
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Run a tool and capture stdout and stderr together.
///
/// `codesign` reports everything interesting on stderr, so splitting the two
/// would just mean reassembling them at every call site.
fn capture(program: &str, args: &[&OsStr]) -> (bool, String) {
    match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
    {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            (out.status.success(), text)
        }
        Err(e) => (false, e.to_string()),
    }
}

fn tool_exists(program: &str) -> bool {
    Path::new(program).exists()
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("no output")
        .to_string()
}

// ---------------------------------------------------------------------------
// Placement helpers
// ---------------------------------------------------------------------------

/// A path next to `original`, so swapping the two is a rename that never
/// crosses a filesystem.
fn sibling(original: &Path, suffix: &str) -> PathBuf {
    let mut name = original.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    original.with_file_name(name)
}

/// Move the extracted payload to its final home, falling back to a copy when
/// the cache and the store are on different filesystems.
///
/// The replacement is assembled beside the destination and swapped in last.
/// Deleting the old directory first — as the obvious version does — means an
/// upgrade that fails while copying leaves the user with no working version of
/// a package they already had installed.
fn move_into_store(payload: &Path, store: &Path) -> Result<()> {
    // `relink` re-runs placement over a payload that is already in the store;
    // without this the swap below would move it out from under itself.
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
        // Put back the version that was working before reporting the failure.
        let _ = std::fs::rename(&retired, store);
        let _ = remove_any(&staged);
        return Err(Error::io(store, e));
    }
    let _ = remove_any(&retired);
    Ok(())
}

/// True when `path` sits inside *another* bundle — a helper app nested in the
/// one being installed, which must not be placed in the applications directory
/// on its own.
///
/// Only the ancestors below `root` count. Testing the whole relative path
/// includes the leaf, and since every `.app` ends in `.app`, that answered
/// "inside a bundle" for every bundle there is.
fn is_inside_bundle(path: &Path, root: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(|rel| rel.parent())
        .is_some_and(|ancestors| {
            ancestors.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .is_some_and(crate::extract::is_bundle_name)
            })
        })
}

fn find_app_bundles(root: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(3)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.into_path())
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .filter(|p| !is_inside_bundle(p, root))
        .collect()
}

fn preflight_destinations(
    platform: &MacOsPlatform,
    plan: &Placement<'_>,
    owned: &Path,
) -> Result<()> {
    let bundles = if plan.kind != PackageKind::Binary {
        find_app_bundles(plan.payload_dir)
    } else {
        Vec::new()
    };
    let want_binaries = match plan.kind {
        PackageKind::App => false,
        PackageKind::Binary => true,
        PackageKind::Auto => bundles.is_empty(),
    };
    let binaries = if want_binaries {
        if plan.bin_specs.is_empty() {
            let found = discover_executables(platform, plan.payload_dir);
            let sole = found.len() == 1;
            found
                .into_iter()
                .map(|path| {
                    let file_name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    if sole && looks_like_build_artifact(&file_name) {
                        plan.name.to_string()
                    } else {
                        file_name
                    }
                })
                .collect()
        } else {
            resolve_bin_specs(plan.payload_dir, plan.bin_specs)?
                .into_iter()
                .map(|(_, name)| name)
                .collect()
        }
    } else {
        Vec::new()
    };

    let mut destinations = HashSet::new();
    for bundle in bundles {
        let Some(name) = bundle.file_name() else {
            continue;
        };
        let link = plan.apps_dir.join(name);
        if !destinations.insert(link.clone()) {
            return Err(Error::msg(format!(
                "multiple payload entries want to create {}",
                link.display()
            )));
        }
        destination_available(&link, owned, plan.replacing)?;
    }
    for name in binaries {
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

fn place_app(
    bundle: &Path,
    apps_dir: &Path,
    link_apps: bool,
    owned: &Path,
    recorded: &[LinkRecord],
) -> Result<LinkRecord> {
    std::fs::create_dir_all(apps_dir).map_err(|e| Error::io(apps_dir, e))?;
    let name = bundle.file_name().unwrap_or_default();
    let link = apps_dir.join(name);
    clear_destination(&link, owned, recorded)?;

    if link_apps {
        symlink(bundle, &link)?;
        return Ok(LinkRecord {
            link,
            target: bundle.to_path_buf(),
            kind: LinkKind::LinkedApp,
        });
    }
    // Copied by default: Launchpad and Spotlight both ignore symlinked apps.
    copy_tree(bundle, &link)?;
    Ok(LinkRecord {
        link,
        target: bundle.to_path_buf(),
        kind: LinkKind::CopiedApp,
    })
}

// ---------------------------------------------------------------------------

impl Platform for MacOsPlatform {
    fn id(&self) -> &str {
        "macos"
    }

    fn target(&self) -> TargetSpec {
        self.target
    }

    fn score_asset(&self, asset_name: &str, allow_emulation: bool) -> Option<AssetScore> {
        super::scoring::score_macos_asset(asset_name, self.target.arch, allow_emulation)
    }

    fn extractors(&self) -> Vec<Box<dyn Extractor>> {
        use crate::extract::archive::*;
        use crate::extract::macos::{DmgExtractor, PkgExtractor};
        vec![
            Box::new(DmgExtractor),
            Box::new(PkgExtractor),
            Box::new(TarGzExtractor),
            Box::new(TarXzExtractor),
            Box::new(TarBz2Extractor),
            Box::new(TarExtractor),
            Box::new(ZipExtractor),
            Box::new(GzFileExtractor),
            // Accepts anything, so it must stay last.
            Box::new(RawBinaryExtractor),
        ]
    }

    fn place(&self, plan: &Placement<'_>) -> Result<Vec<LinkRecord>> {
        let package_dir = plan.store_dir.parent().unwrap_or(plan.store_dir);
        if plan.link {
            preflight_destinations(self, plan, package_dir)?;
        }
        move_into_store(plan.payload_dir, plan.store_dir)?;
        if !plan.link {
            return Ok(Vec::new());
        }
        let mut links = Vec::new();
        // Every version of this package lives under here. The version being
        // replaced still owns its links at this point: install retires them
        // only once placement has succeeded.

        if plan.kind != PackageKind::Binary {
            for bundle in find_app_bundles(plan.store_dir) {
                links.push(place_app(
                    &bundle,
                    plan.apps_dir,
                    plan.link_apps,
                    package_dir,
                    plan.replacing,
                )?);
            }
        }

        // An app bundle carries its own executables; do not also scatter them
        // across PATH.
        let want_binaries = match plan.kind {
            PackageKind::App => false,
            PackageKind::Binary => true,
            PackageKind::Auto => links.is_empty(),
        };
        if want_binaries {
            let targets = if plan.bin_specs.is_empty() {
                let found = discover_executables(self, plan.store_dir);
                let sole = found.len() == 1;
                found
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
                    .collect()
            } else {
                resolve_bin_specs(plan.store_dir, plan.bin_specs)?
            };
            for (target, name) in targets {
                links.push(link_binary(
                    &target,
                    plan.bin_dir,
                    &name,
                    package_dir,
                    plan.replacing,
                )?);
            }
        }

        if links.is_empty() {
            return Err(Error::EmptyPayload(plan.store_dir.to_path_buf()));
        }
        Ok(links)
    }

    fn unplace(&self, links: &[LinkRecord]) -> Result<()> {
        super::unix::unplace(links)
    }

    fn verify_trust(&self, path: &Path) -> Result<TrustVerdict> {
        if !tool_exists("/usr/bin/codesign") {
            return Ok(TrustVerdict::NotApplicable);
        }
        let (valid, detail) = capture(
            "/usr/bin/codesign",
            &[
                OsStr::new("--verify"),
                OsStr::new("--strict"),
                OsStr::new("--"),
                path.as_os_str(),
            ],
        );
        if !valid {
            return Ok(TrustVerdict::Untrusted {
                detail: first_line(&detail),
            });
        }

        let (_, info) = capture(
            "/usr/bin/codesign",
            &[
                OsStr::new("-dv"),
                OsStr::new("--verbose=4"),
                OsStr::new("--"),
                path.as_os_str(),
            ],
        );
        let authority = info
            .lines()
            .find_map(|l| l.trim().strip_prefix("Authority="))
            .map(str::to_string);

        let Some(authority) = authority else {
            // Valid but ad-hoc: the signature proves nothing about origin.
            return Ok(TrustVerdict::Weak {
                detail: "ad-hoc signature, no signing authority".to_string(),
            });
        };
        if !authority.starts_with("Developer ID") {
            return Ok(TrustVerdict::Weak {
                detail: format!("signed by {authority}, which is not a distribution identity"),
            });
        }

        // Only a notarized binary passes system policy; a Developer ID
        // signature on its own does not.
        let (accepted, why) = capture(
            "/usr/sbin/spctl",
            &[
                OsStr::new("--assess"),
                OsStr::new("--type"),
                OsStr::new("exec"),
                OsStr::new("--"),
                path.as_os_str(),
            ],
        );
        if accepted {
            Ok(TrustVerdict::Trusted { authority })
        } else {
            Ok(TrustVerdict::Weak {
                detail: format!("{authority}; system policy: {}", first_line(&why)),
            })
        }
    }

    fn clear_quarantine(&self, path: &Path) -> Result<()> {
        // Non-zero simply means the attribute was not there.
        crate::extract::macos::try_tool(
            "/usr/bin/xattr",
            &[
                OsStr::new("-r"),
                OsStr::new("-d"),
                OsStr::new("com.apple.quarantine"),
                path.as_os_str(),
            ],
        );
        Ok(())
    }

    fn is_executable(&self, path: &Path) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
            return false;
        }
        // The bit alone is not enough: tarballs routinely ship +x READMEs.
        crate::extract::read_head(path)
            .map(|head| is_program_head(&head))
            .unwrap_or(false)
    }

    fn app_bundle_extension(&self) -> Option<&str> {
        Some(".app")
    }

    fn doctor(&self, cfg: &Config) -> Vec<DoctorCheck> {
        let mut checks = Vec::new();

        for (label, dir) in [("root", &cfg.root), ("store", &cfg.store_dir)] {
            checks.push(match writable(dir) {
                Ok(()) => DoctorCheck::ok(label, format!("{} is writable", dir.display())),
                Err(e) => DoctorCheck::fail(
                    label,
                    format!("{}: {e}", dir.display()),
                    format!("mkdir -p {} && chmod u+w {}", dir.display(), dir.display()),
                ),
            });
        }

        for tool in [
            "/usr/bin/codesign",
            "/usr/bin/xattr",
            "/usr/bin/hdiutil",
            "/usr/bin/ditto",
        ] {
            checks.push(if tool_exists(tool) {
                DoctorCheck::ok(tool, "present")
            } else {
                DoctorCheck::warn(
                    tool,
                    "missing",
                    "install the Command Line Tools: xcode-select --install",
                )
            });
        }

        if self.target.arch == Arch::Aarch64 {
            let rosetta = Path::new("/usr/libexec/rosetta/oahd").exists()
                || Path::new("/Library/Apple/usr/share/rosetta").exists();
            checks.push(if rosetta {
                DoctorCheck::ok("rosetta", "installed — x86_64-only releases will run")
            } else {
                DoctorCheck::warn(
                    "rosetta",
                    "not installed — x86_64-only releases will not run",
                    "softwareupdate --install-rosetta --agree-to-license",
                )
            });
        }
        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_bundles_are_found_but_their_helpers_are_not() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let app = root.join("Thing.app");
        std::fs::create_dir_all(app.join("Contents/Updater.app")).unwrap();
        std::fs::create_dir_all(root.join("Extra.app")).unwrap();

        let mut found = find_app_bundles(root);
        found.sort();
        assert_eq!(found, [root.join("Extra.app"), app]);
    }

    #[test]
    fn a_failed_upgrade_leaves_the_installed_version_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store/tool/1.0");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join("tool"), b"the working version").unwrap();

        // Nothing to move: the version already installed must survive.
        assert!(move_into_store(&tmp.path().join("missing"), &store).is_err());
        assert_eq!(
            std::fs::read(store.join("tool")).unwrap(),
            b"the working version"
        );

        let payload = tmp.path().join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join("tool"), b"the new version").unwrap();
        move_into_store(&payload, &store).unwrap();
        assert_eq!(
            std::fs::read(store.join("tool")).unwrap(),
            b"the new version"
        );
        assert!(!store.with_file_name("1.0.old").exists());
        assert!(!store.with_file_name("1.0.incoming").exists());
    }

    #[test]
    fn unplace_leaves_a_file_the_user_put_where_a_link_was() {
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store/pkg/1.0");
        std::fs::create_dir_all(&store).unwrap();
        let target = store.join("tool");
        std::fs::write(&target, b"#!/bin/sh\n").unwrap();

        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join("tool");
        let record = LinkRecord {
            link: link.clone(),
            target: target.clone(),
            kind: LinkKind::Symlink,
        };
        let platform = MacOsPlatform::new();

        // The link ketch made is ketch's to remove.
        std::os::unix::fs::symlink(&target, &link).unwrap();
        platform.unplace(std::slice::from_ref(&record)).unwrap();
        assert!(std::fs::symlink_metadata(&link).is_err());

        // The user replaced it with a copy of their own: the record is stale,
        // and removing what is there would delete their file.
        std::fs::write(&link, b"the user's own build").unwrap();
        platform.unplace(std::slice::from_ref(&record)).unwrap();
        assert_eq!(
            std::fs::read(&link).unwrap(),
            b"the user's own build",
            "a stale record must not authorize deleting the user's file"
        );

        // Already gone is not a failure: uninstall stays idempotent.
        std::fs::remove_file(&link).unwrap();
        platform.unplace(&[record]).unwrap();
    }

    #[test]
    fn a_binary_name_another_package_owns_is_not_taken_over() {
        let tmp = tempfile::tempdir().unwrap();
        let mine = tmp.path().join("store/mine");
        let theirs = tmp.path().join("store/theirs/1.0");
        std::fs::create_dir_all(mine.join("1.0")).unwrap();
        std::fs::create_dir_all(&theirs).unwrap();
        let target = mine.join("1.0/tool");
        std::fs::write(&target, b"#!/bin/sh\n").unwrap();
        std::fs::write(theirs.join("tool"), b"#!/bin/sh\n").unwrap();

        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(theirs.join("tool"), bin.join("tool")).unwrap();
        assert!(link_binary(&target, &bin, "tool", &mine, &[]).is_err());
        assert_eq!(
            std::fs::read_link(bin.join("tool")).unwrap(),
            theirs.join("tool"),
            "the other package must still own its link"
        );

        // An older version of the same package is ours to replace.
        let old = mine.join("0.9");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("tool"), b"#!/bin/sh\n").unwrap();
        std::fs::remove_file(bin.join("tool")).unwrap();
        std::os::unix::fs::symlink(old.join("tool"), bin.join("tool")).unwrap();
        let record = link_binary(&target, &bin, "tool", &mine, &[]).unwrap();
        assert_eq!(std::fs::read_link(&record.link).unwrap(), target);
    }

    #[test]
    fn placement_checks_all_binary_destinations_before_replacing_any() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        for name in ["first", "second"] {
            let path = payload.join(name);
            std::fs::write(&path, b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let external = tmp.path().join("external");
        std::fs::write(&external, b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&external, bin.join("second")).unwrap();

        let store = tmp.path().join("store/pkg/1.0");
        let apps = tmp.path().join("Applications");
        let package_dir = store.parent().unwrap();
        let plan = Placement {
            name: "pkg",
            version: "1.0",
            payload_dir: &payload,
            store_dir: &store,
            bin_dir: &bin,
            apps_dir: &apps,
            kind: PackageKind::Binary,
            bin_specs: &[],
            replacing: &[],
            link_apps: false,
            link: true,
        };

        assert!(preflight_destinations(&MacOsPlatform::new(), &plan, package_dir).is_err());
        assert!(
            !bin.join("first").exists(),
            "preflight must not create links"
        );
    }

    #[test]
    fn placement_preflight_checks_the_package_name_for_a_single_build_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        let artifact = payload.join("tool-macos-arm64");
        std::fs::write(&artifact, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&artifact, std::fs::Permissions::from_mode(0o755)).unwrap();

        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let external = tmp.path().join("external");
        std::fs::write(&external, b"#!/bin/sh\n").unwrap();
        std::os::unix::fs::symlink(&external, bin.join("tool")).unwrap();

        let store = tmp.path().join("store/tool/1.0");
        let apps = tmp.path().join("Applications");
        let plan = Placement {
            name: "tool",
            version: "1.0",
            payload_dir: &payload,
            store_dir: &store,
            bin_dir: &bin,
            apps_dir: &apps,
            kind: PackageKind::Binary,
            bin_specs: &[],
            replacing: &[],
            link_apps: false,
            link: true,
        };

        assert!(
            preflight_destinations(&MacOsPlatform::new(), &plan, store.parent().unwrap()).is_err()
        );
    }

    #[test]
    fn an_app_ketch_did_not_install_is_never_deleted() {
        let tmp = tempfile::tempdir().unwrap();
        let package_dir = tmp.path().join("store/thing");
        let bundle = package_dir.join("1.0/Thing.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        std::fs::write(bundle.join("Contents/Info.plist"), b"x").unwrap();

        let apps = tmp.path().join("Applications");
        let existing = apps.join("Thing.app");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(existing.join("mine.txt"), b"the user's own copy").unwrap();

        assert!(place_app(&bundle, &apps, false, &package_dir, &[]).is_err());
        assert!(existing.join("mine.txt").is_file(), "must not be deleted");

        // A copied bundle leaves no mark on disk; the record we wrote when we
        // placed it is the only thing that makes it ours to replace.
        let recorded = [LinkRecord {
            link: existing.clone(),
            target: bundle.clone(),
            kind: LinkKind::CopiedApp,
        }];
        place_app(&bundle, &apps, false, &package_dir, &recorded).unwrap();
        assert!(existing.join("Contents/Info.plist").is_file());
        assert!(!existing.join("mine.txt").exists());
    }

    #[test]
    fn a_link_that_now_points_elsewhere_survives_uninstall() {
        let tmp = tempfile::tempdir().unwrap();
        let ours = tmp.path().join("ours");
        let theirs = tmp.path().join("theirs");
        std::fs::write(&ours, b"x").unwrap();
        std::fs::write(&theirs, b"x").unwrap();
        let link = tmp.path().join("tool");
        std::os::unix::fs::symlink(&theirs, &link).unwrap();

        let record = LinkRecord {
            link: link.clone(),
            target: ours.clone(),
            kind: LinkKind::Symlink,
        };
        let platform = MacOsPlatform::new();
        platform.unplace(std::slice::from_ref(&record)).unwrap();
        assert!(link.symlink_metadata().is_ok(), "not ours to remove");

        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(&ours, &link).unwrap();
        platform.unplace(&[record]).unwrap();
        assert!(link.symlink_metadata().is_err());
    }
}
