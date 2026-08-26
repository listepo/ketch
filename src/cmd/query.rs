//! Read-only commands.
//!
//! These never take the lock and never write. Their data output goes to stdout
//! through `ui::out`/`ui::table` so it can be piped while progress and warnings
//! stay on stderr.

use crate::cli::{InfoArgs, ListArgs, OutdatedArgs, SearchArgs};
use crate::config::Config;
use crate::error::Result;

pub fn list(_cfg: &Config, _args: ListArgs) -> Result<()> {
    todo!("name, version, source, pinned marker; --json and --names-only")
}

pub fn outdated(_cfg: &Config, _args: OutdatedArgs) -> Result<()> {
    todo!("compare each installed package against its source's latest release")
}

pub fn info(_cfg: &Config, _args: InfoArgs) -> Result<()> {
    todo!("installed details when present, plus live release info; --assets shows scoring")
}

pub fn search(_cfg: &Config, _args: SearchArgs) -> Result<()> {
    todo!("built-in registry hits first, then source search results")
}
