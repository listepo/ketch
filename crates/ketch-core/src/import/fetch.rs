// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Where `ketch import` reads the other catalogues from, behind one trait.
//!
//! Every converter is written against [`Fetch`], so a test hands it recorded
//! responses and never reaches the network. [`Endpoints`] holds each
//! catalogue's base URL; the environment can point any of them elsewhere —
//! a mirror, or a test's local server.

use crate::error::{Error, Result};
use crate::http::Http;
use crate::report::Ctx;

/// A GET that answers with the body, or `None` when the resource is absent.
pub trait Fetch {
    fn text(&self, url: &str) -> Result<Option<String>>;
}

/// Base URLs of the catalogues, with no trailing slash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// Homebrew's JSON API: `<brew>/formula/<name>.json`.
    pub brew: String,
    /// The GitHub contents API over `winget-pkgs/manifests`.
    pub winget_api: String,
    /// Raw files of `winget-pkgs/manifests`.
    pub winget_raw: String,
    /// archlinux.org, for the official repositories' JSON search.
    pub arch: String,
    /// Arch's packaging GitLab group, for an official package's `.SRCINFO`.
    pub arch_gitlab: String,
    /// The AUR: its RPC and its cgit.
    pub aur: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            brew: "https://formulae.brew.sh/api".into(),
            winget_api: "https://api.github.com/repos/microsoft/winget-pkgs/contents/manifests"
                .into(),
            winget_raw: "https://raw.githubusercontent.com/microsoft/winget-pkgs/master/manifests"
                .into(),
            arch: "https://archlinux.org".into(),
            arch_gitlab: "https://gitlab.archlinux.org/archlinux/packaging/packages".into(),
            aur: "https://aur.archlinux.org".into(),
        }
    }
}

impl Endpoints {
    /// The defaults, each overridden by its `KETCH_IMPORT_*` variable.
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let d = Endpoints::default();
        let pick = |key: &str, default: String| {
            get(key)
                .map(|v| v.trim().trim_end_matches('/').to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or(default)
        };
        Endpoints {
            brew: pick("KETCH_IMPORT_BREW", d.brew),
            winget_api: pick("KETCH_IMPORT_WINGET_API", d.winget_api),
            winget_raw: pick("KETCH_IMPORT_WINGET_RAW", d.winget_raw),
            arch: pick("KETCH_IMPORT_ARCH", d.arch),
            arch_gitlab: pick("KETCH_IMPORT_ARCH_GITLAB", d.arch_gitlab),
            aur: pick("KETCH_IMPORT_AUR", d.aur),
        }
    }
}

/// The real thing: anonymous everywhere except the GitHub contents API,
/// which gets the configured token because its anonymous rate limit is
/// sixty requests an hour.
pub struct HttpFetch {
    anonymous: Http,
    github: Http,
    github_api: String,
}

impl HttpFetch {
    pub fn new(cx: &Ctx<'_>, endpoints: &Endpoints) -> Self {
        HttpFetch {
            anonymous: Http::anonymous(cx.report),
            github: Http::new(cx),
            github_api: endpoints.winget_api.clone(),
        }
    }
}

impl Fetch for HttpFetch {
    fn text(&self, url: &str) -> Result<Option<String>> {
        let authed = url.starts_with(&self.github_api);
        let http = if authed {
            &self.github
        } else {
            &self.anonymous
        };
        match http.get_text(url, authed) {
            Ok(body) => Ok(Some(body)),
            Err(Error::Http { status: 404, .. }) => Ok(None),
            Err(other) => Err(other),
        }
    }
}

/// Recorded responses, for tests: a URL not in the map is a 404.
#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub(crate) struct Recorded {
    pub responses: std::collections::BTreeMap<String, String>,
}

#[cfg(test)]
impl Recorded {
    pub(crate) fn with(mut self, url: impl Into<String>, body: impl Into<String>) -> Self {
        self.responses.insert(url.into(), body.into());
        self
    }
}

#[cfg(test)]
impl Fetch for Recorded {
    fn text(&self, url: &str) -> Result<Option<String>> {
        Ok(self.responses.get(url).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_endpoint_follows_its_own_variable_and_drops_a_trailing_slash() {
        let e = Endpoints::from_lookup(|k| match k {
            "KETCH_IMPORT_BREW" => Some("http://127.0.0.1:9/brew/".into()),
            "KETCH_IMPORT_AUR" => Some("  ".into()),
            _ => None,
        });
        assert_eq!(e.brew, "http://127.0.0.1:9/brew");
        assert_eq!(e.aur, Endpoints::default().aur, "a blank value is no value");
        assert_eq!(e.winget_raw, Endpoints::default().winget_raw);
    }

    #[test]
    fn a_recorded_url_answers_and_any_other_is_absent() {
        let f = Recorded::default().with("u", "body");
        assert_eq!(f.text("u").unwrap().as_deref(), Some("body"));
        assert_eq!(f.text("v").unwrap(), None);
    }
}
