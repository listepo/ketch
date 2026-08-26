//! Commands that change what is installed.
//!
//! Each one takes the lock for the whole batch and writes `state.json` once at
//! the end, so an interrupted run leaves the file either fully old or fully new.

use crate::cli::{InstallArgs, NameArgs, UninstallArgs, UpgradeArgs};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::install::{self, InstallRequest, Installed};
use crate::model::{PackageSpec, VersionSpec};
use crate::source::SourceRegistry;
use crate::state::{Lock, State};
use crate::ui;

pub fn install(cfg: &Config, args: InstallArgs) -> Result<()> {
    if let Some(asset) = &args.asset {
        if args.packages.len() > 1 {
            return Err(Error::msg(
                "--asset names one file, so it can only be used with a single package",
            ));
        }
        // Naming an asset bypasses the check that stops ketch installing a
        // build for another platform, so it is confirmed rather than assumed.
        let question = format!(
            "install `{asset}` without checking it runs on {}?",
            cfg.target
        );
        if !args.yes && !ui::confirm(&question, false) {
            return Ok(());
        }
    }

    let _lock = Lock::acquire(cfg)?;
    let sources = SourceRegistry::load(cfg);
    let mut state = State::load(cfg)?;

    let single = args.packages.len() == 1;
    let mut done = 0usize;
    let mut failed: Vec<String> = Vec::new();

    for raw in &args.packages {
        let req = InstallRequest {
            spec: PackageSpec::parse(raw),
            force: args.force,
            prerelease: args.prerelease,
            link: !args.no_link,
            require_checksum: args.require_checksum || cfg.require_checksums,
            asset_override: args.asset.clone(),
        };
        match install::install(cfg, &sources, &mut state, &req) {
            Ok(out) => {
                done += 1;
                report(&out);
            }
            Err(e) if single => return Err(e),
            // One bad package must not discard the ones that already
            // succeeded, so the failure is held until the state file is saved.
            Err(e) => {
                ui::error(&e);
                failed.push(raw.clone());
            }
        }
    }

    if done > 0 {
        state.save(cfg)?;
        path_hint(cfg, &state);
    }
    if !failed.is_empty() {
        return Err(Error::msg(format!(
            "{} of {} packages failed: {}",
            failed.len(),
            args.packages.len(),
            failed.join(", ")
        )));
    }
    Ok(())
}

pub fn uninstall(cfg: &Config, args: UninstallArgs) -> Result<()> {
    let _lock = Lock::acquire(cfg)?;
    let mut state = State::load(cfg)?;

    // Resolve every name up front: a typo should stop the command before it
    // has already removed the packages that did match.
    let mut targets: Vec<String> = Vec::new();
    for name in &args.names {
        let found = state
            .find(name)
            .ok_or_else(|| Error::NotInstalled(name.clone()))?
            .name
            .clone();
        if !targets.contains(&found) {
            targets.push(found);
        }
    }

    if !args.yes && !ui::confirm(&format!("remove {}?", targets.join(", ")), false) {
        return Ok(());
    }

    let mut removed = 0usize;
    for name in &targets {
        match install::uninstall(cfg, &mut state, name) {
            Ok(pkg) => {
                removed += 1;
                ui::success("removed", &format!("{} {}", pkg.name, pkg.version));
            }
            Err(e) => ui::error(&e),
        }
    }
    if removed > 0 {
        state.save(cfg)?;
    }
    if removed < targets.len() {
        return Err(Error::msg(format!(
            "removed {removed} of {} packages",
            targets.len()
        )));
    }
    Ok(())
}

pub fn upgrade(cfg: &Config, args: UpgradeArgs) -> Result<()> {
    let _lock = Lock::acquire(cfg)?;
    let sources = SourceRegistry::load(cfg);
    let mut state = State::load(cfg)?;

    let names = select(&state, &args.names)?;
    if names.is_empty() {
        ui::out("nothing installed");
        return Ok(());
    }

    let prerelease = args.prerelease || cfg.prerelease;
    let mut plan = Vec::new();
    let (mut checked, mut unreachable) = (0usize, 0usize);
    for name in &names {
        let pkg = match state.get(name) {
            Some(p) => p.clone(),
            None => continue,
        };
        if pkg.pinned && !args.force {
            ui::debug(&format!("{} is pinned at {}", pkg.name, pkg.version));
            continue;
        }
        ui::step("checking", &pkg.name);
        let release = match install::latest_release(&sources, &pkg, prerelease) {
            Ok(r) => r,
            // An unreachable source for one package must not abandon the rest.
            Err(e) => {
                ui::warn(&format!("{}: {e}", pkg.name));
                unreachable += 1;
                continue;
            }
        };
        checked += 1;
        // Compare versions, not tags: a retagged release is not an upgrade,
        // and neither is a source that briefly reports an older one.
        if release.tag == pkg.tag || release.version <= pkg.version {
            continue;
        }
        plan.push((pkg, release));
    }

    if plan.is_empty() {
        // "Up to date" is a claim about versions we actually saw. With nothing
        // checked we do not know, and exiting 0 tells a script the opposite.
        if checked == 0 && unreachable > 0 {
            return Err(Error::msg(format!(
                "could not check any of the {unreachable} packages; see the warnings above"
            )));
        }
        let detail = if unreachable > 0 {
            format!("{checked} packages ({unreachable} could not be checked)")
        } else {
            format!("{checked} packages")
        };
        ui::success("up to date", &detail);
        return Ok(());
    }

    let rows: Vec<Vec<String>> = plan
        .iter()
        .map(|(pkg, release)| {
            vec![
                pkg.name.clone(),
                pkg.version.to_string(),
                release.version.to_string(),
            ]
        })
        .collect();
    ui::table(&["package", "from", "to"], &rows);

    if args.dry_run {
        return Ok(());
    }
    if !args.yes && !ui::confirm(&format!("upgrade {} packages?", plan.len()), true) {
        return Ok(());
    }

    let mut done = 0usize;
    let mut failed = Vec::new();
    for (pkg, release) in plan {
        let req = InstallRequest {
            spec: PackageSpec {
                raw: format!("{}@{}", pkg.source, release.tag),
                reference: Some(pkg.source.clone()),
                alias: None,
                // The exact tag that was reported, so nothing can change
                // between the plan the user approved and what is installed.
                version: VersionSpec::Exact(release.tag.clone()),
            },
            force: true,
            prerelease,
            // A package installed with --no-link stays unlinked.
            link: !pkg.links.is_empty(),
            require_checksum: cfg.require_checksums,
            asset_override: None,
        };
        match install::install(cfg, &sources, &mut state, &req) {
            Ok(out) => {
                done += 1;
                report(&out);
            }
            Err(e) => {
                ui::error(&e);
                failed.push(pkg.name.clone());
            }
        }
    }

    if done > 0 {
        state.save(cfg)?;
    }
    if !failed.is_empty() {
        return Err(Error::msg(format!(
            "failed to upgrade {}",
            failed.join(", ")
        )));
    }
    Ok(())
}

/// `pin` and `unpin` — `pinned` selects which.
pub fn pin(cfg: &Config, args: NameArgs, pinned: bool) -> Result<()> {
    let _lock = Lock::acquire(cfg)?;
    let mut state = State::load(cfg)?;

    for name in select(&state, &args.names)? {
        let Some(entry) = state.get_mut(&name) else {
            continue;
        };
        entry.pinned = pinned;
        ui::success(
            if pinned { "pinned" } else { "unpinned" },
            &format!("{} {}", entry.name, entry.version),
        );
    }
    state.save(cfg)
}

/// `link` and `unlink` — `linked` selects which.
pub fn link(cfg: &Config, args: NameArgs, linked: bool) -> Result<()> {
    let _lock = Lock::acquire(cfg)?;
    let mut state = State::load(cfg)?;

    for name in select(&state, &args.names)? {
        if linked {
            install::relink(cfg, &mut state, &name)?;
            ui::success("linked", &name);
        } else {
            install::unlink(cfg, &mut state, &name)?;
            ui::success("unlinked", &name);
        }
    }
    state.save(cfg)?;
    if linked {
        path_hint(cfg, &state);
    }
    Ok(())
}

/// Turn user-supplied names into installed package names. An empty list means
/// every installed package, which is what the bare `upgrade` form wants.
fn select(state: &State, names: &[String]) -> Result<Vec<String>> {
    if names.is_empty() {
        return Ok(state.iter().map(|p| p.name.clone()).collect());
    }
    names
        .iter()
        .map(|n| {
            state
                .find(n)
                .map(|p| p.name.clone())
                .ok_or_else(|| Error::NotInstalled(n.clone()))
        })
        .collect()
}

fn report(out: &Installed) {
    let pkg = &out.package;
    let detail = match &out.replaced {
        Some(old) if old != &pkg.version => format!("{} {} (was {old})", pkg.name, pkg.version),
        _ => format!("{} {}", pkg.name, pkg.version),
    };
    ui::success("installed", &detail);

    for link in pkg.binaries() {
        ui::debug(&format!("linked {}", link.link.display()));
    }
    if !pkg.checksum_verified {
        ui::warn(&format!(
            "{} published no checksum; trusting {} on first use",
            pkg.name,
            &pkg.sha256[..12]
        ));
    }
    if let Some(notes) = pkg.manifest.as_ref().and_then(|m| m.notes.as_deref()) {
        ui::out(notes);
    }
}

/// Say so once, at the end, when the links we just made are not reachable.
fn path_hint(cfg: &Config, state: &State) {
    if cfg.bin_dir_on_path() || !state.iter().any(|p| p.binaries().next().is_some()) {
        return;
    }
    ui::warn(&format!("{} is not on your PATH", cfg.bin_dir.display()));
    if crate::shell::configured_in(cfg).is_empty() {
        ui::out("Run `ketch path install` to add it.");
    } else {
        // Already in the startup file: this shell just predates the edit, and
        // telling them to install again would not change that.
        ui::out("It is in your shell config already — open a new shell.");
    }
}
