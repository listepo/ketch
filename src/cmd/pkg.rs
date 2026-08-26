//! Commands that change what is installed.
//!
//! All of these take the state lock for the whole operation and save
//! `state.json` once at the end, so a multi-package run is one write.

use crate::cli::{InstallArgs, NameArgs, UninstallArgs, UpgradeArgs};
use crate::config::Config;
use crate::error::Result;

pub fn install(_cfg: &Config, _args: InstallArgs) -> Result<()> {
    todo!("lock, resolve each spec, install, report, save state once")
}

pub fn uninstall(_cfg: &Config, _args: UninstallArgs) -> Result<()> {
    todo!("lock, confirm unless --yes, remove each, save state once")
}

pub fn upgrade(_cfg: &Config, _args: UpgradeArgs) -> Result<()> {
    todo!("resolve latest for each candidate, skip pinned unless --force, honour --dry-run")
}

/// `pin` and `unpin` — `pinned` selects which.
pub fn pin(_cfg: &Config, _args: NameArgs, _pinned: bool) -> Result<()> {
    todo!("flip the pinned flag and save")
}

/// `link` and `unlink` — `linked` selects which.
pub fn link(_cfg: &Config, _args: NameArgs, _linked: bool) -> Result<()> {
    todo!("create or remove links for already-installed packages")
}
