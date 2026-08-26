//! Commands about ketch itself and its environment.

use crate::cli::{PluginCommand, SelfCommand};
use crate::config::Config;
use crate::error::Result;

pub fn doctor(_cfg: &Config) -> Result<()> {
    todo!("platform checks plus store/state consistency; exit non-zero on any failure")
}

pub fn plugin(_cfg: &Config, _command: PluginCommand) -> Result<()> {
    todo!("list discovered plugins with scheme, path and protocol version")
}

pub fn zelf(_cfg: &Config, _command: SelfCommand) -> Result<()> {
    todo!("update, version, uninstall")
}
