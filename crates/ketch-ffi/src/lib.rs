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
use ketch_core::listing::{self, Local};
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
    reporter: Option<Arc<dyn Reporter>>,
    decider: Option<ForeignDecider>,
}

/// What one call runs with: its own configuration and the reporter that logs
/// and forwards.
struct Operation {
    cfg: Config,
    report: Report,
}

#[uniffi::export]
impl KetchCore {
    /// The core for `root`, or for `KETCH_ROOT` / `~/.ketch` when `None` —
    /// the same tree, state and lock the CLI uses. `reporter` receives every
    /// event; `decider` answers the pipeline's questions, and without one
    /// every question is declined, as under `ketch --yes`.
    #[uniffi::constructor]
    pub fn new(
        root: Option<String>,
        reporter: Option<Box<dyn Reporter>>,
        decider: Option<Box<dyn Decider>>,
    ) -> Arc<Self> {
        Arc::new(KetchCore {
            root: root.map(PathBuf::from),
            reporter: reporter.map(Arc::from),
            decider: decider.map(ForeignDecider),
        })
    }

    /// The ketch root this core manages, resolved as the next call will.
    pub fn root(&self) -> Result<String> {
        Ok(self.operation()?.cfg.root.display().to_string())
    }

    /// Installed packages, by name. Reads `state.json` only; no network.
    pub fn installed(&self) -> Result<Vec<Package>> {
        let op = self.operation()?;
        let state = State::load(&op.cfg)?;
        Ok(state.iter().map(Package::from).collect())
    }

    /// Packages matching `query`: what the registry and the person's own
    /// manifests know first, then repositories the sources find, `limit` in
    /// all. A source that cannot be reached is reported as a warning; only
    /// when every one failed and nothing was found is it an error.
    pub fn search(&self, query: String, limit: u32) -> Result<SearchResults> {
        let query = query.trim();
        if query.is_empty() {
            return Err(KetchError::Other {
                message: "nothing to search for".into(),
            });
        }
        let limit = usize::try_from(limit).unwrap_or(usize::MAX);
        let op = self.operation()?;
        let cx = self.ctx(&op);
        let resolver = Resolver::new(&cx)?;
        let known: Vec<RegistryPackage> = resolver
            .search(query)
            .into_iter()
            .take(limit)
            .map(RegistryPackage::from)
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
    /// outdated` uses: pinned and local packages never have one. An answer
    /// looked up in the last ten minutes is reused.
    pub fn outdated(&self) -> Result<Vec<Upgrade>> {
        let op = self.operation()?;
        let cx = self.ctx(&op);
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
        let op = self.operation()?;
        let cx = self.ctx(&op);
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
    /// releases `outdated` reports. Processes holding the files about to be
    /// replaced are put to the decider. Failures are returned as `install`
    /// returns them.
    pub fn upgrade(
        &self,
        names: Vec<String>,
        cancel: Option<Arc<CancelToken>>,
    ) -> Result<Vec<Installed>> {
        let op = self.operation()?;
        let cx = self.ctx(&op);
        let _lock = Lock::acquire(&cx)?;
        let sources = SourceRegistry::load(&cx);
        let mut state = State::load(&op.cfg)?;
        let wanted = if names.is_empty() {
            None
        } else {
            Some(installed_names(&state, &names)?)
        };
        let plan = upgrades(&cx, &state, wanted.as_ref())?;
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
    /// is checked first, so an unknown one stops the call before anything is
    /// removed.
    pub fn uninstall(&self, names: Vec<String>) -> Result<Vec<Package>> {
        let op = self.operation()?;
        let cx = self.ctx(&op);
        let _lock = Lock::acquire(&cx)?;
        let mut state = State::load(&op.cfg)?;
        let targets = installed_names(&state, &names)?;

        let mut removed = Vec::new();
        let mut failed = Vec::new();
        for name in &targets {
            match install::uninstall(&cx, &mut state, name) {
                Ok(pkg) => removed.push(Package::from(&pkg)),
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
    pub fn changelog(&self, package: String, version: Option<String>) -> Result<Changelog> {
        let op = self.operation()?;
        let cx = self.ctx(&op);
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

    /// Every `ketch doctor` check. Repairs nothing.
    pub fn doctor(&self) -> Result<Vec<Check>> {
        let op = self.operation()?;
        let cx = self.ctx(&op);
        Ok(ketch_core::doctor::checks(&cx)
            .iter()
            .map(Check::from)
            .collect())
    }
}

impl KetchCore {
    /// Configuration and log for one call, built afresh: the core keeps no
    /// configuration across operations, and neither does this.
    fn operation(&self) -> Result<Operation> {
        let forward = self
            .reporter
            .clone()
            .map(|r| Report::new(ForeignReporter(r)));
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
        Ok(Operation { cfg, report })
    }

    fn ctx<'a>(&'a self, op: &'a Operation) -> Ctx<'a> {
        let cx = Ctx::new(&op.cfg, &op.report);
        match &self.decider {
            Some(decider) => cx.with_decider(decider),
            None => cx,
        }
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
        let core = KetchCore::new(Some(dir.path().display().to_string()), None, None);
        (dir, core)
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
            core.uninstall(vec!["nope".into()]),
            Err(KetchError::NotFound {
                name: "nope".into()
            })
        );
    }

    #[test]
    fn a_held_lock_makes_a_mutating_call_busy() {
        let (_dir, core) = scratch();
        let op = core.operation().unwrap();
        let _held = Lock::acquire(&Ctx::new(&op.cfg, &op.report)).unwrap();
        let err = core.uninstall(vec!["nope".into()]).unwrap_err();
        assert!(matches!(err, KetchError::Busy { .. }), "{err:?}");
    }

    #[derive(Default)]
    struct Recorded(std::sync::Mutex<Vec<Event>>);

    impl Reporter for Arc<Recorded> {
        fn event(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn a_local_package_installs_reports_and_uninstalls() {
        let dir = tempfile::tempdir().unwrap();
        let payload = dir.path().join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join("hello"), "#!/bin/sh\necho hi\n").unwrap();
        let recorded = Arc::new(Recorded::default());
        let core = KetchCore::new(
            Some(dir.path().join("root").display().to_string()),
            Some(Box::new(recorded.clone())),
            None,
        );

        let spec = format!("local:{}", payload.join("hello").display());
        let placed = core
            .install(vec![spec], InstallOptions::default(), None)
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
        assert!(recorded.0.lock().unwrap().iter().any(|e| matches!(
            e,
            Event::Step {
                stage: Stage::Installing,
                ..
            }
        )));

        let removed = core.uninstall(vec![name.clone()]).unwrap();
        assert_eq!(removed[0].name, name);
        assert_eq!(core.installed().unwrap(), Vec::new());
    }

    #[test]
    fn a_cancelled_install_places_nothing() {
        let (dir, core) = scratch();
        let payload = dir.path().join("payload");
        std::fs::create_dir_all(&payload).unwrap();
        std::fs::write(payload.join("hello"), "#!/bin/sh\necho hi\n").unwrap();
        let token = CancelToken::new();
        token.cancel();
        let spec = format!("local:{}", payload.join("hello").display());
        let err = core
            .install(vec![spec], InstallOptions::default(), Some(token))
            .unwrap_err();
        assert_eq!(err, KetchError::Cancelled);
        assert_eq!(core.installed().unwrap(), Vec::new());
    }

    #[test]
    fn empty_requests_are_refused() {
        let (_dir, core) = scratch();
        assert!(core
            .install(Vec::new(), InstallOptions::default(), None)
            .is_err());
        assert!(core.search("  ".into(), 10).is_err());
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
}
