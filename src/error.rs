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

    #[error("`{name}` {version} is already installed")]
    AlreadyInstalled { name: String, version: String },

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
    Plugin { name: String, detail: String },

    #[error("{0}")]
    Config(String),

    #[error("another ketch process holds the lock ({0})")]
    Locked(String),
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
            Error::Command { stderr, .. } if !stderr.trim().is_empty() => {
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
            Error::AlreadyInstalled { .. } => Some("Use --force to reinstall.".to_string()),
            Error::Pinned { .. } => Some("Run `ketch unpin <pkg>` first.".to_string()),
            Error::UnknownScheme(s) => Some(format!(
                "Install a source plugin named `ketch-source-{s}` on PATH or in the plugins dir."
            )),
            _ => None,
        }
    }

    /// Process exit code. Distinct codes let scripts branch on failure class.
    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NotInstalled(_) | Error::NoRelease(_) => 4,
            Error::AlreadyInstalled { .. } | Error::Pinned { .. } => 5,
            Error::ChecksumMismatch { .. } | Error::ChecksumMissing(_) => 6,
            Error::Http { .. } | Error::Network { .. } => 7,
            Error::Locked(_) => 8,
            _ => 1,
        }
    }
}
