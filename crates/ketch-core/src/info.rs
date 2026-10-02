// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! What `ketch info` knows about one package, gathered once for every front
//! end: the manifest, the installed record, what the source says about the
//! project and its newest release.
//!
//! Separate from the command so the CLI and the app bindings answer the same
//! question the same way; each only formats the answer. Nothing here is
//! filtered for a terminal: the prose in it is a client app's, and every
//! caller sanitises what it shows.

use crate::error::Result;
use crate::manifest::Resolver;
use crate::model::{
    InstalledPackage, Manifest, ManifestOrigin, PackageSpec, Release, SourceInfo, VersionSpec,
};
use crate::report::Ctx;
use crate::source::{ListOpts, SourceRegistry};
use crate::state::State;

/// One package, as far as it could be found out.
#[derive(Debug, Clone)]
pub struct Info {
    pub manifest: Manifest,
    /// Where the manifest came from; `None` when the registry no longer knows
    /// the package and the one recorded at install time was used.
    pub origin: Option<ManifestOrigin>,
    pub installed: Option<InstalledPackage>,
    /// The project's page on its forge, when the source has one.
    pub url: Option<String>,
    /// What the source says about the project.
    pub described: Option<SourceInfo>,
    /// The newest release, by the package's prerelease rules.
    pub latest: Option<Release>,
}

impl Info {
    /// The manifest's description, or the source's when it has none.
    pub fn description(&self) -> Option<&str> {
        self.manifest
            .description
            .as_deref()
            .or_else(|| self.described.as_ref()?.description.as_deref())
    }

    /// The manifest's homepage, or the source's when it has none.
    pub fn homepage(&self) -> Option<&str> {
        self.manifest
            .homepage
            .as_deref()
            .or_else(|| self.described.as_ref()?.homepage.as_deref())
    }
}

/// Everything known about `query`, a name or spec.
///
/// An installed package always has an answer, even when the registry has
/// forgotten the name it was installed under. A source that cannot be reached
/// costs only what it would have said: that is a warning on `cx.report`, not
/// an error, because the rest of the answer still stands.
pub fn gather(cx: &Ctx<'_>, state: &State, query: &str) -> Result<Info> {
    let installed = state.find(query).cloned();
    let spec = PackageSpec::parse(query);
    let (manifest, origin) = Resolver::new(cx)?.resolve_or_recorded(&spec, installed.as_ref())?;

    let sources = SourceRegistry::load(cx);
    let source = match sources.for_ref(&manifest.source) {
        Ok(s) => Some(s),
        Err(e) => {
            cx.report.warn(&format!("{}: {e}", manifest.name));
            None
        }
    };
    let described = source.as_ref().and_then(|s| {
        s.describe(&manifest.source.id).unwrap_or_else(|e| {
            cx.report.debug(&format!("describe failed: {e}"));
            None
        })
    });
    let opts = ListOpts {
        include_prerelease: cx.cfg.prerelease || manifest.prerelease,
        ..Default::default()
    };
    let latest = source.as_ref().and_then(|s| {
        match s.resolve(&manifest.source.id, &VersionSpec::Latest, &opts) {
            Ok(r) => Some(r),
            Err(e) => {
                cx.report.warn(&format!("{}: {e}", manifest.name));
                None
            }
        }
    });
    let url = source.as_ref().and_then(|s| s.web_url(&manifest.source.id));
    Ok(Info {
        manifest,
        origin,
        installed,
        url,
        described,
        latest,
    })
}
