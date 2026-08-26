//! External source plugins.
//!
//! A plugin is any executable named `ketch-source-<scheme>` found in the
//! plugins directory or on PATH. ketch invokes it with a subcommand and reads
//! one JSON document from stdout. That is the entire contract — plugins can be
//! written in any language and need no ketch release to ship.
//!
//! The protocol is specified in `docs/PLUGINS.md`; `PROTOCOL_VERSION` is what
//! this build speaks.

use super::{ListOpts, Source};
use crate::config::Config;
use crate::error::Result;
use crate::model::{Release, ReleaseAsset, SourceInfo};
use crate::ui::ProgressSink;
use std::path::{Path, PathBuf};

pub const PROTOCOL_VERSION: u32 = 1;

/// Executable prefix a plugin must use to be discovered.
pub const PLUGIN_PREFIX: &str = "ketch-source-";

/// A discovered plugin executable.
pub struct PluginSource {
    _path: PathBuf,
    _scheme: String,
}

impl PluginSource {
    /// Interrogate an executable and adopt it if it speaks a version we know.
    pub fn probe(_path: &Path) -> Result<Self> {
        todo!("run `<plugin> capabilities` and validate the response")
    }

    /// File name of the executable, for diagnostics.
    pub fn name(&self) -> &str {
        todo!("file name")
    }

    pub fn path(&self) -> &Path {
        todo!("path")
    }
}

impl Source for PluginSource {
    fn scheme(&self) -> &str {
        todo!("scheme reported by the plugin")
    }

    fn describe(&self, _id: &str) -> Result<Option<SourceInfo>> {
        todo!("`describe <id>`")
    }

    fn list_releases(&self, _id: &str, _opts: &ListOpts) -> Result<Vec<Release>> {
        todo!("`releases <id>`")
    }

    fn download(
        &self,
        _asset: &ReleaseAsset,
        _dest: &Path,
        _progress: &dyn ProgressSink,
    ) -> Result<String> {
        todo!("fetch over HTTP, or delegate to `download` when the plugin offers it")
    }

    fn search(&self, _query: &str, _limit: usize) -> Result<Vec<SourceInfo>> {
        todo!("`search <query>`")
    }
}

/// Find every plugin available to this run.
///
/// Returns one entry per candidate so a single broken plugin can be reported
/// without hiding the ones that work.
pub fn discover(_cfg: &Config) -> Vec<Result<PluginSource>> {
    todo!("scan the plugins dir then PATH for `ketch-source-*`")
}
