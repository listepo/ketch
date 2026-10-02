// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! Plain records a foreign caller receives, converted from the core's types.
//!
//! They mirror `ketch_core` rather than deriving UniFFI traits on it, so the
//! core keeps no binding attributes and can change a field without changing
//! the foreign API. Paths and versions cross as strings: every target language
//! has those, and none of them has `PathBuf` or ketch's `Version`. Text that a
//! client app's author wrote (descriptions, changelogs, doctor details naming
//! their files) is passed through `changelog::sanitize` on the way out.

use ketch_core::changelog::{self, Entry, Origin};
use ketch_core::listing::{self, Latest, Local, Row};
use ketch_core::model::{InstalledPackage, Manifest, SourceInfo};
use ketch_core::platform::{CheckStatus, DoctorCheck};

/// One installed package.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Package {
    pub name: String,
    pub version: String,
    /// The release tag the version came from.
    pub tag: String,
    /// Where it came from, as `scheme:id` (`github:BurntSushi/ripgrep`).
    pub source: String,
    pub pinned: bool,
    /// Older versions still on disk, oldest first.
    pub retained: Vec<String>,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    /// The store directory holding the payload.
    pub prefix: String,
    /// The links on `PATH` (or in `/Applications`) that expose it.
    pub binaries: Vec<String>,
    /// How far the download is established: `signed`, `checksum` or
    /// `first use`.
    pub trust: String,
}

impl From<&InstalledPackage> for Package {
    fn from(pkg: &InstalledPackage) -> Self {
        Package {
            name: pkg.name.clone(),
            version: pkg.version.to_string(),
            tag: pkg.tag.clone(),
            source: pkg.source.to_string(),
            pinned: pkg.pinned,
            retained: pkg.retained.iter().map(|r| r.version.to_string()).collect(),
            installed_at: pkg.installed_at,
            prefix: pkg.prefix.display().to_string(),
            binaries: pkg
                .binaries()
                .map(|l| l.link.display().to_string())
                .collect(),
            trust: pkg.publisher_trust().to_string(),
        }
    }
}

/// What an install or upgrade placed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Installed {
    pub package: Package,
    /// The version it replaced, for an upgrade or a reinstall.
    pub replaced: Option<String>,
}

impl From<&ketch_core::install::Installed> for Installed {
    fn from(out: &ketch_core::install::Installed) -> Self {
        Installed {
            package: Package::from(&out.package),
            replaced: out.replaced.as_ref().map(ToString::to_string),
        }
    }
}

/// How to install. The defaults are `ketch install` with no flags, except that
/// an installed package with a newer release is updated rather than asked
/// about: the person already chose to install it in the front end.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InstallOptions {
    /// Reinstall even when the resolved version is already present.
    #[uniffi(default = false)]
    pub force: bool,
    /// Allow a prerelease to be the latest.
    #[uniffi(default = false)]
    pub prerelease: bool,
    /// Link the binaries onto `PATH` (and `.app`s into Applications).
    #[uniffi(default = true)]
    pub link: bool,
    /// Fail rather than trust a download no checksum was published for.
    #[uniffi(default = false)]
    pub require_checksum: bool,
    /// Which of several binaries sharing the package's name to link.
    #[uniffi(default = None)]
    pub bin: Option<String>,
}

impl Default for InstallOptions {
    fn default() -> Self {
        InstallOptions {
            force: false,
            prerelease: false,
            link: true,
            require_checksum: false,
            bin: None,
        }
    }
}

/// A package the registry, or the person's own manifests, know.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RegistryPackage {
    pub name: String,
    pub source: String,
    pub description: Option<String>,
    /// The newest version an earlier listing found, `None` when no fresh
    /// answer is cached. A search asks no source for it.
    pub latest: Option<String>,
}

impl RegistryPackage {
    pub(crate) fn new(m: &Manifest, latest: Option<String>) -> Self {
        RegistryPackage {
            name: m.name.clone(),
            source: m.source.to_string(),
            description: m.description.as_deref().map(changelog::sanitize),
            latest,
        }
    }
}

/// A repository a source found for a search, installable by `spec`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Repository {
    /// What to pass to `install`: `github:owner/repo`.
    pub spec: String,
    pub stars: Option<u64>,
    pub description: Option<String>,
}

impl Repository {
    pub(crate) fn new(scheme: &str, hit: &SourceInfo) -> Self {
        Repository {
            spec: format!("{scheme}:{}", hit.id),
            stars: hit.stars,
            description: hit.description.as_deref().map(changelog::sanitize),
        }
    }
}

/// What a search found: curated packages first, then repositories.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SearchResults {
    pub known: Vec<RegistryPackage>,
    pub repositories: Vec<Repository>,
}

/// An installed package with a newer release.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Upgrade {
    pub name: String,
    pub installed: String,
    pub latest: String,
    /// The release tag `upgrade` installs.
    pub tag: String,
    /// Held at `installed` by a pin: `upgrade` leaves it alone until it is
    /// unpinned.
    pub pinned: bool,
    /// The `ketch.lock` that holds the pin, when ketch knows one. `ketch
    /// sync` restores a pin without recording which file it came from, so
    /// today this is always `None`.
    pub held_by: Option<String>,
}

impl Upgrade {
    /// The upgrade a listing row offers, when it offers one. A pinned package
    /// is offered one too, marked as held, so an app can show what the pin is
    /// keeping back; the update rules are otherwise the ones `ketch outdated`
    /// uses.
    pub(crate) fn from_row(row: &Row) -> Option<Self> {
        let local = row.local.as_ref()?;
        let Latest::Found(found) = &row.latest else {
            return None;
        };
        let unpinned = Local {
            pinned: false,
            ..local.clone()
        };
        listing::update_available(&unpinned, found).then(|| Upgrade {
            name: local.name.clone(),
            installed: local.version.to_string(),
            latest: found.version.to_string(),
            tag: found.tag.clone(),
            pinned: local.pinned,
            held_by: None,
        })
    }
}

/// Where a changelog came from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ChangelogSource {
    /// A file inside the installed payload.
    File { path: String },
    /// Notes published with the release.
    Release,
}

/// What changed in one release of a package.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Changelog {
    pub name: String,
    pub version: String,
    pub source: ChangelogSource,
    /// The heading the section was found under. `None` from a file means no
    /// section matched and `body` is the whole file.
    pub heading: Option<String>,
    pub body: String,
}

impl Changelog {
    pub(crate) fn new(name: &str, version: &str, entry: Entry) -> Self {
        Changelog {
            name: name.to_string(),
            version: version.to_string(),
            source: match entry.origin {
                Origin::File(path) => ChangelogSource::File {
                    path: path.display().to_string(),
                },
                Origin::Release => ChangelogSource::Release,
            },
            heading: entry.heading.as_deref().map(changelog::sanitize),
            body: changelog::sanitize(&entry.body),
        }
    }
}

/// How a doctor check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CheckOutcome {
    Ok,
    Warn,
    Fail,
}

/// One line of `ketch doctor`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Check {
    pub name: String,
    pub outcome: CheckOutcome,
    pub detail: String,
    /// What to do about it, when there is something to do.
    pub fix: Option<String>,
}

impl From<&DoctorCheck> for Check {
    fn from(c: &DoctorCheck) -> Self {
        Check {
            name: c.name.clone(),
            outcome: match c.status {
                CheckStatus::Ok => CheckOutcome::Ok,
                CheckStatus::Warn => CheckOutcome::Warn,
                CheckStatus::Fail => CheckOutcome::Fail,
            },
            detail: changelog::sanitize(&c.detail),
            fix: c.fix.as_deref().map(changelog::sanitize),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ketch_core::listing::Found;
    use ketch_core::model::{
        LinkKind, LinkRecord, ManifestOrigin, PackageRef, RetainedVersion, TargetSpec, Version,
    };
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    fn installed() -> InstalledPackage {
        InstalledPackage {
            name: "rg".into(),
            version: Version::parse("14.1.0"),
            source: PackageRef::github("BurntSushi/ripgrep"),
            tag: "14.1.0".into(),
            target: TargetSpec::host(),
            asset_name: "rg.tar.gz".into(),
            sha256: "0".repeat(64),
            checksum_verified: true,
            installed_at: 1_700_000_000,
            prefix: PathBuf::from("/k/store/rg/14.1.0"),
            links: vec![LinkRecord {
                link: PathBuf::from("/k/bin/rg"),
                target: PathBuf::from("/k/store/rg/14.1.0/rg"),
                kind: LinkKind::Symlink,
                role: Default::default(),
            }],
            pinned: true,
            origin: ManifestOrigin::Inferred,
            manifest: None,
            local_kind: None,
            local_path: None,
            trust: Default::default(),
            retained: vec![RetainedVersion {
                version: Version::parse("14.0.0"),
                prefix: PathBuf::from("/k/store/rg/14.0.0"),
                sha256: "1".repeat(64),
                checksum_verified: true,
                links: Vec::new(),
                trust: Default::default(),
                provenance: None,
                tag: "14.0.0".into(),
                asset_name: "rg.tar.gz".into(),
                installed_at: 0,
                target: TargetSpec::host(),
            }],
            provenance: None,
            bin_choice: None,
        }
    }

    #[test]
    fn an_installed_package_crosses_as_strings() {
        assert_eq!(
            Package::from(&installed()),
            Package {
                name: "rg".into(),
                version: "14.1.0".into(),
                tag: "14.1.0".into(),
                source: "github:BurntSushi/ripgrep".into(),
                pinned: true,
                retained: vec!["14.0.0".into()],
                installed_at: 1_700_000_000,
                prefix: "/k/store/rg/14.1.0".into(),
                binaries: vec!["/k/bin/rg".into()],
                trust: "checksum".into(),
            }
        );
    }

    fn row(pinned: bool, latest: &str) -> Row {
        let local = Local {
            name: "rg".into(),
            source: PackageRef::github("BurntSushi/ripgrep"),
            version: Version::parse("14.0.0"),
            tag: "14.0.0".into(),
            pinned,
            retained: Vec::new(),
            prerelease: false,
        };
        let mut rows = ketch_core::listing::merge(vec![local], Vec::new());
        rows[0].latest = Latest::Found(Found {
            version: Version::parse(latest),
            tag: latest.into(),
        });
        rows.remove(0)
    }

    #[test]
    fn a_newer_release_is_an_upgrade_with_its_tag() {
        assert_eq!(
            Upgrade::from_row(&row(false, "14.1.0")),
            Some(Upgrade {
                name: "rg".into(),
                installed: "14.0.0".into(),
                latest: "14.1.0".into(),
                tag: "14.1.0".into(),
                pinned: false,
                held_by: None,
            })
        );
    }

    #[test]
    fn a_pinned_package_with_a_newer_release_is_an_upgrade_marked_held() {
        let held = Upgrade::from_row(&row(true, "14.1.0")).expect("offered");
        assert!(held.pinned);
        assert_eq!(held.latest, "14.1.0");
        assert_eq!(held.held_by, None);
    }

    #[test]
    fn a_current_package_is_no_upgrade_pinned_or_not() {
        assert_eq!(Upgrade::from_row(&row(false, "14.0.0")), None);
        assert_eq!(Upgrade::from_row(&row(true, "14.0.0")), None);
    }

    #[test]
    fn doctor_checks_keep_their_outcome_and_lose_control_characters() {
        let check = DoctorCheck::warn("links", "1 broken\u{1b}", "ketch link x");
        assert_eq!(
            Check::from(&check),
            Check {
                name: "links".into(),
                outcome: CheckOutcome::Warn,
                detail: "1 broken".into(),
                fix: Some("ketch link x".into()),
            }
        );
    }

    #[test]
    fn a_changelog_names_its_file() {
        let entry = Entry {
            origin: Origin::File(PathBuf::from("/p/CHANGELOG.md")),
            heading: Some("## 1.0".into()),
            body: "- fixed\u{1b}".into(),
        };
        assert_eq!(
            Changelog::new("rg", "1.0", entry),
            Changelog {
                name: "rg".into(),
                version: "1.0".into(),
                source: ChangelogSource::File {
                    path: "/p/CHANGELOG.md".into()
                },
                heading: Some("## 1.0".into()),
                body: "- fixed".into(),
            }
        );
    }
}
