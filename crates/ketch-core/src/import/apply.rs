// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! What a `ketch import` run changes, decided before anything is touched.
//!
//! The decision is a pure function of four things: the manifest the source
//! converts to today, the file already on disk, what is installed, and the
//! host. Keeping it pure is what lets every idempotency rule be tested
//! without a network, a ketch root or an install.

use super::{Converted, IMPORT_HEADER};
use crate::error::{Error, Result};
use crate::model::{TargetSpec, Version, VersionSpec};

/// The outcome of comparing the source with what this machine has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// The file is current and so is the install: nothing to do.
    UpToDate,
    Apply(Apply),
}

/// Writes and the install that bring this machine in line with the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Apply {
    /// The manifest text to write; `None` when the file already holds it.
    pub write: Option<String>,
    /// The release to install.
    pub version: VersionSpec,
    /// Reinstall a version that is already there, because the manifest
    /// describing it changed (another binary, another asset).
    pub force: bool,
    /// The exact asset the source downloads for this host, so ketch takes
    /// the same file rather than scoring the release itself.
    pub asset: Option<String>,
    /// The checksum the source recorded for that asset.
    pub sha256: Option<String>,
    /// The source recorded no checksum for this host's asset.
    pub unchecked: bool,
}

/// What is installed under the imported name: its version and tag.
#[derive(Debug, Clone, Copy)]
pub struct Have<'a> {
    pub version: &'a Version,
    pub tag: &'a str,
}

/// Decide what to do. `on_disk` is the current text of the manifest file,
/// `label` names that file in errors.
///
/// A file that does not start with the import header was written by a
/// person, and replacing it would lose their work, so it is refused rather
/// than overwritten.
pub fn decide(
    converted: &Converted,
    rendered: &str,
    on_disk: Option<&str>,
    installed: Option<Have<'_>>,
    host: &TargetSpec,
    label: &str,
) -> Result<Step> {
    if let Some(text) = on_disk {
        if !text.starts_with(IMPORT_HEADER) {
            return Err(Error::msg(format!(
                "{label} was not written by `ketch import`, so it is left as it is; \
                 move it away to import `{}`",
                converted.name
            )));
        }
    }
    // Refused before the file is written: a manifest this machine cannot
    // install from would leave a package behind that `ketch install` fails on.
    let artifact = converted.artifact_for(host).ok_or_else(|| {
        let mut have: Vec<&str> = converted.artifacts.keys().map(String::as_str).collect();
        have.sort_unstable();
        Error::msg(format!(
            "the {} definition of `{}` has no download for {host}, only for {}",
            converted.backend,
            converted.package,
            have.join(", ")
        ))
    })?;

    let changed = on_disk != Some(rendered);
    let write = changed.then(|| rendered.to_string());
    let wanted = Version::parse(converted.tag.as_deref().unwrap_or(&converted.version));

    if let Some(have) = installed {
        if !changed && *have.version >= wanted {
            return Ok(Step::UpToDate);
        }
        // Newer than the catalogue: the catalogue is behind, not the user,
        // so the installed release stays and only the manifest is redone.
        if *have.version > wanted {
            return Ok(Step::Apply(Apply {
                write,
                version: VersionSpec::Exact(have.tag.to_string()),
                force: true,
                asset: None,
                sha256: None,
                unchecked: false,
            }));
        }
    }

    let same_version = installed.is_some_and(|have| *have.version == wanted);
    let version = match &converted.tag {
        Some(tag) => VersionSpec::Exact(tag.clone()),
        None => VersionSpec::Latest,
    };
    Ok(Step::Apply(Apply {
        write,
        version,
        force: changed && same_version,
        asset: Some(artifact.file.clone()),
        // A `latest/download` URL follows the repository, so a checksum
        // the catalogue took from an older release would not match.
        sha256: converted.tag.as_ref().and_then(|_| artifact.sha256.clone()),
        unchecked: artifact.sha256.is_none(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::{Artifact, Backend};
    use crate::model::{Arch, BinSpec, Os, PackageKind};
    use std::collections::BTreeMap;

    const MAC: TargetSpec = TargetSpec {
        os: Os::MacOs,
        arch: Arch::Aarch64,
    };
    const LINUX: TargetSpec = TargetSpec {
        os: Os::Linux,
        arch: Arch::X86_64,
    };

    fn converted(tag: Option<&str>, sha: Option<&str>) -> Converted {
        let mut artifacts = BTreeMap::new();
        artifacts.insert(
            MAC.to_string(),
            Artifact {
                file: "fly-1.2.0-darwin-arm64.tar.gz".into(),
                sha256: sha.map(str::to_string),
            },
        );
        Converted {
            backend: Backend::Brew,
            package: "fly".into(),
            name: "fly".into(),
            version: "1.2.0".into(),
            repo: "superfly/flyctl".into(),
            tag: tag.map(str::to_string),
            kind: PackageKind::Auto,
            bins: vec![BinSpec {
                path: None,
                name: Some("fly".into()),
            }],
            artifacts,
        }
    }

    fn rendered(c: &Converted) -> String {
        c.render().unwrap()
    }

    fn apply(step: Step) -> Apply {
        match step {
            Step::Apply(a) => a,
            Step::UpToDate => panic!("expected changes, got up to date"),
        }
    }

    #[test]
    fn a_first_import_writes_the_file_and_installs_the_pinned_asset() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let a = apply(decide(&c, &rendered(&c), None, None, &MAC, "fly.toml").unwrap());
        assert_eq!(a.write.as_deref(), Some(rendered(&c).as_str()));
        assert_eq!(a.version, VersionSpec::Exact("v1.2.0".into()));
        assert!(!a.force);
        assert_eq!(a.asset.as_deref(), Some("fly-1.2.0-darwin-arm64.tar.gz"));
        assert_eq!(a.sha256.as_deref(), Some("ab"));
        assert!(!a.unchecked);
    }

    #[test]
    fn the_same_file_and_the_same_version_installed_is_up_to_date() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let text = rendered(&c);
        let v = Version::parse("v1.2.0");
        let have = Have {
            version: &v,
            tag: "v1.2.0",
        };
        let step = decide(&c, &text, Some(&text), Some(have), &MAC, "fly.toml").unwrap();
        assert_eq!(step, Step::UpToDate);
    }

    #[test]
    fn a_new_upstream_version_rewrites_the_file_and_upgrades() {
        let old = converted(Some("v1.1.0"), Some("aa"));
        let new = converted(Some("v1.2.0"), Some("ab"));
        let v = Version::parse("v1.1.0");
        let have = Have {
            version: &v,
            tag: "v1.1.0",
        };
        let a = apply(
            decide(
                &new,
                &rendered(&new),
                Some(&rendered(&old)),
                Some(have),
                &MAC,
                "fly.toml",
            )
            .unwrap(),
        );
        assert_eq!(a.version, VersionSpec::Exact("v1.2.0".into()));
        assert!(!a.force, "a new version needs no force");
    }

    #[test]
    fn an_unchanged_file_with_an_older_install_upgrades_without_writing() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let text = rendered(&c);
        let v = Version::parse("v1.0.0");
        let have = Have {
            version: &v,
            tag: "v1.0.0",
        };
        let a = apply(decide(&c, &text, Some(&text), Some(have), &MAC, "fly.toml").unwrap());
        assert_eq!(a.write, None);
        assert_eq!(a.version, VersionSpec::Exact("v1.2.0".into()));
    }

    #[test]
    fn a_changed_manifest_at_the_installed_version_reinstalls_it() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let mut before = c.clone();
        before.bins[0].name = Some("flyctl".into());
        let v = Version::parse("v1.2.0");
        let have = Have {
            version: &v,
            tag: "v1.2.0",
        };
        let a = apply(
            decide(
                &c,
                &rendered(&c),
                Some(&rendered(&before)),
                Some(have),
                &MAC,
                "fly.toml",
            )
            .unwrap(),
        );
        assert!(a.write.is_some());
        assert!(a.force);
    }

    #[test]
    fn an_install_newer_than_the_catalogue_is_kept_and_only_relinked() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let mut before = c.clone();
        before.bins[0].name = Some("flyctl".into());
        let v = Version::parse("v2.0.0");
        let have = Have {
            version: &v,
            tag: "v2.0.0",
        };
        let a = apply(
            decide(
                &c,
                &rendered(&c),
                Some(&rendered(&before)),
                Some(have),
                &MAC,
                "fly.toml",
            )
            .unwrap(),
        );
        assert_eq!(a.version, VersionSpec::Exact("v2.0.0".into()));
        assert!(a.force);
        assert_eq!(a.asset, None, "the catalogue's file is for another version");
    }

    #[test]
    fn an_install_newer_than_the_catalogue_with_the_same_file_is_up_to_date() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let text = rendered(&c);
        let v = Version::parse("v2.0.0");
        let have = Have {
            version: &v,
            tag: "v2.0.0",
        };
        let step = decide(&c, &text, Some(&text), Some(have), &MAC, "fly.toml").unwrap();
        assert_eq!(step, Step::UpToDate);
    }

    #[test]
    fn a_file_somebody_wrote_by_hand_is_refused() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let err = decide(
            &c,
            &rendered(&c),
            Some("name = \"fly\"\nrepo = \"me/fly\"\n"),
            None,
            &MAC,
            "/m/fly.toml",
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("/m/fly.toml was not written by `ketch import`"),
            "{err}"
        );
    }

    #[test]
    fn a_host_the_source_has_no_build_for_is_refused_before_writing() {
        let c = converted(Some("v1.2.0"), Some("ab"));
        let err = decide(&c, &rendered(&c), None, None, &LINUX, "fly.toml")
            .unwrap_err()
            .to_string();
        assert!(err.contains("has no download for linux-x86_64"), "{err}");
        assert!(err.contains("macos-aarch64"), "{err}");
    }

    #[test]
    fn a_missing_checksum_is_flagged_and_not_invented() {
        let c = converted(Some("v1.2.0"), None);
        let a = apply(decide(&c, &rendered(&c), None, None, &MAC, "fly.toml").unwrap());
        assert!(a.unchecked);
        assert_eq!(a.sha256, None);
    }

    #[test]
    fn a_latest_download_url_installs_latest_without_the_old_checksum() {
        let c = converted(None, Some("ab"));
        let a = apply(decide(&c, &rendered(&c), None, None, &MAC, "fly.toml").unwrap());
        assert_eq!(a.version, VersionSpec::Latest);
        assert_eq!(a.sha256, None);
        assert!(!a.unchecked);
    }
}
