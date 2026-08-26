//! Updating ketch with ketch.
//!
//! Deliberately stricter than a normal install: the running binary is the thing
//! that verifies every other download, so it is replaced only against a
//! published checksum — never on trust-on-first-use — and the previous binary is
//! kept until the new one has proven it can run.

use crate::config::Config;
use crate::error::Result;
use crate::model::Version;
use std::path::PathBuf;

/// Outcome of a self-update attempt.
#[derive(Debug, Clone)]
pub struct SelfUpdate {
    pub from: Version,
    pub to: Version,
    /// False when already current, or when `dry_run` was set.
    pub replaced: bool,
    pub notes: Option<String>,
}

/// The version this binary was built as.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION"))
}

/// Where the running binary lives, with symlinks resolved so we replace the
/// real file rather than the link pointing at it.
pub fn current_exe() -> Result<PathBuf> {
    todo!("resolve current exe")
}

/// Fetch the latest ketch release and replace this binary.
pub fn update(_cfg: &Config, _force: bool, _dry_run: bool) -> Result<SelfUpdate> {
    todo!("self update")
}

/// Remove ketch itself. With `purge`, also removes the store, cache and state.
pub fn uninstall_self(_cfg: &Config, _purge: bool) -> Result<Vec<PathBuf>> {
    todo!("self uninstall")
}
