//! macOS.
//!
//! Asset scoring understands Apple's naming conventions (`darwin`, `apple`,
//! `universal`, `arm64` vs `aarch64`), placement knows the difference between a
//! CLI binary and a `.app` bundle, and trust checks run `codesign`/`spctl`
//! before any quarantine flag is cleared.

use super::{AssetScore, DoctorCheck, Placement, Platform, TrustVerdict};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::extract::archive::is_program_head;
use crate::extract::macos::copy_tree;
use crate::extract::Extractor;
use crate::model::{glob_match, Arch, BinSpec, LinkKind, LinkRecord, Os, PackageKind, TargetSpec};
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

/// Extra tokens that mark a macOS asset as a build by-product.
const BYPRODUCT_TOKENS: &[&str] = &["dsym", "debuginfo", "symbols"];

// ---------------------------------------------------------------------------
// Asset scoring
// ---------------------------------------------------------------------------

/// Does `needle` appear in `haystack` as a whole token?
///
/// Plain `contains` is not usable here: `darwin` contains `win`, `install`
/// contains `all`, and either would misroute an asset to the wrong platform.
fn token_at(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(needle).any(|(start, matched)| {
        let end = start + matched.len();
        let left = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let right = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        left && right
    })
}

fn find_token(haystack: &str, tokens: &[&'static str]) -> Option<&'static str> {
    tokens.iter().copied().find(|t| token_at(haystack, t))
}

/// Bonus and label for the container format, read off the file name.
///
/// The spread is deliberately small: it only breaks ties between assets that
/// already agree on OS and architecture.
fn container_bonus(lower: &str) -> (i32, &'static str) {
    const KNOWN: &[(&str, i32, &str)] = &[
        (".tar.gz", 8, "tar.gz"),
        (".tgz", 8, "tar.gz"),
        (".tar.xz", 7, "tar.xz"),
        (".txz", 7, "tar.xz"),
        (".tar.bz2", 5, "tar.bz2"),
        (".tar", 6, "tar"),
        (".zip", 6, "zip"),
        (".gz", 5, "gz"),
        (".dmg", 3, "dmg"),
        (".pkg", 2, "pkg"),
    ];
    for (suffix, bonus, label) in KNOWN {
        if lower.ends_with(suffix) {
            return (*bonus, label);
        }
    }
    // No recognised container: most likely the bare executable.
    (4, "raw")
}

fn is_rejected(lower: &str) -> bool {
    super::is_sidecar(lower)
        || super::NON_BINARY_TOKENS.iter().any(|t| lower.contains(t))
        || super::REJECTED_EXTENSIONS
            .iter()
            .any(|e| lower.ends_with(e))
        || BYPRODUCT_TOKENS.iter().any(|t| token_at(lower, t))
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

/// Remove whatever is at `path` — file, symlink or directory — treating "it
/// was not there" as success.
fn remove_any(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

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

fn ensure_executable(path: &Path) -> Result<()> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    let mode = meta.permissions().mode();
    if mode & 0o111 != 0 {
        return Ok(());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o111))
        .map_err(|e| Error::io(path, e))
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

/// A raw binary asset lands under the asset's own file name — `jq-macos-arm64`
/// — which is not what anyone wants on PATH. Rename to the package name only
/// when the discovered name plainly carries build metadata and there is no
/// second binary that the rename could collide with.
fn looks_like_build_artifact(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let platform_token = [
        Os::MacOs.tokens(),
        Arch::Aarch64.tokens(),
        Arch::X86_64.tokens(),
        Arch::Universal.tokens(),
    ]
    .iter()
    .flat_map(|set| set.iter())
    .any(|t| token_at(&lower, t));

    platform_token || has_version_run(&lower)
}

/// True for names carrying something like `1.2` or `v3`.
fn has_version_run(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    bytes
        .windows(3)
        .any(|w| w[0].is_ascii_digit() && w[1] == b'.' && w[2].is_ascii_digit())
        || bytes
            .windows(2)
            .any(|w| w[0] == b'v' && w[1].is_ascii_digit())
}

/// Every executable file in the payload that is a plausible entry point.
fn discover_executables(platform: &MacOsPlatform, root: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // Never descend into a bundle or a docs tree looking for a CLI.
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            e.path() == root
                || !(name.ends_with(".app")
                    || name.ends_with(".framework")
                    || NOISE_DIRS.contains(&name.as_str()))
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| platform.is_executable(p))
        .collect();

    // A `bin/` directory is an explicit statement about what to expose.
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

/// Resolve the manifest's explicit binary list against the extracted payload.
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

/// Whether an occupied destination is this package's own to replace.
///
/// Two kinds of evidence. A symlink pointing into `owned` — the package's
/// directory in the store, covering every version of it — was made by ketch for
/// this package. A copied `.app` leaves no mark on disk at all, so the only
/// evidence there is the record written when it was placed.
///
/// Anything else is somebody else's: another package that claims the same
/// binary name, or an application the user installed themselves. Taking one
/// over silently means uninstalling this package later deletes it.
fn is_ours(link: &Path, owned: &Path, recorded: &[LinkRecord]) -> bool {
    recorded.iter().any(|r| r.link == link)
        || std::fs::read_link(link).is_ok_and(|target| target.starts_with(owned))
}

/// Clear a destination, or explain who already has it.
fn clear_destination(link: &Path, owned: &Path, recorded: &[LinkRecord]) -> Result<()> {
    destination_available(link, owned, recorded)?;
    remove_any(link).map_err(|e| Error::io(link, e))
}

fn destination_available(link: &Path, owned: &Path, recorded: &[LinkRecord]) -> Result<()> {
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

/// Check all destinations before replacing any old links. A multi-binary
/// upgrade must not install its first link and only then discover that its
/// second name belongs to another package.
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

fn link_binary(
    target: &Path,
    bin_dir: &Path,
    name: &str,
    owned: &Path,
    recorded: &[LinkRecord],
) -> Result<LinkRecord> {
    std::fs::create_dir_all(bin_dir).map_err(|e| Error::io(bin_dir, e))?;
    let link = bin_dir.join(name);
    clear_destination(&link, owned, recorded)?;

    // zip archives and `ditto` both lose the execute bit often enough.
    ensure_executable(target)?;
    std::os::unix::fs::symlink(target, &link).map_err(|e| Error::io(&link, e))?;
    Ok(LinkRecord {
        link,
        target: target.to_path_buf(),
        kind: LinkKind::Symlink,
    })
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
        std::os::unix::fs::symlink(bundle, &link).map_err(|e| Error::io(&link, e))?;
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
        let lower = asset_name.trim().to_ascii_lowercase();
        if lower.is_empty() || is_rejected(&lower) {
            return None;
        }

        // Anything that names a foreign OS is not ours, whatever else it says.
        if find_token(&lower, Os::Linux.tokens()).is_some()
            || find_token(&lower, Os::Windows.tokens()).is_some()
        {
            return None;
        }

        let mut score = 0;
        let mut reason = Vec::new();
        match find_token(&lower, Os::MacOs.tokens()) {
            Some(token) => {
                score += 50;
                reason.push(token.to_string());
            }
            // No OS in the name at all: single-platform projects do this, so it
            // stays a candidate but loses to anything explicit.
            None => score += 15,
        }

        let host = self.target.arch;
        let (arch, emulated) = if find_token(&lower, host.tokens()).is_some() {
            score += 40;
            (host, false)
        } else if find_token(&lower, Arch::Universal.tokens()).is_some() {
            score += 35;
            (Arch::Universal, false)
        } else if host == Arch::Aarch64 && find_token(&lower, Arch::X86_64.tokens()).is_some() {
            score += 10;
            (Arch::X86_64, true)
        } else if find_token(&lower, Arch::Aarch64.tokens()).is_some()
            || find_token(&lower, Arch::X86_64.tokens()).is_some()
        {
            // Names a real architecture, just not one this machine can run.
            return None;
        } else {
            score += 18;
            (Arch::Universal, false)
        };

        if emulated {
            if !allow_emulation {
                return None;
            }
            reason.push("x86_64 under Rosetta".to_string());
        } else {
            reason.push(arch.to_string());
        }

        let (bonus, container) = container_bonus(&lower);
        score += bonus;
        reason.push(container.to_string());

        Some(AssetScore {
            score,
            arch,
            emulated,
            reason: reason.join(" / "),
        })
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
        for record in links {
            // A symlink that no longer points where we put it belongs to
            // something else now — another package that took the name over, or
            // the user. Removing it would break whatever owns it.
            if std::fs::read_link(&record.link).is_ok_and(|t| t != record.target) {
                crate::ui::debug(&format!(
                    "leaving {}: it no longer points at {}",
                    record.link.display(),
                    record.target.display()
                ));
                continue;
            }
            // Anything already gone is fine: uninstall stays idempotent.
            remove_any(&record.link).map_err(|e| Error::io(&record.link, e))?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn score(name: &str) -> Option<AssetScore> {
        MacOsPlatform::new().score_asset(name, true)
    }

    #[test]
    fn token_matching_respects_word_boundaries() {
        // The whole reason `contains` is not good enough.
        assert!(!token_at("x86_64-apple-darwin", "win"));
        assert!(token_at("tool-windows-amd64.zip", "windows"));
        assert!(!token_at("tool-install.tar.gz", "all"));
        assert!(token_at("tool-universal-all.zip", "all"));
    }

    #[test]
    fn rejects_foreign_platforms_and_sidecars() {
        for name in [
            "rg-14.1.0-x86_64-unknown-linux-musl.tar.gz",
            "tool-windows-amd64.zip",
            "rg-14.1.0-aarch64-apple-darwin.tar.gz.sha256",
            "ripgrep_14.1.0_amd64.deb",
            "checksums.txt",
            "tool-macos-arm64.dSYM.zip",
        ] {
            assert!(score(name).is_none(), "should have rejected {name}");
        }
    }

    #[test]
    fn prefers_the_native_architecture_over_emulation() {
        let native = score("rg-14.1.0-aarch64-apple-darwin.tar.gz").unwrap();
        let rosetta = score("rg-14.1.0-x86_64-apple-darwin.tar.gz").unwrap();
        let host = TargetSpec::host().arch;
        if host == Arch::Aarch64 {
            assert!(native.score > rosetta.score);
            assert!(rosetta.emulated && !native.emulated);
            // Emulation is a choice, not a default the user cannot refuse.
            assert!(MacOsPlatform::new()
                .score_asset("rg-14.1.0-x86_64-apple-darwin.tar.gz", false)
                .is_none());
        }
    }

    #[test]
    fn universal_builds_are_accepted_on_any_mac() {
        let universal = score("tool-1.0-universal2-apple-darwin.tar.gz").unwrap();
        assert_eq!(universal.arch, Arch::Universal);
        assert!(!universal.emulated);
    }

    #[test]
    fn names_without_an_os_still_qualify_but_rank_lower() {
        let explicit = score("tool_1.0_darwin_arm64.tar.gz").unwrap();
        let bare = score("tool_1.0_arm64.tar.gz").unwrap();
        assert!(explicit.score > bare.score);
    }

    #[test]
    fn recognises_build_metadata_in_a_binary_name() {
        assert!(looks_like_build_artifact("jq-macos-arm64"));
        assert!(looks_like_build_artifact("tool-v1.2.3"));
        assert!(!looks_like_build_artifact("rg"));
        assert!(!looks_like_build_artifact("fd"));
    }

    #[test]
    fn making_a_binary_executable_does_not_grant_read_access() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("tool");
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        ensure_executable(&path).unwrap();

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o711);
    }

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
