// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! ketch-ffi — `ketch-core` exported through UniFFI, for front ends written in
//! another language that link the core in-process: the SwiftUI app first, a
//! Windows front end later, from the same Rust and the same generated
//! metadata.
//!
//! The surface is coarse on purpose. One object, [`KetchCore`], with one
//! method per thing a person does — list what is installed, search, see what
//! is outdated, install, upgrade, uninstall, read a changelog, run the doctor —
//! returning plain records ([`records`]). The core's types stay unannotated, so
//! nothing here constrains how they change; the price is a conversion per
//! record, tested in each module. What the core says while it works reaches a
//! foreign [`Reporter`], what it asks reaches a foreign [`Decider`], and a
//! [`CancelToken`] stops an operation; all three are adapters onto the core's
//! own traits ([`callbacks`]). Errors are one enum, [`KetchError`].
//!
//! The reporter, decider and token are passed to each call, not to the
//! constructor. An app shows each operation where it was started — a progress
//! row in one screen, a sheet in another — so one `KetchCore` serving two
//! screens at once has to tell their events and questions apart, and a
//! reporter fixed at construction would send both to the same place. Calls
//! that change nothing take only a reporter; `root` and `installed` read a
//! file and take none.
//!
//! Threading follows the rules the core sets for a long-running host. Every
//! method is synchronous and blocks for as long as the work takes, so a caller
//! runs it off its UI thread. Each call builds its `Config` and opens the log
//! afresh, so an edit to `config.toml` is seen by the next call. A call that
//! changes the install tree holds `state::Lock` for its whole run; a second one
//! from any thread or process fails at once with [`KetchError::Busy`] rather
//! than waiting. `ketch registry push` and its tokio runtime are not exported.
//!
//! `unsafe_code` stays `forbid` here, as in the rest of the workspace. The
//! scaffolding `uniffi::setup_scaffolding!` and `#[uniffi::export]` generate is unsafe by
//! nature — `#[unsafe(no_mangle)] unsafe extern "C"` functions reading raw
//! pointers and `RustBuffer`s from the foreign side — but it is expanded by a
//! macro from another crate, and rustc does not apply the lint to such
//! expansions (checked with uniffi 0.32.2). Code written in this crate is
//! still linted, so `forbid` costs nothing and keeps hand-written `unsafe`
//! out. Should a later uniffi trip the lint, the fix is this crate's own
//! `[lints.rust]` with `unsafe_code = "deny"` and an `allow` on the generated
//! items, not a weaker workspace.

uniffi::setup_scaffolding!();

pub mod callbacks;
pub mod error;
pub mod records;

pub use callbacks::{CancelToken, Decider, Event, Holder, Reporter, Stage, TaskKind};
pub use error::KetchError;
pub use records::{
    Changelog, ChangelogSource, Check, CheckOutcome, InstallOptions, Installed, Package,
    RegistryPackage, Repository, SearchResults, Upgrade,
};

use callbacks::{ForeignDecider, ForeignReporter};
use ketch_core::cancel::Cancel;
use ketch_core::changelog;
use ketch_core::config::Config;
use ketch_core::error::Error;
use ketch_core::install::{self, InstallRequest};
use ketch_core::listing::{self, Available, Local, Row};
use ketch_core::log;
use ketch_core::manifest::Resolver;
use ketch_core::model::{PackageSpec, VersionSpec};
use ketch_core::process;
use ketch_core::report::{Ctx, LogReporter, Report};
use ketch_core::source::SourceRegistry;
use ketch_core::state::{Lock, State};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

type Result<T> = std::result::Result<T, KetchError>;

/// The version of the core this library was built from, as `ketch --version`
/// prints it.
#[uniffi::export]
pub fn ketch_version() -> String {
    ketch_core::self_update::display_version()
}

/// The core, for one ketch root. Cheap to create and safe to share between
/// threads; it holds no state between calls besides what it was created with.
#[derive(uniffi::Object)]
pub struct KetchCore {
    root: Option<PathBuf>,
}

/// What one call runs with: its own configuration, the reporter that logs and
/// forwards to the caller's, and the caller's decider.
struct Operation {
    cfg: Config,
    report: Report,
    decider: Option<ForeignDecider>,
}

impl Operation {
    /// The context the core's calls take, asking this call's decider. Without
    /// one every question is declined, as under `ketch --yes`.
    fn ctx(&self) -> Ctx<'_> {
        let cx = Ctx::new(&self.cfg, &self.report);
        match &self.decider {
            Some(decider) => cx.with_decider(decider),
            None => cx,
        }
    }
}

#[uniffi::export]
impl KetchCore {
    /// The core for `root`, or for `KETCH_ROOT` / `~/.ketch` when `None` —
    /// the same tree, state and lock the CLI uses.
    #[uniffi::constructor]
    pub fn new(root: Option<String>) -> Arc<Self> {
        Arc::new(KetchCore {
            root: root.map(PathBuf::from),
        })
    }

    /// The ketch root this core manages, resolved as the next call will.
    pub fn root(&self) -> Result<String> {
        Ok(self.operation(None, None)?.cfg.root.display().to_string())
    }

    /// Installed packages, by name. Reads `state.json` only; no network.
    pub fn installed(&self) -> Result<Vec<Package>> {
        let op = self.operation(None, None)?;
        let state = State::load(&op.cfg)?;
        Ok(state.iter().map(Package::from).collect())
    }

    /// Packages matching `query`: what the registry and the person's own
    /// manifests know first, then repositories the sources find, `limit` in
    /// all. A source that cannot be reached is a warning to `reporter`; only
    /// when every one failed and nothing was found is it an error.
    pub fn search(
        &self,
        query: String,
        limit: u32,
        reporter: Option<Arc<dyn Reporter>>,
    ) -> Result<SearchResults> {
        let query = query.trim();
        if query.is_empty() {
            return Err(KetchError::Other {
                message: "nothing to search for".into(),
            });
        }
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let op = self.operation(reporter, None)?;
        let cx = op.ctx();
        let resolver = Resolver::new(&cx)?;
        let manifests: Vec<_> = resolver.search(query).into_iter().take(limit).collect();
        // One row each, not `listing::merge`: that sorts by name, and search
        // results keep the resolver's order.
        let mut rows: Vec<Row> = manifests
            .iter()
            .map(|m| Row::offered(Available::from_manifest(&op.cfg, m)))
            .collect();
        listing::fill_cached(&cx, &mut rows);
        let known: Vec<RegistryPackage> = manifests
            .iter()
            .zip(&rows)
            .map(|(m, row)| RegistryPackage::new(m, row.latest_version()))
            .collect();

        let rest = limit.saturating_sub(known.len());
        let mut repositories = Vec::new();
        if rest > 0 {
            let mut unreachable = 0usize;
            for source in SourceRegistry::load(&cx).all() {
                match source.search(query, rest) {
                    Ok(hits) => repositories
                        .extend(hits.iter().map(|hit| Repository::new(source.scheme(), hit))),
                    Err(e) => {
                        op.report.warn(&format!("{}: {e}", source.scheme()));
                        unreachable += 1;
                    }
                }
            }
            repositories.truncate(rest);
            // "Nothing found" after every source failed would be a claim made
            // from no evidence, the mistake the CLI's search refuses too.
            if known.is_empty() && repositories.is_empty() && unreachable > 0 {
                return Err(KetchError::Network {
                    message: format!(
                        "no source could be searched for `{query}` ({unreachable} failed)"
                    ),
                });
            }
        }
        Ok(SearchResults {
            known,
            repositories,
        })
    }

    /// Installed packages with a newer release, under the rules `ketch
    /// outdated` uses, except that a pinned package is reported too, marked
    /// [`Upgrade::pinned`], so an app can show what its pin holds back. Local
    /// packages never have one. An answer looked up in the last ten minutes is
    /// reused.
    pub fn outdated(&self, reporter: Option<Arc<dyn Reporter>>) -> Result<Vec<Upgrade>> {
        let op = self.operation(reporter, None)?;
        let cx = op.ctx();
        let state = State::load(&op.cfg)?;
        upgrades(&cx, &state, None)
    }

    /// Install `specs` (`ripgrep`, `BurntSushi/ripgrep@14.1.0`,
    /// `local:/abs/path`), several at once. An installed package with a newer
    /// release is updated. One failure does not undo the others: what
    /// succeeded is recorded, then the failure is returned — the package's own
    /// error for a single spec, a summary for several.
    pub fn install(
        &self,
        specs: Vec<String>,
        options: InstallOptions,
        reporter: Option<Arc<dyn Reporter>>,
        decider: Option<Arc<dyn Decider>>,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<Vec<Installed>> {
        let specs = distinct(specs);
        if specs.is_empty() {
            return Err(KetchError::Other {
                message: "nothing to install".into(),
            });
        }
        if options.bin.is_some() && specs.len() > 1 {
            return Err(KetchError::Other {
                message: "a binary choice names one package, so it needs exactly one spec".into(),
            });
        }
        let op = self.operation(reporter, decider)?;
        let cx = op.ctx();
        let _lock = Lock::acquire(&cx)?;
        let sources = SourceRegistry::load(&cx);
        let mut state = State::load(&op.cfg)?;
        let cancel = token(cancel);
        let reqs: Vec<InstallRequest> = specs
            .iter()
            .map(|raw| InstallRequest {
                force: options.force,
                prerelease: options.prerelease,
                link: options.link,
                require_checksum: options.require_checksum || op.cfg.require_checksums,
                bin: options.bin.clone(),
                cancel: cancel.clone(),
                ..InstallRequest::new(PackageSpec::parse(raw))
            })
            .collect();
        let outcomes = install::batch(&cx, &sources, &mut state, &reqs, op.cfg.jobs);
        let labels = reqs.iter().map(|r| r.spec.label()).collect();
        settle(&op.cfg, &state, labels, outcomes)
    }

    /// Upgrade `names`, or every installed package when empty, to the
    /// releases `outdated` reports, leaving pinned packages where they are.
    /// Processes holding the files about to be replaced are put to `decider`.
    /// Failures are returned as `install` returns them.
    pub fn upgrade(
        &self,
        names: Vec<String>,
        reporter: Option<Arc<dyn Reporter>>,
        decider: Option<Arc<dyn Decider>>,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<Vec<Installed>> {
        let op = self.operation(reporter, decider)?;
        let cx = op.ctx();
        let _lock = Lock::acquire(&cx)?;
        let sources = SourceRegistry::load(&cx);
        let mut state = State::load(&op.cfg)?;
        let wanted = if names.is_empty() {
            None
        } else {
            Some(installed_names(&state, &names)?)
        };
        let mut plan = upgrades(&cx, &state, wanted.as_ref())?;
        plan.retain(|u| !u.pinned);
        if plan.is_empty() {
            return Ok(Vec::new());
        }

        let cancel = token(cancel);
        let mut files = Vec::new();
        let mut reqs = Vec::new();
        let mut labels = Vec::new();
        for upgrade in &plan {
            let Some(pkg) = state.get(&upgrade.name) else {
                continue;
            };
            files.extend(
                pkg.links
                    .iter()
                    .flat_map(|link| [link.link.clone(), link.target.clone()]),
            );
            reqs.push(install::upgrade_request(
                &op.cfg,
                pkg,
                &upgrade.tag,
                op.cfg.prerelease,
                None,
                cancel.clone(),
            ));
            labels.push(upgrade.name.clone());
        }
        process::offer_to_stop(&files, false, &cx);
        let outcomes = install::batch(&cx, &sources, &mut state, &reqs, op.cfg.jobs);
        settle(&op.cfg, &state, labels, outcomes)
    }

    /// Remove `names`: their links, their files and their records. Every name
    /// is checked first, so an unknown one stops the call with `NotFound`
    /// before any installed package is removed — though, as `ketch uninstall`
    /// does, a `store/<name>/` folder a failed uninstall left behind for that
    /// name is taken away, since nothing else ever will. `cancel` is checked
    /// before each package: what was removed by then stays removed.
    pub fn uninstall(
        &self,
        names: Vec<String>,
        reporter: Option<Arc<dyn Reporter>>,
        decider: Option<Arc<dyn Decider>>,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<Vec<Package>> {
        let op = self.operation(reporter, decider)?;
        let cx = op.ctx();
        let _lock = Lock::acquire(&cx)?;
        let mut state = State::load(&op.cfg)?;
        let missing: Vec<&String> = names.iter().filter(|n| state.find(n).is_none()).collect();
        if let Some(first) = missing.first() {
            for name in &missing {
                install::remove_package_dir(&op.cfg, name, &op.report);
            }
            return Err(KetchError::NotFound {
                name: (*first).clone(),
            });
        }
        let targets = installed_names(&state, &names)?;

        let cancel = token(cancel);
        let mut removed = Vec::new();
        let mut failed = Vec::new();
        for name in &targets {
            let outcome = cancel.check().and_then(|()| {
                op.report.step("removing", name);
                install::uninstall(&cx, &mut state, name)
            });
            match outcome {
                Ok(pkg) => {
                    op.report
                        .success("removed", &format!("{} {}", pkg.name, pkg.version));
                    removed.push(Package::from(&pkg));
                }
                Err(e) => failed.push((name.clone(), e)),
            }
        }
        if !removed.is_empty() {
            state.save(&op.cfg)?;
        }
        match summarise(targets.len(), failed) {
            Some(error) => Err(error),
            None => Ok(removed),
        }
    }

    /// What changed in `package`: the installed version when `version` is
    /// `None`, read from the changelog file the release ships when it has a
    /// section for that version, else from the notes the release published.
    /// Text is filtered of control characters.
    pub fn changelog(
        &self,
        package: String,
        version: Option<String>,
        reporter: Option<Arc<dyn Reporter>>,
    ) -> Result<Changelog> {
        let op = self.operation(reporter, None)?;
        let cx = op.ctx();
        let state = State::load(&op.cfg)?;
        let raw = match &version {
            Some(v) => format!("{package}@{v}"),
            None => package,
        };
        let spec = PackageSpec::parse(&raw);
        let installed = state.find_spec(&spec).cloned();

        // Only the installed version has a file on disk. A file with no
        // section for it is not an answer about it, so the published notes
        // come first and the whole file is the fallback.
        let mut whole_file = None;
        if let (Some(pkg), VersionSpec::Latest) = (&installed, &spec.version) {
            let version = pkg.version.to_string();
            if let Some(path) = changelog::find_file(&pkg.prefix) {
                let entry = changelog::from_file(&path, Some(&version))?;
                if entry.heading.is_some() {
                    return Ok(Changelog::new(&pkg.name, &version, entry));
                }
                whole_file = Some(Changelog::new(&pkg.name, &version, entry));
            }
        }
        match changelog::published(&cx, &spec, installed, false) {
            Ok((name, version, entry)) => Ok(Changelog::new(&name, &version, entry)),
            Err(e) => match whole_file {
                Some(file) => {
                    op.report.warn(&e.to_string());
                    Ok(file)
                }
                None => Err(e.into()),
            },
        }
    }

    /// What changed in `package` between two versions: one entry per release
    /// newer than `from` and no newer than `to`, newest first, from the notes
    /// each release published. `from` defaults to the installed version, so
    /// the range is what an upgrade would bring, and `to` to the newest
    /// release. A release that published no notes is left out. Text is
    /// filtered of control characters.
    pub fn changelog_range(
        &self,
        package: String,
        from: Option<String>,
        to: Option<String>,
        reporter: Option<Arc<dyn Reporter>>,
    ) -> Result<Vec<Changelog>> {
        let op = self.operation(reporter, None)?;
        let cx = op.ctx();
        let state = State::load(&op.cfg)?;
        let spec = PackageSpec::parse(&package);
        let installed = state.find_spec(&spec).cloned();
        let (name, entries) =
            changelog::published_range(&cx, &spec, installed, from.as_deref(), to.as_deref())?;
        Ok(entries
            .into_iter()
            .map(|(version, entry)| Changelog::new(&name, &version, entry))
            .collect())
    }

    /// Every `ketch doctor` check. Repairs nothing.
    pub fn doctor(&self, reporter: Option<Arc<dyn Reporter>>) -> Result<Vec<Check>> {
        let op = self.operation(reporter, None)?;
        Ok(ketch_core::doctor::checks(&op.ctx())
            .iter()
            .map(Check::from)
            .collect())
    }
}

impl KetchCore {
    /// Configuration and log for one call, built afresh: the core keeps no
    /// configuration across operations, and neither does this.
    fn operation(
        &self,
        reporter: Option<Arc<dyn Reporter>>,
        decider: Option<Arc<dyn Decider>>,
    ) -> Result<Operation> {
        let forward = reporter.map(|r| Report::new(ForeignReporter(r)));
        let report = Report::new(LogReporter::new(forward.clone()));
        let cfg = Config::load(self.root.clone(), &report)?;
        cfg.ensure_dirs()?;
        if let Err(e) = log::init(&cfg, false) {
            // Past the log: reporting it through `report` would log the
            // failure to the log that failed.
            if let Some(forward) = &forward {
                forward.warn(&e.to_string());
            }
        }
        Ok(Operation {
            cfg,
            report,
            decider: decider.map(ForeignDecider),
        })
    }
}

/// The token an operation checks: the caller's, or one nothing cancels.
fn token(cancel: Option<Arc<CancelToken>>) -> Cancel {
    cancel.map(|t| t.0.clone()).unwrap_or_default()
}

/// `specs` without repeats, in the order given.
fn distinct(specs: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    specs
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

/// The installed names `names` refer to, or `NotFound` for the first that
/// matches nothing.
fn installed_names(state: &State, names: &[String]) -> Result<BTreeSet<String>> {
    names
        .iter()
        .map(|n| {
            state
                .find(n)
                .map(|p| p.name.clone())
                .ok_or_else(|| KetchError::NotFound { name: n.clone() })
        })
        .collect()
}

/// The upgrades on offer for installed packages, all of them or those in
/// `only`: `ketch list`'s lookups, cache and update rules.
fn upgrades(cx: &Ctx<'_>, state: &State, only: Option<&BTreeSet<String>>) -> Result<Vec<Upgrade>> {
    let locals = state
        .iter()
        .filter(|p| only.is_none_or(|names| names.contains(&p.name)))
        .map(|p| Local::from_installed(cx.cfg, p))
        .collect();
    let mut rows = listing::merge(locals, Vec::new());
    let sources = SourceRegistry::load(cx);
    if listing::fill_latest(cx, &sources, &mut rows).offline() {
        return Err(KetchError::Network {
            message: "could not reach any package source to check the latest versions".into(),
        });
    }
    Ok(rows.iter().filter_map(Upgrade::from_row).collect())
}

/// Record what an install batch placed, then answer for the batch.
fn settle(
    cfg: &Config,
    state: &State,
    labels: Vec<String>,
    outcomes: Vec<ketch_core::error::Result<install::Installed>>,
) -> Result<Vec<Installed>> {
    let total = labels.len();
    let mut done = Vec::new();
    let mut failed = Vec::new();
    for (label, outcome) in labels.into_iter().zip(outcomes) {
        match outcome {
            Ok(out) => done.push(Installed::from(&out)),
            Err(e) => failed.push((label, e)),
        }
    }
    // Saved before any failure is returned: one bad package must not discard
    // the ones that were already placed.
    if !done.is_empty() {
        state.save(cfg)?;
    }
    match summarise(total, failed) {
        Some(error) => Err(error),
        None => Ok(done),
    }
}

/// The error for a batch of `total` in which `failed` went wrong, if any did:
/// a lone package's own error, `Cancelled` when cancelling is all that
/// happened, otherwise one message naming each failure.
fn summarise(total: usize, mut failed: Vec<(String, Error)>) -> Option<KetchError> {
    if failed.is_empty() {
        return None;
    }
    if total == 1 && failed.len() == 1 {
        return failed.pop().map(|(_, e)| e.into());
    }
    if failed.iter().all(|(_, e)| matches!(e, Error::Cancelled)) {
        return Some(KetchError::Cancelled);
    }
    let mut lines = vec![format!("{} of {total} packages failed", failed.len())];
    lines.extend(
        failed
            .into_iter()
            .map(|(label, e)| format!("{label}: {}", KetchError::from(e))),
    );
    Some(KetchError::Other {
        message: lines.join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn scratch() -> (tempfile::TempDir, Arc<KetchCore>) {
        let dir = tempfile::tempdir().unwrap();
        let core = KetchCore::new(Some(dir.path().join("root").display().to_string()));
        (dir, core)
    }

    /// A one-file `local:` package under `dir`, named `name`.
    fn payload(dir: &std::path::Path, name: &str) -> String {
        let payload = dir.join("payload").join(name);
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join(name), "#!/bin/sh\necho hi\n").unwrap();
        format!("local:{}", payload.join(name).display())
    }

    #[test]
    fn a_scratch_root_has_nothing_installed() {
        let (_dir, core) = scratch();
        assert_eq!(core.installed().unwrap(), Vec::new());
    }

    #[test]
    fn uninstalling_an_unknown_name_is_not_found_before_anything_changes() {
        let (_dir, core) = scratch();
        assert_eq!(
            core.uninstall(vec!["nope".into()], None, None, None),
            Err(KetchError::NotFound {
                name: "nope".into()
            })
        );
    }

    #[test]
    fn a_held_lock_makes_a_mutating_call_busy() {
        let (_dir, core) = scratch();
        let op = core.operation(None, None).unwrap();
        let _held = Lock::acquire(&op.ctx()).unwrap();
        let err = core
            .uninstall(vec!["nope".into()], None, None, None)
            .unwrap_err();
        assert!(matches!(err, KetchError::Busy { .. }), "{err:?}");
    }

    #[derive(Default)]
    struct Recorded(std::sync::Mutex<Vec<Event>>);

    impl Recorded {
        /// Whether a package reached the installing stage. Not checked by
        /// name: the name a local file installs under differs on Windows.
        fn saw_installing(&self) -> bool {
            self.0.lock().unwrap().iter().any(|e| {
                matches!(
                    e,
                    Event::Step {
                        stage: Stage::Installing,
                        ..
                    }
                )
            })
        }
    }

    impl Reporter for Recorded {
        fn event(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn a_local_package_installs_reports_and_uninstalls() {
        let (dir, core) = scratch();
        let recorded = Arc::new(Recorded::default());
        let placed = core
            .install(
                vec![payload(dir.path(), "hello")],
                InstallOptions::default(),
                Some(recorded.clone()),
                None,
                None,
            )
            .unwrap();
        assert_eq!(placed.len(), 1);
        let name = placed[0].package.name.clone();
        assert_eq!(
            core.installed()
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>(),
            vec![name.clone()]
        );
        assert!(recorded.saw_installing());

        let removed = core
            .uninstall(vec![name.clone()], None, None, None)
            .unwrap();
        assert_eq!(removed[0].name, name);
        assert_eq!(core.installed().unwrap(), Vec::new());
    }

    #[test]
    fn each_call_reports_to_its_own_reporter_only() {
        let (dir, core) = scratch();
        let installing = Arc::new(Recorded::default());
        let removing = Arc::new(Recorded::default());
        let placed = core
            .install(
                vec![payload(dir.path(), "hello")],
                InstallOptions::default(),
                Some(installing.clone()),
                None,
                None,
            )
            .unwrap();
        let name = placed[0].package.name.clone();
        core.uninstall(vec![name.clone()], Some(removing.clone()), None, None)
            .unwrap();
        assert!(installing.saw_installing());
        assert!(!removing.saw_installing());
        assert!(removing.0.lock().unwrap().iter().any(|e| matches!(
            e,
            Event::Success { verb, detail } if verb == "removed" && detail.starts_with(&name)
        )));
    }

    #[test]
    fn a_cancelled_install_places_nothing() {
        let (dir, core) = scratch();
        let token = CancelToken::new();
        token.cancel();
        let err = core
            .install(
                vec![payload(dir.path(), "hello")],
                InstallOptions::default(),
                None,
                None,
                Some(token),
            )
            .unwrap_err();
        assert_eq!(err, KetchError::Cancelled);
        assert_eq!(core.installed().unwrap(), Vec::new());
    }

    #[test]
    fn a_cancelled_uninstall_removes_nothing() {
        let (dir, core) = scratch();
        let spec = payload(dir.path(), "hello");
        let placed = core
            .install(vec![spec], InstallOptions::default(), None, None, None)
            .unwrap();
        let name = placed[0].package.name.clone();
        let token = CancelToken::new();
        token.cancel();
        let err = core
            .uninstall(vec![name.clone()], None, None, Some(token))
            .unwrap_err();
        assert_eq!(err, KetchError::Cancelled);
        assert_eq!(
            core.installed()
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect::<Vec<_>>(),
            vec![name]
        );
    }

    #[test]
    fn empty_requests_are_refused() {
        let (_dir, core) = scratch();
        assert!(core
            .install(Vec::new(), InstallOptions::default(), None, None, None)
            .is_err());
        assert!(core.search("  ".into(), 10, None).is_err());
    }

    #[test]
    fn a_batch_names_each_failure_and_a_lone_one_keeps_its_own_error() {
        let failures = vec![
            ("a".to_string(), Error::NoRelease("a".into())),
            ("b".to_string(), Error::msg("broken")),
        ];
        let Some(KetchError::Other { message }) = summarise(3, failures) else {
            panic!("not a summary");
        };
        assert_eq!(
            message,
            "2 of 3 packages failed\na: `a` not found\nb: broken"
        );

        let lone = vec![("a".to_string(), Error::NoRelease("a".into()))];
        assert_eq!(
            summarise(1, lone),
            Some(KetchError::NotFound { name: "a".into() })
        );
        let cancelled = vec![
            ("a".to_string(), Error::Cancelled),
            ("b".to_string(), Error::Cancelled),
        ];
        assert_eq!(summarise(2, cancelled), Some(KetchError::Cancelled));
        assert_eq!(summarise(2, Vec::new()), None);
    }

    #[test]
    fn repeated_specs_are_installed_once() {
        assert_eq!(
            distinct(vec!["a".into(), "b".into(), "a".into()]),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn uninstalling_a_name_with_no_record_takes_its_leftover_folder() {
        let (_dir, core) = scratch();
        let store = core.operation(None, None).unwrap().cfg.store_dir;
        let leftover = store.join("ghost");
        std::fs::create_dir_all(leftover.join("1.0.0.old")).unwrap();
        assert_eq!(
            core.uninstall(vec!["ghost".into()], None, None, None),
            Err(KetchError::NotFound {
                name: "ghost".into()
            })
        );
        assert!(!leftover.exists());
    }

    #[test]
    fn an_unknown_name_beside_an_installed_one_removes_neither() {
        let (dir, core) = scratch();
        let placed = core
            .install(
                vec![payload(dir.path(), "hello")],
                InstallOptions::default(),
                None,
                None,
                None,
            )
            .unwrap();
        let name = placed[0].package.name.clone();
        let err = core
            .uninstall(vec![name.clone(), "ghost".into()], None, None, None)
            .unwrap_err();
        assert_eq!(
            err,
            KetchError::NotFound {
                name: "ghost".into()
            }
        );
        assert_eq!(core.installed().unwrap().len(), 1);
    }

    /// A `test:` source plugin in `core`'s root serving `alpha`'s releases,
    /// each with notes, and a user manifest naming it — enough for a search
    /// and a changelog, which need no download. A shell script, so Unix only.
    #[cfg(unix)]
    fn publish_alpha(dir: &std::path::Path, core: &KetchCore, versions: &[&str]) {
        use std::os::unix::fs::PermissionsExt;
        let cfg = core.operation(None, None).unwrap().cfg;
        let releases: Vec<String> = versions
            .iter()
            .map(|v| {
                format!(r#"{{"version":"{v}","tag":"v{v}","notes":"notes for {v}","assets":[]}}"#)
            })
            .collect();
        let feed = dir.join("alpha.releases.json");
        std::fs::write(&feed, format!("[{}]", releases.join(","))).unwrap();
        std::fs::create_dir_all(&cfg.plugin_dir).unwrap();
        let plugin = cfg.plugin_dir.join("ketch-source-test");
        std::fs::write(
            &plugin,
            format!(
                "#!/bin/sh\ncase \"$1\" in\n\
                 capabilities) printf '%s' '{{\"protocol\":1,\"scheme\":\"test\",\"download\":false,\"search\":false}}' ;;\n\
                 describe) printf 'null' ;;\n\
                 releases) cat '{}' ;;\n\
                 search) printf '[]' ;;\n\
                 *) exit 1 ;;\nesac\n",
                feed.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&plugin, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(&cfg.manifest_dir).unwrap();
        std::fs::write(
            cfg.manifest_dir.join("alpha.toml"),
            "name = \"alpha\"\nsource = \"test:alpha\"\ndescription = \"the alpha tool\"\n",
        )
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_changelog_range_is_every_release_between_newest_first() {
        let (dir, core) = scratch();
        publish_alpha(dir.path(), &core, &["1.0.0", "1.1.0", "1.2.0", "1.3.0"]);
        let range = core
            .changelog_range(
                "alpha".into(),
                Some("1.0.0".into()),
                Some("1.2.0".into()),
                None,
            )
            .unwrap();
        let versions: Vec<&str> = range.iter().map(|c| c.version.as_str()).collect();
        assert_eq!(versions, ["1.2.0", "1.1.0"]);
        assert_eq!(range[0].body, "notes for 1.2.0");
        assert_eq!(range[0].source, ChangelogSource::Release);

        let open = core
            .changelog_range("alpha".into(), None, None, None)
            .unwrap();
        assert_eq!(open.len(), 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_search_result_carries_the_latest_an_earlier_listing_cached() {
        let (dir, core) = scratch();
        publish_alpha(dir.path(), &core, &["1.0.0", "2.0.0"]);
        // A limit of one leaves no room for repositories, so no source is
        // searched over the network.
        let before = core.search("alpha".into(), 1, None).unwrap();
        assert_eq!(before.known[0].latest, None);

        let op = core.operation(None, None).unwrap();
        let cx = op.ctx();
        let (manifest, _) = Resolver::new(&cx)
            .unwrap()
            .resolve(&PackageSpec::parse("alpha"))
            .unwrap();
        let mut rows = vec![Row::offered(Available::from_manifest(&op.cfg, &manifest))];
        listing::fill_latest(&cx, &SourceRegistry::load(&cx), &mut rows);

        let after = core.search("alpha".into(), 1, None).unwrap();
        assert_eq!(after.known[0].name, "alpha");
        assert_eq!(after.known[0].latest.as_deref(), Some("2.0.0"));
    }
}
