//! `ketch registry`: offering a package to the registry, with a review step.
//!
//! `push` puts a project's `ketch.toml` in front of the people who curate
//! the registry, as a pull request — but looks before it leaps. The
//! registry's current copy of the package file is fetched first, so an
//! update shows its diff and asks before the pull request is opened, and a
//! package the registry has never seen says so before it is added.

use crate::cli::RegistryCommand;
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::push;
use crate::ui;
use std::path::PathBuf;

/// Entry point for `ketch registry <command>`.
pub fn run(cfg: &Config, command: RegistryCommand) -> Result<()> {
    match command {
        RegistryCommand::Push {
            file,
            registry,
            dry_run,
            yes,
        } => push(cfg, file, registry, dry_run, yes),
        RegistryCommand::Validate { path } => validate(&path),
    }
}

/// `ketch registry validate`: every package folder in a checkout, checked the
/// way the client will check it, so a bad entry is caught before merge.
fn validate(_path: &std::path::Path) -> Result<()> {
    Err(Error::msg("`ketch registry validate` is being built"))
}

/// `ketch registry push`: this project's package file, as a registry pull
/// request — but looking before it leaps. The registry's current copy is
/// fetched first so each of the three situations can say what it is: a new
/// package announces itself and goes straight to the pull request, an
/// unchanged one opens nothing, and an update shows its diff and asks before
/// anything is sent.
///
/// The GitHub mechanics stay in [`crate::push`]; this body only sequences,
/// prints and asks.
fn push(
    cfg: &Config,
    file: Option<PathBuf>,
    registry: Option<String>,
    dry_run: bool,
    yes: bool,
) -> Result<()> {
    let file = file.unwrap_or_else(|| PathBuf::from(crate::registry::PACKAGE_FILE));
    let proposal = push::load(&file)?;
    let target = match registry {
        Some(repo) => config::validate_repo("registry", repo)?,
        None => cfg.registry.clone(),
    };
    let destination = format!(
        "{target}:{}/{}",
        proposal.name,
        crate::registry::PACKAGE_FILE
    );
    if dry_run {
        ui::step("would push", &destination);
        // The file ends with its own newline; `out` adds one, so strip it or
        // the dry run prints a blank line the real file does not have.
        ui::out(proposal.body.trim_end_matches('\n'));
        return Ok(());
    }
    let api = push::GitHub::new(cfg)?;
    let current = push::current(&api, &target, &proposal.name)?;
    match push::plan(current.as_ref(), &proposal) {
        push::Plan::Add => {
            ui::step("new package", &format!("{} @ {target}", proposal.name));
            report(
                &target,
                &proposal.name,
                push::open(&api, &target, &proposal)?,
            );
        }
        push::Plan::Unchanged => ui::success(
            "unchanged",
            &format!("{target} already has this {}", proposal.name),
        ),
        push::Plan::Update => {
            // `plan` reached Update through this very Option, so the `else`
            // arm cannot run; declining is the least surprising thing to do
            // with the impossible case.
            let Some(existing) = current.as_ref() else {
                return Ok(());
            };
            ui::out(&format!(
                "--- {target} {}/{} (registry)",
                proposal.name,
                crate::registry::PACKAGE_FILE
            ));
            ui::out(&format!("+++ {} (local)", file.display()));
            for line in crate::diff::unified(&existing.text, &proposal.body).lines() {
                if line.starts_with('-') {
                    ui::out(&ui::red(line));
                } else if line.starts_with('+') {
                    ui::out(&ui::green(line));
                } else {
                    ui::out(line);
                }
            }
            let question = format!("update {} in {target} with this change?", proposal.name);
            if !yes && !ui::confirm(&question, false) {
                return Ok(());
            }
            ui::step("pushing", &destination);
            report(
                &target,
                &proposal.name,
                push::open(&api, &target, &proposal)?,
            );
        }
    }
    Ok(())
}

/// The outcome lines the add and update paths share, so the two cannot drift
/// apart in what they say a pull request did.
fn report(target: &str, name: &str, outcome: push::Outcome) {
    match outcome {
        push::Outcome::Unchanged => {
            ui::success("unchanged", &format!("{target} already has this {name}"))
        }
        push::Outcome::Opened(pr) if pr.already_open => ui::success("already open", &pr.url),
        push::Outcome::Opened(pr) => ui::success("opened", &pr.url),
    }
}
