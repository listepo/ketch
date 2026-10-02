// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! What `ketch list` shows: installed packages, the registry, and both merged.
//!
//! Separate from `cmd/query.rs` because the merge has rules of its own — a
//! pinned package is never an update, a source that cannot be reached is a `?`
//! rather than a failed command, one release lookup serves every row that
//! shares a source — and those rules want unit tests with no binary in the way.
//!
//! The lookup itself is not new: it is `install::latest_of`, the call behind
//! `ketch outdated`, with the prerelease rules `install` and `resolve` already
//! apply. What this module adds is running it for many packages at once,
//! remembering the answers for a few minutes, and deciding what a row says.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::install;
use crate::manifest::same_source;
use crate::model::{normalize_name, now_unix, InstalledPackage, Manifest, PackageRef, Version};
use crate::report::{Ctx, Report};
use crate::source::{ListOpts, SourceRegistry};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// How long a looked-up `latest` is reused, in seconds.
///
/// Long enough that listing twice in a row, or piping one listing into
/// another command, does not spend the GitHub rate limit twice; short enough
/// that a release published over lunch shows up after it.
pub const CACHE_TTL_SECS: u64 = 10 * 60;

/// The cache file, under the ketch cache directory.
const CACHE_FILE: &str = "latest.json";

/// What the `installed` column says after the version when the update is held.
pub const PINNED: &str = "(pinned)";

/// What the `latest` column says after the version when it is an update.
pub const UPDATE: &str = "(update available)";

/// What the `latest` column says when the source could not be reached.
pub const UNKNOWN: &str = "?";

/// An installed package, as much of it as a listing needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Local {
    pub name: String,
    pub source: PackageRef,
    pub version: Version,
    pub tag: String,
    pub pinned: bool,
    pub retained: Vec<Version>,
    /// Whether `latest` may be a prerelease, decided as `ketch outdated` does.
    pub prerelease: bool,
}

impl Local {
    pub fn from_installed(cfg: &Config, pkg: &InstalledPackage) -> Local {
        Local {
            name: pkg.name.clone(),
            source: pkg.source.clone(),
            version: pkg.version.clone(),
            tag: pkg.tag.clone(),
            pinned: pkg.pinned,
            retained: pkg.retained.iter().map(|r| r.version.clone()).collect(),
            prerelease: install::installed_list_opts(pkg, cfg.prerelease).include_prerelease,
        }
    }

    /// The `installed` cell: the version and the notes `ketch list local` has
    /// always carried.
    pub fn cell(&self) -> String {
        let mut cell = self.version.to_string();
        if self.pinned {
            cell.push(' ');
            cell.push_str(PINNED);
        }
        if !self.retained.is_empty() {
            cell.push_str(&format!(" (+{} retained)", self.retained.len()));
        }
        cell
    }
}

/// A package the registry offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Available {
    pub name: String,
    pub source: PackageRef,
    pub description: Option<String>,
    /// Whether `latest` may be a prerelease, decided as install does.
    pub prerelease: bool,
}

impl Available {
    pub fn from_manifest(cfg: &Config, manifest: &Manifest) -> Available {
        Available {
            name: manifest.name.clone(),
            source: manifest.source.clone(),
            description: manifest.description.clone(),
            prerelease: crate::resolve::list_opts(cfg, manifest, false).include_prerelease,
        }
    }
}

/// The newest release a lookup found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub version: Version,
    pub tag: String,
}

/// The `latest` of one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Latest {
    /// Not looked up: a package installed from a local path has no upstream,
    /// and a listing that needs no versions never asks.
    NotChecked,
    Found(Found),
    /// The source was asked and did not answer.
    Unreachable,
}

/// One source asked for its newest release, with or without prereleases.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Lookup {
    pub source: PackageRef,
    pub prerelease: bool,
}

impl Lookup {
    /// The cache key. The prerelease flag is part of it: the same repository
    /// has a different `latest` for someone who opted into prereleases.
    fn key(&self) -> String {
        let channel = if self.prerelease { "pre" } else { "stable" };
        format!("{}#{channel}", self.source)
    }
}

/// One row of a listing: installed, available, or both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub local: Option<Local>,
    pub available: Option<Available>,
    pub latest: Latest,
}

impl Row {
    fn installed(local: Local) -> Row {
        Row {
            name: local.name.clone(),
            local: Some(local),
            available: None,
            latest: Latest::NotChecked,
        }
    }

    pub fn offered(available: Available) -> Row {
        Row {
            name: available.name.clone(),
            local: None,
            available: Some(available),
            latest: Latest::NotChecked,
        }
    }

    /// Where the package comes from: what it was installed from when it is
    /// installed, since that is where its next version will come from too.
    pub fn source(&self) -> Option<&PackageRef> {
        self.local
            .as_ref()
            .map(|l| &l.source)
            .or_else(|| self.available.as_ref().map(|a| &a.source))
    }

    /// The lookup that answers this row's `latest`, if it has one.
    pub fn lookup(&self) -> Option<Lookup> {
        let (source, prerelease) = match (&self.local, &self.available) {
            (Some(l), _) => (&l.source, l.prerelease),
            (None, Some(a)) => (&a.source, a.prerelease),
            (None, None) => return None,
        };
        // A local path has no release stream, and its synthetic tag is not an
        // upstream to compare against: `ketch outdated` skips it for the same
        // reason.
        (source.scheme != "local").then(|| Lookup {
            source: source.clone(),
            prerelease,
        })
    }

    pub fn update_available(&self) -> bool {
        match (&self.local, &self.latest) {
            (Some(local), Latest::Found(found)) => update_available(local, found),
            _ => false,
        }
    }

    /// The `latest` cell: a version, a version marked as an update, `?`, or
    /// nothing when there was nothing to ask.
    pub fn latest_cell(&self) -> String {
        match &self.latest {
            Latest::Found(found) if self.update_available() => {
                format!("{} {UPDATE}", found.version)
            }
            Latest::Found(found) => found.version.to_string(),
            Latest::Unreachable => UNKNOWN.to_string(),
            Latest::NotChecked => String::new(),
        }
    }

    pub fn latest_version(&self) -> Option<String> {
        match &self.latest {
            Latest::Found(found) => Some(found.version.to_string()),
            _ => None,
        }
    }
}

/// Whether `found` is an update to offer for `local`.
///
/// The rules `ketch outdated` and `ketch upgrade` use: a pinned package is
/// never offered one, a source still reporting the installed tag is current
/// however its version string parses, and only a strictly newer version
/// counts. A prerelease counts only when the package's lookup allowed
/// prereleases — a source or plugin that returns one it was not asked for
/// must not talk a stable install into it.
pub fn update_available(local: &Local, found: &Found) -> bool {
    if local.pinned || found.tag == local.tag {
        return false;
    }
    if found.version.is_prerelease() && !local.prerelease {
        return false;
    }
    found.version > local.version
}

/// Installed and available packages as one list, one row per package, sorted
/// by name.
///
/// An installed package and a registry entry are the same package when their
/// names match, or failing that when they name the same source — installing
/// `BurntSushi/ripgrep` records the curated name, but a user manifest may
/// have renamed it since.
pub fn merge(locals: Vec<Local>, available: Vec<Available>) -> Vec<Row> {
    let mut rows: Vec<Row> = locals.into_iter().map(Row::installed).collect();
    for offer in available {
        let wanted = normalize_name(&offer.name);
        let free = |row: &Row| row.available.is_none() && row.local.is_some();
        let by_name = rows.iter().position(|row| {
            free(row)
                && row
                    .local
                    .as_ref()
                    .is_some_and(|l| normalize_name(&l.name) == wanted)
        });
        let by_source = || {
            rows.iter().position(|row| {
                free(row)
                    && row
                        .local
                        .as_ref()
                        .is_some_and(|l| same_source(&l.source, &offer.source))
            })
        };
        match by_name.or_else(by_source) {
            Some(i) => rows[i].available = Some(offer),
            None => rows.push(Row::offered(offer)),
        }
    }
    sort(&mut rows);
    rows
}

/// Sort rows the way every listing prints them: by name, case-insensitively.
pub fn sort(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        normalize_name(&a.name)
            .cmp(&normalize_name(&b.name))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// How the lookups went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Distinct sources that needed a `latest`.
    pub asked: usize,
    /// Of those, how many have one, fresh from the source or from the cache.
    pub answered: usize,
}

impl Outcome {
    /// Nothing answered at all: the network, not one package, is missing.
    pub fn offline(&self) -> bool {
        self.asked > 0 && self.answered == 0
    }
}

/// Fill in `latest` for every row that has an upstream.
///
/// Each distinct source is asked once, `cfg.jobs` at a time, and a fresh
/// cached answer is used instead of asking. A source that fails leaves its
/// rows `Unreachable`; nothing here returns an error, because one missing
/// version must not hide every other row.
pub fn fill_latest(cx: &Ctx<'_>, sources: &SourceRegistry, rows: &mut [Row]) -> Outcome {
    let cfg = cx.cfg;
    let mut lookups: BTreeMap<String, Lookup> = BTreeMap::new();
    for row in rows.iter() {
        if let Some(lookup) = row.lookup() {
            lookups.entry(lookup.key()).or_insert(lookup);
        }
    }
    if lookups.is_empty() {
        return Outcome::default();
    }

    let path = cache_path(cfg);
    let now = now_unix();
    let mut cache = Cache::load(&path, cx.report);
    cache.expire(now);

    let mut answers: HashMap<String, Found> = HashMap::new();
    let mut stale: Vec<(&String, &Lookup)> = Vec::new();
    for (key, lookup) in &lookups {
        match cache.entries.get(key) {
            Some(entry) => {
                answers.insert(key.clone(), entry.found());
            }
            None => stale.push((key, lookup)),
        }
    }

    let fetched = ask_all(cx, sources, &stale);
    let mut changed = false;
    for (key, result) in fetched {
        match result {
            Ok(found) => {
                cache
                    .entries
                    .insert(key.clone(), CacheEntry::new(&found, now));
                answers.insert(key, found);
                changed = true;
            }
            Err(e) => cx.report.debug(&format!("latest for {key}: {e}")),
        }
    }
    if changed {
        cache.save(&path, cx.report);
    }

    for row in rows.iter_mut() {
        if let Some(lookup) = row.lookup() {
            row.latest = match answers.get(&lookup.key()) {
                Some(found) => Latest::Found(found.clone()),
                None => Latest::Unreachable,
            };
        }
    }
    Outcome {
        asked: lookups.len(),
        answered: answers.len(),
    }
}

/// The lookups that the cache could not answer, in parallel, with a counter
/// on stderr while they run.
fn ask_all(
    cx: &Ctx<'_>,
    sources: &SourceRegistry,
    stale: &[(&String, &Lookup)],
) -> Vec<(String, Result<Found>)> {
    if stale.is_empty() {
        return Vec::new();
    }
    let counter = cx
        .report
        .counter("checking", stale.len() as u64, "packages");
    let jobs = cx.cfg.jobs.clamp(1, stale.len());
    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((key, lookup)) = stale.get(i) else {
                    return;
                };
                let opts = ListOpts {
                    include_prerelease: lookup.prerelease,
                    ..Default::default()
                };
                let result = install::latest_of(sources, &lookup.source, &opts).map(|r| Found {
                    version: r.version,
                    tag: r.tag,
                });
                counter.inc();
                done.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(((*key).clone(), result));
            });
        }
    });
    done.into_inner().unwrap_or_else(|e| e.into_inner())
}

fn cache_path(cfg: &Config) -> PathBuf {
    cfg.cache_dir.join(CACHE_FILE)
}

/// Answers from earlier listings, keyed by [`Lookup::key`].
#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    #[serde(default)]
    entries: BTreeMap<String, CacheEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    version: Version,
    tag: String,
    checked_at: u64,
}

impl CacheEntry {
    fn new(found: &Found, now: u64) -> CacheEntry {
        CacheEntry {
            version: found.version.clone(),
            tag: found.tag.clone(),
            checked_at: now,
        }
    }

    fn found(&self) -> Found {
        Found {
            version: self.version.clone(),
            tag: self.tag.clone(),
        }
    }

    /// Fresh for [`CACHE_TTL_SECS`]. A timestamp from the future — a clock
    /// moved back — is stale rather than fresh for however long the jump was.
    fn fresh(&self, now: u64) -> bool {
        now >= self.checked_at && now - self.checked_at < CACHE_TTL_SECS
    }
}

impl Cache {
    /// The cache is a convenience: unreadable or malformed means empty, and
    /// the next listing simply asks the sources again.
    fn load(path: &Path, report: &Report) -> Cache {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Cache::default();
        };
        serde_json::from_str(&text).unwrap_or_else(|e| {
            report.debug(&format!("ignoring {}: {e}", path.display()));
            Cache::default()
        })
    }

    fn expire(&mut self, now: u64) {
        self.entries.retain(|_, entry| entry.fresh(now));
    }

    /// Written whole and swapped in, so two listings at once cannot leave a
    /// torn file; a failure costs the next listing a lookup, nothing more.
    fn save(&self, path: &Path, report: &Report) {
        if let Err(e) = self.try_save(path) {
            report.debug(&format!("could not write {}: {e}", path.display()));
        }
    }

    fn try_save(&self, path: &Path) -> Result<()> {
        let dir = path
            .parent()
            .ok_or_else(|| Error::msg("cache path has no parent"))?;
        std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        let text =
            serde_json::to_string(self).map_err(|e| Error::parse("latest cache", e.to_string()))?;
        let staged = tempfile::NamedTempFile::new_in(dir).map_err(|e| Error::io(dir, e))?;
        std::fs::write(staged.path(), text).map_err(|e| Error::io(staged.path(), e))?;
        staged.persist(path).map_err(|e| Error::io(path, e.error))?;
        Ok(())
    }
}

/// The line under the table naming each package that has an update, with the
/// command that takes them all.
pub fn update_footer(rows: &[Row]) -> Option<String> {
    let names: Vec<&str> = rows
        .iter()
        .filter(|row| row.update_available())
        .map(|row| row.name.as_str())
        .collect();
    match names.len() {
        0 => None,
        1 => Some(format!("1 update available: ketch upgrade {}", names[0])),
        n => Some(format!(
            "{n} updates available: ketch upgrade {}",
            names.join(" ")
        )),
    }
}

/// The line under the table naming each package whose `latest` is `?`.
pub fn unreachable_note(rows: &[Row]) -> Option<String> {
    let names = unreachable(rows);
    (!names.is_empty()).then(|| {
        format!(
            "{UNKNOWN} means the latest release could not be checked: {}",
            names.join(", ")
        )
    })
}

/// Names of the rows whose source did not answer.
pub fn unreachable(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .filter(|row| row.latest == Latest::Unreachable)
        .map(|row| row.name.clone())
        .collect()
}

/// Rows for `ketch list remote`: `package`, `latest`, `description`, with
/// each description cut so the row fits `width` columns when there is one.
///
/// A description is somebody else's prose, so it is filtered and folded onto
/// one line before it is measured; the table filters again, which is then a
/// no-op.
pub fn remote_cells(rows: &[Row], width: Option<usize>) -> Vec<Vec<String>> {
    let cells: Vec<(String, String, String)> = rows
        .iter()
        .map(|row| {
            let description = row
                .available
                .as_ref()
                .and_then(|a| a.description.as_deref())
                .map(|d| {
                    crate::changelog::sanitize(d)
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            (
                crate::changelog::sanitize(&row.name),
                row.latest_cell(),
                description,
            )
        })
        .collect();
    let name_width = cells
        .iter()
        .map(|c| c.0.chars().count())
        .chain(std::iter::once("package".len()))
        .max()
        .unwrap_or(0);
    let latest_width = cells
        .iter()
        .map(|c| c.1.chars().count())
        .chain(std::iter::once("latest".len()))
        .max()
        .unwrap_or(0);
    // Two separators of two spaces each. Below a dozen columns a description
    // says nothing, so a very narrow terminal wraps rather than shows `…`.
    let room = width.map(|w| w.saturating_sub(name_width + latest_width + 4).max(12));
    cells
        .into_iter()
        .map(|(name, latest, description)| {
            let description = match room {
                Some(room) => crate::text::truncate(&description, room),
                None => description,
            };
            vec![name, latest, description]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn local(name: &str, version: &str) -> Local {
        Local {
            name: name.to_string(),
            source: PackageRef::new("test", name),
            version: Version::parse(version),
            tag: format!("v{version}"),
            pinned: false,
            retained: Vec::new(),
            prerelease: false,
        }
    }

    fn offer(name: &str) -> Available {
        Available {
            name: name.to_string(),
            source: PackageRef::new("test", name),
            description: Some(format!("the {name} tool")),
            prerelease: false,
        }
    }

    fn found(version: &str) -> Found {
        Found {
            version: Version::parse(version),
            tag: format!("v{version}"),
        }
    }

    fn names(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn merging_installed_only_lists_each_installed_package() {
        let rows = merge(
            vec![local("bravo", "1.0.0"), local("alpha", "2.0.0")],
            vec![],
        );
        assert_eq!(names(&rows), vec!["alpha", "bravo"]);
        assert!(rows
            .iter()
            .all(|r| r.local.is_some() && r.available.is_none()));
    }

    #[test]
    fn merging_available_only_lists_each_registry_package_uninstalled() {
        let rows = merge(vec![], vec![offer("bravo"), offer("alpha")]);
        assert_eq!(names(&rows), vec!["alpha", "bravo"]);
        assert!(rows
            .iter()
            .all(|r| r.local.is_none() && r.available.is_some()));
    }

    #[test]
    fn merging_an_installed_registry_package_gives_one_row_with_both_halves() {
        let rows = merge(
            vec![local("alpha", "1.0.0")],
            vec![offer("alpha"), offer("bravo")],
        );
        assert_eq!(names(&rows), vec!["alpha", "bravo"]);
        assert!(rows[0].local.is_some() && rows[0].available.is_some());
        assert!(rows[1].local.is_none());
    }

    #[test]
    fn merging_matches_a_renamed_package_by_its_source() {
        let mut renamed = local("rg", "1.0.0");
        renamed.source = PackageRef::github("BurntSushi/ripgrep");
        let mut curated = offer("ripgrep");
        curated.source = PackageRef::github("burntsushi/ripgrep");
        let rows = merge(vec![renamed], vec![curated]);
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(rows[0].local.is_some() && rows[0].available.is_some());
    }

    #[test]
    fn an_installed_package_missing_from_the_registry_is_still_listed_with_its_own_source() {
        let mut direct = local("gamma", "1.0.0");
        direct.source = PackageRef::github("owner/gamma");
        let rows = merge(vec![direct], vec![offer("alpha")]);
        assert_eq!(names(&rows), vec!["alpha", "gamma"]);
        let gamma = &rows[1];
        assert!(gamma.available.is_none());
        assert_eq!(gamma.source(), Some(&PackageRef::github("owner/gamma")));
        assert_eq!(
            gamma.lookup(),
            Some(Lookup {
                source: PackageRef::github("owner/gamma"),
                prerelease: false
            })
        );
    }

    #[test]
    fn a_package_installed_from_a_local_path_is_not_looked_up() {
        let mut path = local("delta", "1.0.0");
        path.source = PackageRef::new("local", "/tmp/delta");
        let rows = merge(vec![path], vec![]);
        assert_eq!(rows[0].lookup(), None);
        assert_eq!(rows[0].latest_cell(), "");
    }

    #[test]
    fn merging_keeps_a_pinned_package_pinned() {
        let mut held = local("alpha", "1.0.0");
        held.pinned = true;
        let mut rows = merge(vec![held], vec![offer("alpha")]);
        rows[0].latest = Latest::Found(found("2.0.0"));
        assert_eq!(
            rows[0].local.as_ref().map(Local::cell).as_deref(),
            Some("1.0.0 (pinned)")
        );
        assert!(!rows[0].update_available());
        assert_eq!(rows[0].latest_cell(), "2.0.0");
        assert_eq!(update_footer(&rows), None);
    }

    #[test]
    fn the_installed_lookup_wins_over_the_registry_one() {
        let mut installed = local("alpha", "1.0.0");
        installed.prerelease = true;
        installed.source = PackageRef::new("test", "alpha-fork");
        let rows = merge(vec![installed], vec![offer("alpha")]);
        assert_eq!(
            rows[0].lookup(),
            Some(Lookup {
                source: PackageRef::new("test", "alpha-fork"),
                prerelease: true
            })
        );
    }

    #[test]
    fn a_newer_release_is_an_update() {
        assert!(update_available(&local("alpha", "1.0.0"), &found("1.1.0")));
    }

    #[test]
    fn the_same_release_is_not_an_update() {
        assert!(!update_available(&local("alpha", "1.0.0"), &found("1.0.0")));
    }

    #[test]
    fn the_installed_tag_is_not_an_update_however_its_version_parses() {
        let mut odd = found("1.0.1");
        odd.tag = "v1.0.0".into();
        assert!(!update_available(&local("alpha", "1.0.0"), &odd));
    }

    #[test]
    fn an_older_release_is_not_an_update() {
        assert!(!update_available(&local("alpha", "2.0.0"), &found("1.9.9")));
    }

    #[test]
    fn a_prerelease_is_an_update_only_when_prereleases_are_allowed() {
        let stable = local("alpha", "1.0.0");
        assert!(!update_available(&stable, &found("1.1.0-rc.1")));
        let mut opted_in = stable.clone();
        opted_in.prerelease = true;
        assert!(update_available(&opted_in, &found("1.1.0-rc.1")));
    }

    #[test]
    fn a_stable_release_supersedes_the_prerelease_it_follows() {
        assert!(update_available(
            &local("alpha", "2.0.0-rc.1"),
            &found("2.0.0")
        ));
    }

    #[test]
    fn a_pinned_package_never_has_an_update() {
        let mut held = local("alpha", "1.0.0");
        held.pinned = true;
        assert!(!update_available(&held, &found("9.0.0")));
        held.prerelease = true;
        assert!(!update_available(&held, &found("9.0.0-rc.1")));
    }

    #[test]
    fn the_prerelease_flag_separates_cache_entries() {
        let stable = Lookup {
            source: PackageRef::new("test", "alpha"),
            prerelease: false,
        };
        let pre = Lookup {
            prerelease: true,
            ..stable.clone()
        };
        assert_ne!(stable.key(), pre.key());
    }

    #[test]
    fn the_footer_counts_updates_and_names_the_command() {
        let mut rows = merge(
            vec![local("alpha", "1.0.0"), local("bravo", "1.0.0")],
            vec![offer("charlie")],
        );
        rows[0].latest = Latest::Found(found("2.0.0"));
        rows[1].latest = Latest::Found(found("1.0.0"));
        rows[2].latest = Latest::Found(found("5.0.0"));
        assert_eq!(
            update_footer(&rows).as_deref(),
            Some("1 update available: ketch upgrade alpha")
        );
        rows[1].latest = Latest::Found(found("1.1.0"));
        assert_eq!(
            update_footer(&rows).as_deref(),
            Some("2 updates available: ketch upgrade alpha bravo")
        );
    }

    #[test]
    fn the_unreachable_note_names_every_question_mark() {
        let mut rows = merge(vec![local("alpha", "1.0.0")], vec![offer("bravo")]);
        assert_eq!(unreachable_note(&rows), None);
        rows[0].latest = Latest::Unreachable;
        rows[1].latest = Latest::Unreachable;
        assert_eq!(rows[0].latest_cell(), "?");
        assert_eq!(
            unreachable_note(&rows).as_deref(),
            Some("? means the latest release could not be checked: alpha, bravo")
        );
    }

    #[test]
    fn nothing_answering_is_offline_but_one_answer_is_not() {
        assert!(!Outcome::default().offline());
        assert!(Outcome {
            asked: 3,
            answered: 0
        }
        .offline());
        assert!(!Outcome {
            asked: 3,
            answered: 1
        }
        .offline());
    }

    #[test]
    fn a_cache_entry_is_fresh_for_the_ttl_and_not_from_the_future() {
        let entry = CacheEntry::new(&found("1.0.0"), 1_000);
        assert!(entry.fresh(1_000));
        assert!(entry.fresh(1_000 + CACHE_TTL_SECS - 1));
        assert!(!entry.fresh(1_000 + CACHE_TTL_SECS));
        assert!(!entry.fresh(999));
    }

    #[test]
    fn the_cache_round_trips_and_a_malformed_one_is_empty() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("cache").join(CACHE_FILE);
        let mut cache = Cache::default();
        cache.entries.insert(
            "test:alpha#stable".into(),
            CacheEntry::new(&found("1.2.3"), 5),
        );
        cache.save(&path, &Report::silent());
        let back = Cache::load(&path, &Report::silent());
        assert_eq!(
            back.entries.get("test:alpha#stable").map(CacheEntry::found),
            Some(found("1.2.3"))
        );
        std::fs::write(&path, "not json").expect("write");
        assert!(Cache::load(&path, &Report::silent()).entries.is_empty());
    }

    #[test]
    fn remote_descriptions_are_cut_to_the_width_and_filtered() {
        let mut long = offer("alpha");
        long.description = Some("a very\nlong \u{1b}[31mdescription that goes on".into());
        let mut rows = vec![Row::offered(long)];
        rows[0].latest = Latest::Found(found("1.0.0"));
        // package (7) + latest (6) + 4 separators leaves 13 of 30.
        let cells = remote_cells(&rows, Some(30));
        assert_eq!(cells[0][2].chars().count(), 13, "{:?}", cells[0][2]);
        assert!(cells[0][2].ends_with('…'));
        assert!(!cells[0][2].contains('\u{1b}'));
        assert!(!cells[0][2].contains('\n'));
        let whole = remote_cells(&rows, None);
        assert_eq!(whole[0][2], "a very long [31mdescription that goes on");
    }
}
