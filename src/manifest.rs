//! Turning what the user typed into a `Manifest`.
//!
//! Three tiers, in order: a user manifest in `~/.ketch/manifests/<name>.toml`,
//! the built-in registry compiled into the binary, then inference from the
//! source reference itself. Inference is what lets `ketch install owner/repo`
//! work for a repository nobody has ever curated.

use crate::config::Config;
use crate::error::Result;
use crate::model::{Manifest, ManifestOrigin, PackageSpec};

/// The built-in registry: curated manifests for tools whose release layout
/// needs a hint that inference cannot guess.
pub const BUILTIN_TOML: &str = include_str!("builtin.toml");

/// Resolves specs to manifests. Built once per command.
pub struct Resolver {
    _private: (),
}

impl Resolver {
    pub fn new(_cfg: &Config) -> Result<Self> {
        todo!("load builtin registry and user manifests")
    }

    /// Resolve a spec, reporting where the manifest came from.
    pub fn resolve(&self, _spec: &PackageSpec) -> Result<(Manifest, ManifestOrigin)> {
        todo!("resolve manifest")
    }

    /// Every alias the registry knows, for completion and `ketch search`.
    pub fn aliases(&self) -> Vec<&str> {
        todo!("aliases")
    }

    /// Built-in entries matching a free-text query.
    pub fn search(&self, _query: &str) -> Vec<&Manifest> {
        todo!("search builtin registry")
    }
}
