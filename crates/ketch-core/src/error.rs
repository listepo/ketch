// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Error type shared by every module.
//!
//! One enum keeps the plumbing honest: any module may construct any variant,
//! and `main` renders them uniformly. Variants carry the data needed to write a
//! message a user can act on — never a bare string where a path or URL exists.

use std::path::{Path, PathBuf};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),

    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{0}")]
    PlainIo(#[from] std::io::Error),

    #[error("HTTP {status} from {url}")]
    Http {
        url: String,
        status: u16,
        detail: Option<String>,
    },

    #[error("network error requesting {url}")]
    Network {
        url: String,
        #[source]
        source: Box<ureq::Error>,
    },

    #[error("could not parse {what}: {detail}")]
    Parse { what: String, detail: String },

    #[error("no source is registered for scheme `{0}`")]
    UnknownScheme(String),

    #[error("`{0}` is not installed")]
    NotInstalled(String),

    #[error("`{0}` has no retained version to roll back to")]
    NoRetained(String),

    #[error("`{name}` {version} is already installed")]
    AlreadyInstalled { name: String, version: String },

    /// `ketch install` of a package already installed without a version:
    /// the resolved release is the installed one.
    #[error("cannot install `{name}`: {version} is already installed and no update is available")]
    NoUpdate { name: String, version: String },

    /// `ketch install` of an installed package that has a newer release:
    /// the command asks before it updates.
    #[error("`{name}` {installed} is installed and {latest} is available")]
    UpdateAvailable {
        name: String,
        installed: String,
        latest: String,
        /// The release tag to update to, so the update installs exactly what
        /// the question named.
        tag: String,
    },

    #[error("`{name}` is pinned to {version}")]
    Pinned { name: String, version: String },

    #[error("no release found for `{0}`")]
    NoRelease(String),

    #[error("release `{tag}` of `{id}` has no asset for {target}")]
    NoCompatibleAsset {
        id: String,
        tag: String,
        target: String,
    },

    #[error("checksum mismatch for {name}")]
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },

    #[error("no published checksum for {0}")]
    ChecksumMissing(String),

    #[error("no installable files found in {0}")]
    EmptyPayload(PathBuf),

    #[error("unsupported archive format: {0}")]
    UnsupportedArchive(PathBuf),

    #[error("`{cmd}` failed ({status})")]
    Command {
        cmd: String,
        status: String,
        stderr: String,
    },

    #[error("plugin `{name}`: {detail}")]
    Plugin {
        name: String,
        detail: String,
        stderr: String,
    },

    #[error("{0}")]
    Config(String),

    /// The command has already told the user everything, line by line, and
    /// only the exit code is left: `main` prints nothing more for it.
    #[error("")]
    Reported(i32),

    /// Another operation holds the install tree's lock: a ketch process named
    /// by `pid`, or (when the lock file could not be read) just `lock`.
    #[error("another ketch process holds the lock ({})", busy_holder(*pid, lock))]
    Busy { pid: Option<u32>, lock: PathBuf },

    /// The caller's `Cancel` token fired; nothing was left half-installed.
    #[error("cancelled")]
    Cancelled,
}

/// The parenthesised part of the busy message: the pid when known, else the
/// lock file, exactly as the message read before `Busy` was typed.
fn busy_holder(pid: Option<u32>, lock: &Path) -> String {
    match pid {
        Some(pid) => format!("pid {pid}"),
        None => lock.display().to_string(),
    }
}

impl Error {
    pub fn msg(text: impl Into<String>) -> Self {
        Error::Msg(text.into())
    }

    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn parse(what: impl Into<String>, detail: impl Into<String>) -> Self {
        Error::Parse {
            what: what.into(),
            detail: detail.into(),
        }
    }

    /// Extra lines shown under the headline message. Keeps the `Display` impl
    /// short while still surfacing server bodies, diffs and stderr.
    pub fn details(&self) -> Vec<String> {
        match self {
            Error::Http { detail, .. } => detail.iter().cloned().collect(),
            Error::Command { stderr, .. } | Error::Plugin { stderr, .. }
                if !stderr.trim().is_empty() =>
            {
                stderr.trim().lines().map(|l| l.to_string()).collect()
            }
            Error::ChecksumMismatch {
                expected, actual, ..
            } => vec![format!("expected {expected}"), format!("actual   {actual}")],
            _ => Vec::new(),
        }
    }

    /// A short, actionable next step, when one exists.
    pub fn hint(&self) -> Option<String> {
        match self {
            Error::Http { status: 403, .. } | Error::Http { status: 429, .. } => Some(
                "GitHub rate limit. Set GITHUB_TOKEN (or `gh auth token`) to raise it."
                    .to_string(),
            ),
            Error::Http { status: 404, .. } => {
                Some("Check the owner/repo spelling, or the repo may be private.".to_string())
            }
            Error::NoCompatibleAsset { .. } => Some(
                "Run `ketch info <pkg>` to list assets, then pin one with `asset.include` in a manifest."
                    .to_string(),
            ),
            Error::ChecksumMismatch { .. } => {
                Some("Refusing to install. Re-run to retry the download.".to_string())
            }
            Error::AlreadyInstalled { .. } | Error::NoUpdate { .. } => {
                Some("Use --force to reinstall.".to_string())
            }
            Error::UpdateAvailable { name, .. } => Some(format!(
                "Pass --yes to update it, or run `ketch upgrade {name}`."
            )),
            Error::Pinned { .. } => Some("Run `ketch unpin <pkg>` first.".to_string()),
            Error::NoRetained(_) => Some(
                "Upgrade keeps the previous prefix until `ketch prune`.".to_string(),
            ),
            Error::UnknownScheme(s) => Some(format!(
                "Install a source plugin named `ketch-source-{s}` on PATH or in the plugins dir."
            )),
            _ => None,
        }
    }

    /// Process exit code. Distinct codes let scripts branch on failure class.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotInstalled(_) | Error::NoRelease(_) | Error::NoRetained(_) => 4,
            Error::AlreadyInstalled { .. }
            | Error::NoUpdate { .. }
            | Error::UpdateAvailable { .. }
            | Error::Pinned { .. } => 5,
            Error::ChecksumMismatch { .. } | Error::ChecksumMissing(_) => 6,
            Error::Http { .. } | Error::Network { .. } => 7,
            Error::Busy { .. } => 8,
            Error::Reported(code) => *code,
            // The shell convention for "interrupted", as the TUI already uses.
            Error::Cancelled => 130,
            _ => 1,
        }
    }
}
