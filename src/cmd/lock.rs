//! `ketch lock` and `ketch sync`: writing a reproducible record of what is
//! installed, and making a machine match one.
//!
//! Thin, like every other command body. The file's shape and its checks live
//! in `lockfile.rs`; the installing is the same pipeline `install` uses.

use crate::cli::{LockArgs, SyncArgs};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::install::{self, InstallRequest};
use crate::lockfile::{self, LockedPackage, Lockfile, Plan};
use crate::manifest::Resolver;
use crate::model::{PackageSpec, VersionSpec};
use crate::source::SourceRegistry;
use crate::state::{Lock, State};
use crate::ui;

/// Write the lockfile, or compare it against the tree.
pub fn lock(cfg: &Config, args: LockArgs) -> Result<()> {
    let path = lockfile::path(args.file.as_deref());
    let state = State::load(cfg)?;

    if args.check {
        let plan = lockfile::plan(&Lockfile::load(&path)?, &state);
        report_plan(&plan);
        if !plan.is_clean(false) {
            return Err(Error::msg(format!(
                "{} does not describe what is installed; run `ketch sync` or `ketch lock`",
                path.display()
            )));
        }
        ui::success("clean", &format!("{} matches", path.display()));
        return Ok(());
    }

    let lock = Lockfile::from_state(&state);
    lock.save(&path)?;
    ui::success(
        "wrote",
        &format!("{} ({} packages)", path.display(), lock.packages.len()),
    );
    Ok(())
}

/// Bring the tree in line with the lockfile.
pub fn sync(cfg: &Config, args: SyncArgs) -> Result<()> {
    let path = lockfile::path(args.file.as_deref());
    let lock = Lockfile::load(&path)?;

    let _guard = Lock::acquire(cfg)?;
    let sources = SourceRegistry::load(cfg);
    let mut state = State::load(cfg)?;
    let plan = lockfile::plan(&lock, &state);

    if plan.is_clean(args.prune) {
        ui::success(
            "in sync",
            &format!("{} packages match {}", plan.matched, path.display()),
        );
        return Ok(());
    }

    report_plan(&plan);
    if args.dry_run {
        return Ok(());
    }
    // Removing something the lockfile merely fails to mention is the one part
    // of a sync that can lose work, so it is asked about rather than assumed.
    let prune = args.prune
        && !plan.extra.is_empty()
        && (args.yes
            || ui::confirm(
                &format!("remove {} packages not in the lockfile?", plan.extra.len()),
                false,
            ));

    let target = cfg.target.to_string();
    let wanted: Vec<LockedPackage> = plan
        .missing
        .iter()
        .cloned()
        .chain(plan.changed.iter().map(|(entry, _)| entry.clone()))
        .collect();

    let mut failed: Vec<String> = Vec::new();
    let mut done = 0usize;

    for entry in &wanted {
        match install_locked(cfg, &sources, &mut state, entry, &target) {
            Ok(()) => done += 1,
            Err(e) => {
                ui::error(&e);
                failed.push(entry.name.clone());
            }
        }
    }

    if prune {
        for name in &plan.extra {
            match install::uninstall(cfg, &mut state, name) {
                Ok(pkg) => {
                    done += 1;
                    ui::success("removed", &format!("{} {}", pkg.name, pkg.version));
                }
                Err(e) => {
                    ui::error(&e);
                    failed.push(name.clone());
                }
            }
        }
    }

    // Saved even on a partial failure, so the file matches what is on disk.
    if done > 0 {
        state.save(cfg)?;
    }
    if !failed.is_empty() {
        return Err(Error::msg(format!(
            "{} of {} packages failed: {}",
            failed.len(),
            wanted.len() + plan.extra.len(),
            failed.join(", ")
        )));
    }
    ui::success("synced", &format!("{done} packages"));
    Ok(())
}

/// Install one locked entry at exactly the tag recorded for it.
fn install_locked(
    cfg: &Config,
    sources: &SourceRegistry,
    state: &mut State,
    entry: &LockedPackage,
    target: &str,
) -> Result<()> {
    let mut req = InstallRequest::new(spec_for(cfg, entry)?);
    req.require_checksum = cfg.require_checksums;
    // The recorded hash describes an asset for the machine that wrote the
    // lock. Somewhere else it names a file this host cannot even run, so
    // holding the download to it would fail every cross-platform sync.
    if entry.matches_target(target) {
        req.expected_sha256 = Some(entry.sha256.clone());
    } else {
        ui::debug(&format!(
            "{}: locked on {}, re-selecting the asset for {target}",
            entry.name, entry.target
        ));
    }

    let out = install::install(cfg, sources, state, &req)?;
    // A pin is part of what the lockfile records, so restore it rather than
    // leaving the fresh install unpinned and quietly upgradeable.
    if let Some(installed) = state.get_mut(&out.package.name) {
        installed.pinned = entry.pinned;
    }
    ui::success(
        "installed",
        &format!("{} {}", out.package.name, out.package.version),
    );
    Ok(())
}

/// How to ask for a locked package.
///
/// The name goes first because that is how the package was found originally,
/// and which manifest tier answers decides what gets linked and under what
/// names — resolving `github:BurntSushi/ripgrep` straight from the source
/// would fall through to inference and could expose different binaries than
/// the registry entry the user actually installed.
///
/// It is used only while it still means the same project. A name that now
/// resolves somewhere else must not quietly install that instead, so the
/// source the lock recorded wins.
fn spec_for(cfg: &Config, entry: &LockedPackage) -> Result<PackageSpec> {
    let version = VersionSpec::Exact(entry.tag.clone());
    let by_name = PackageSpec {
        raw: entry.name.clone(),
        reference: None,
        alias: Some(entry.name.to_ascii_lowercase()),
        version: version.clone(),
    };
    if let Ok(resolver) = Resolver::new(cfg) {
        if let Ok((manifest, _)) = resolver.resolve(&by_name) {
            if manifest.source == entry.source {
                return Ok(by_name);
            }
        }
    }
    Ok(PackageSpec {
        raw: entry.source.to_string(),
        reference: Some(entry.source.clone()),
        alias: None,
        version,
    })
}

fn report_plan(plan: &Plan) {
    for entry in &plan.missing {
        ui::out(&format!(
            "{} {} {}",
            ui::green("+"),
            entry.name,
            ui::dim(&entry.tag)
        ));
    }
    for (entry, have) in &plan.changed {
        ui::out(&format!(
            "{} {} {}",
            ui::yellow("~"),
            entry.name,
            ui::dim(&format!("{have} -> {}", entry.tag))
        ));
    }
    for name in &plan.extra {
        ui::out(&format!(
            "{} {} {}",
            ui::red("-"),
            name,
            ui::dim("not in the lockfile")
        ));
    }
    if plan.matched > 0 {
        ui::out(&ui::dim(&format!("{} already match", plan.matched)));
    }
}
