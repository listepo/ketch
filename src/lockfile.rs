//! `ketch.lock`: one machine's set of tools, pinned to exact releases.
//!
//! Not the lock in `state.rs` — that one is a mutex over the install tree, held
//! for the length of a command. This is a file the user commits next to their
//! dotfiles. `ketch lock` writes it from what is installed, `ketch sync` makes
//! another machine match, and `ketch lock --check` says whether the two have
//! drifted apart.
//!
//! What is reproducible here, and what is not:
//!
//! * The **tag** is. Every machine resolving the same tag gets the same
//!   release, which is the whole point of writing one down.
//! * The **asset and its hash** are reproducible only on the same target. A
//!   lock written on Apple Silicon names an `aarch64` tarball an Intel machine
//!   cannot run, so `sync` re-selects the asset there and verifies against the
//!   source's own checksum instead. Claiming the recorded hash still applied
//!   would be a reproducibility guarantee that quietly is not one.
//!
//! A lockfile is a file somebody else may have written — that is what sharing
//! a dotfiles repository means — so nothing in it is allowed to choose a
//! filesystem path. `sync` asks for a source at a tag and lets the usual
//! manifest resolution decide the install name, the binaries, and where they
//! land. The lock pins *which release*, never *where it goes*.

use crate::config::{sanitize_component, validate_repo};
use crate::error::{Error, Result};
use crate::model::{InstalledPackage, PackageRef};
use crate::state::State;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Bumped only when the on-disk shape changes incompatibly.
pub const LOCK_VERSION: u32 = 1;

/// The name looked for when `--file` is not given.
pub const LOCK_FILE: &str = "ketch.lock";

/// A whole lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lockfile {
    pub version: u32,
    /// Sorted by name, so the file is stable and its diffs are readable.
    /// Omitted entirely when empty: `package = []` is not what "nothing is
    /// installed" should look like in a file people read.
    #[serde(default, rename = "package", skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<LockedPackage>,
}

/// One package, pinned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedPackage {
    /// The name it was installed under. Used to find the same manifest again,
    /// never to build a path.
    pub name: String,
    pub source: PackageRef,
    /// Human-readable version. `tag` is what actually gets resolved.
    pub version: String,
    pub tag: String,
    /// The target this entry was captured on, as `<os>-<arch>`.
    pub target: String,
    pub asset: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
}

impl LockedPackage {
    fn from_installed(pkg: &InstalledPackage) -> LockedPackage {
        LockedPackage {
            name: pkg.name.clone(),
            source: pkg.source.clone(),
            version: pkg.version.to_string(),
            tag: pkg.tag.clone(),
            target: pkg.target.to_string(),
            asset: pkg.asset_name.clone(),
            sha256: pkg.sha256.clone(),
            pinned: pkg.pinned,
        }
    }

    /// True when the recorded asset and hash describe a machine like this one.
    ///
    /// Only then is the hash something to hold a download to: on a different
    /// target the asset is a different file, and a mismatch would be correct
    /// rather than suspicious.
    pub fn matches_target(&self, target: &str) -> bool {
        self.target == target
    }
}

impl Lockfile {
    /// Capture what is installed right now.
    pub fn from_state(state: &State) -> Lockfile {
        let mut packages: Vec<LockedPackage> =
            state.iter().map(LockedPackage::from_installed).collect();
        packages.sort_by(|a, b| a.name.cmp(&b.name));
        Lockfile {
            version: LOCK_VERSION,
            packages,
        }
    }

    /// Read and check a lockfile.
    ///
    /// A missing one is not an empty one: `sync` with nothing to sync from is
    /// a mistake worth naming, not a no-op to report as success.
    pub fn load(path: &Path) -> Result<Lockfile> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::msg(format!(
                    "no lockfile at {}. Run `ketch lock` to write one, or pass \
                     --file to point at another.",
                    path.display()
                )))
            }
            Err(e) => return Err(Error::io(path, e)),
        };
        let lock: Lockfile = toml::from_str(&text)
            .map_err(|e| Error::parse(path.display().to_string(), e.to_string()))?;
        lock.validate(path)?;
        Ok(lock)
    }

    /// Every check that has to pass before a single entry is acted on.
    ///
    /// One bad entry fails the file rather than being skipped: unlike the
    /// registry, where a partial answer beats none, a lockfile that installed
    /// most of itself would not be a lock at all.
    fn validate(&self, path: &Path) -> Result<()> {
        let where_ = path.display();
        if self.version > LOCK_VERSION {
            return Err(Error::msg(format!(
                "{where_} was written by a newer ketch (lock version {}); \
                 upgrade with `ketch self update`",
                self.version
            )));
        }
        let mut seen = BTreeSet::new();
        for pkg in &self.packages {
            let named = format!("{where_}: package `{}`", pkg.name);
            if pkg.name.trim().is_empty() {
                return Err(Error::msg(format!("{where_}: a package has no name")));
            }
            // The name is matched against installed packages and shown to the
            // user. A name that would have to be rewritten to be usable is a
            // name that does not mean what it says.
            if sanitize_component(&pkg.name) != pkg.name {
                return Err(Error::msg(format!("{named} is not a usable package name")));
            }
            if !seen.insert(pkg.name.to_ascii_lowercase()) {
                return Err(Error::msg(format!("{named} is listed twice")));
            }
            if pkg.tag.trim().is_empty() {
                return Err(Error::msg(format!("{named} has no tag to resolve")));
            }
            if pkg.source.scheme == "github" {
                validate_repo("source", pkg.source.id.clone())?;
            } else if pkg.source.id.trim().is_empty() {
                return Err(Error::msg(format!("{named} has no source id")));
            }
            if !is_sha256(&pkg.sha256) {
                return Err(Error::msg(format!(
                    "{named} has `{}` where a sha256 belongs",
                    pkg.sha256
                )));
            }
        }
        Ok(())
    }

    /// The file as it will be written.
    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string_pretty(self)
            .map_err(|e| Error::parse(LOCK_FILE.to_string(), e.to_string()))?;
        Ok(format!(
            "# {LOCK_FILE} — written by `ketch lock`. Commit it.\n\
             #\n\
             # `ketch sync` installs every package below at the tag named here.\n\
             # The asset and sha256 apply to `target`; on any other machine ketch\n\
             # picks the asset that fits and verifies it against the source.\n\
             {body}"
        ))
    }

    /// Write it out, atomically, so an interrupted write cannot leave a
    /// half-parsed lockfile where a whole one used to be.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = self.to_toml()?;
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
        let parent = parent.unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        let mut staged =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;
        staged
            .write_all(text.as_bytes())
            .map_err(|e| Error::io(staged.path(), e))?;
        staged.flush().map_err(|e| Error::io(staged.path(), e))?;
        staged
            .as_file()
            .sync_all()
            .map_err(|e| Error::io(staged.path(), e))?;
        staged.persist(path).map_err(|e| Error::io(path, e.error))?;
        Ok(())
    }

    /// The entry for an installed package, matched on source rather than name.
    ///
    /// The source is the stable identity: a package can be renamed upstream, or
    /// installed under an alias, and still be the same thing to update.
    fn find(&self, pkg: &InstalledPackage) -> Option<&LockedPackage> {
        self.packages.iter().find(|l| l.source == pkg.source)
    }
}

/// What `sync` would do, and what `--check` reports.
#[derive(Debug, Default)]
pub struct Plan {
    /// Not installed at all.
    pub missing: Vec<LockedPackage>,
    /// Installed, at a different tag. Carries the tag actually present.
    pub changed: Vec<(LockedPackage, String)>,
    /// Installed, and the lockfile says nothing about it.
    pub extra: Vec<String>,
    /// Installed at exactly the locked tag.
    pub matched: usize,
}

impl Plan {
    /// True when the tree already is what the lockfile describes. `--prune`
    /// decides whether extras count against that, because a lockfile is a
    /// record of what you want, not necessarily of all you have.
    pub fn is_clean(&self, including_extras: bool) -> bool {
        self.missing.is_empty()
            && self.changed.is_empty()
            && (!including_extras || self.extra.is_empty())
    }
}

/// Compare a lockfile against what is installed.
pub fn plan(lock: &Lockfile, state: &State) -> Plan {
    let mut plan = Plan::default();
    for entry in &lock.packages {
        match state.iter().find(|p| p.source == entry.source) {
            None => plan.missing.push(entry.clone()),
            Some(installed) if installed.tag == entry.tag => plan.matched += 1,
            Some(installed) => plan.changed.push((entry.clone(), installed.tag.clone())),
        }
    }
    for installed in state.iter() {
        if lock.find(installed).is_none() {
            plan.extra.push(installed.name.clone());
        }
    }
    plan
}

/// Where the lockfile lives: what was asked for, or `ketch.lock` here.
pub fn path(explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(LOCK_FILE))
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ManifestOrigin, TargetSpec, Version};

    fn installed(name: &str, repo: &str, tag: &str) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            version: Version::parse(tag.trim_start_matches('v')),
            source: PackageRef::github(repo),
            tag: tag.to_string(),
            target: TargetSpec::host(),
            asset_name: format!("{name}.tar.gz"),
            sha256: "a".repeat(64),
            checksum_verified: true,
            installed_at: 0,
            prefix: PathBuf::from("/store").join(name),
            links: Vec::new(),
            pinned: false,
            origin: ManifestOrigin::Inferred,
            manifest: None,
        }
    }

    fn state_with(packages: Vec<InstalledPackage>) -> State {
        let mut state = State::default();
        for pkg in packages {
            state.insert(pkg);
        }
        state
    }

    fn lock_of(state: &State) -> Lockfile {
        Lockfile::from_state(state)
    }

    #[test]
    fn a_lockfile_round_trips_through_toml() {
        let state = state_with(vec![
            installed("ripgrep", "BurntSushi/ripgrep", "14.1.1"),
            installed("fd", "sharkdp/fd", "v10.2.0"),
        ]);
        let text = lock_of(&state).to_toml().expect("render");
        let parsed: Lockfile = toml::from_str(&text).expect("parse");
        assert_eq!(parsed.packages, lock_of(&state).packages);
    }

    #[test]
    fn packages_are_written_in_a_stable_order() {
        let state = state_with(vec![
            installed("ripgrep", "BurntSushi/ripgrep", "14.1.1"),
            installed("fd", "sharkdp/fd", "v10.2.0"),
            installed("jq", "jqlang/jq", "jq-1.7"),
        ]);
        let lock = lock_of(&state);
        let names: Vec<&str> = lock.packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["fd", "jq", "ripgrep"]);
    }

    #[test]
    fn a_tree_that_matches_its_lockfile_is_clean() {
        let state = state_with(vec![installed("fd", "sharkdp/fd", "v10.2.0")]);
        let plan = plan(&lock_of(&state), &state);
        assert!(plan.is_clean(true));
        assert_eq!(plan.matched, 1);
    }

    #[test]
    fn a_different_tag_is_drift_not_a_missing_package() {
        let state = state_with(vec![installed("fd", "sharkdp/fd", "v10.2.0")]);
        let lock = lock_of(&state);
        let moved = state_with(vec![installed("fd", "sharkdp/fd", "v10.3.0")]);
        let plan = plan(&lock, &moved);
        assert!(plan.missing.is_empty());
        assert_eq!(plan.changed.len(), 1);
        assert_eq!(plan.changed[0].1, "v10.3.0");
    }

    #[test]
    fn a_package_renamed_upstream_is_still_the_same_package() {
        let state = state_with(vec![installed("fd", "sharkdp/fd", "v10.2.0")]);
        let lock = lock_of(&state);
        // Same source, installed under a different name.
        let renamed = state_with(vec![installed("fd-find", "sharkdp/fd", "v10.2.0")]);
        let plan = plan(&lock, &renamed);
        assert_eq!(plan.matched, 1);
        assert!(plan.extra.is_empty(), "{:?}", plan.extra);
    }

    #[test]
    fn something_installed_that_the_lockfile_omits_is_extra() {
        let lock = lock_of(&state_with(vec![installed("fd", "sharkdp/fd", "v10.2.0")]));
        let more = state_with(vec![
            installed("fd", "sharkdp/fd", "v10.2.0"),
            installed("jq", "jqlang/jq", "jq-1.7"),
        ]);
        let plan = plan(&lock, &more);
        assert_eq!(plan.extra, ["jq"]);
        assert!(plan.is_clean(false), "extras alone are not drift");
        assert!(!plan.is_clean(true), "with --prune they are");
    }

    #[test]
    fn the_pinned_flag_survives_a_round_trip() {
        let mut pkg = installed("fd", "sharkdp/fd", "v10.2.0");
        pkg.pinned = true;
        let text = lock_of(&state_with(vec![pkg])).to_toml().expect("render");
        let parsed: Lockfile = toml::from_str(&text).expect("parse");
        assert!(parsed.packages[0].pinned);
    }

    fn parse_checked(text: &str) -> Result<Lockfile> {
        let lock: Lockfile =
            toml::from_str(text).map_err(|e| Error::parse("test".to_string(), e.to_string()))?;
        lock.validate(Path::new("ketch.lock"))?;
        Ok(lock)
    }

    fn entry(overrides: &str) -> String {
        format!(
            "version = 1\n\n[[package]]\nname = \"fd\"\nsource = \"github:sharkdp/fd\"\n\
             version = \"10.2.0\"\ntag = \"v10.2.0\"\ntarget = \"macos-aarch64\"\n\
             asset = \"fd.tar.gz\"\nsha256 = \"{}\"\n{overrides}",
            "a".repeat(64)
        )
    }

    #[test]
    fn a_well_formed_lockfile_passes() {
        assert!(parse_checked(&entry("")).is_ok());
    }

    #[test]
    fn a_name_that_would_escape_the_store_is_refused() {
        let text = entry("").replace("name = \"fd\"", "name = \"../../.zshrc\"");
        assert!(parse_checked(&text).is_err());
    }

    #[test]
    fn a_source_that_is_not_a_repo_is_refused() {
        let text = entry("").replace("github:sharkdp/fd", "github:../../etc");
        assert!(parse_checked(&text).is_err());
    }

    #[test]
    fn a_hash_that_is_not_a_hash_is_refused() {
        let text = entry("").replace(&"a".repeat(64), "not-a-hash");
        assert!(parse_checked(&text).is_err());
    }

    #[test]
    fn a_package_listed_twice_is_refused() {
        let mut text = entry("");
        let second = text.clone();
        text.push_str(
            second
                .split_once("\n\n")
                .map(|(_, rest)| rest)
                .unwrap_or(""),
        );
        assert!(parse_checked(&text).is_err());
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        assert!(parse_checked(&entry("surprise = true\n")).is_err());
    }

    #[test]
    fn a_lockfile_from_a_newer_ketch_is_refused() {
        let text = entry("").replace("version = 1", "version = 99");
        assert!(parse_checked(&text).is_err());
    }

    #[test]
    fn an_empty_lockfile_is_valid_and_means_nothing_installed() {
        let lock = parse_checked("version = 1\n").expect("parse");
        assert!(lock.packages.is_empty());
    }

    #[test]
    fn the_recorded_hash_only_applies_to_the_machine_that_wrote_it() {
        let lock = parse_checked(&entry("")).expect("parse");
        assert!(lock.packages[0].matches_target("macos-aarch64"));
        assert!(!lock.packages[0].matches_target("macos-x86_64"));
    }
}
