//! `ketch registry`: validate a registry tree and offer packages to it.
//!
//! `validate` is the fail-closed check registry CI runs over every package
//! folder. `push` puts a project's `ketch.toml` in front of the people who
//! curate the registry, as a pull request — but looks before it leaps. The
//! registry's current copy of the package file is fetched first, so an
//! update shows its diff and asks before the pull request is opened, and a
//! package the registry has never seen says so before it is added.

use crate::cli::RegistryCommand;
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::push;
use crate::registry;
use crate::ui;
use std::path::PathBuf;

/// Entry point for `ketch registry <command>`.
pub fn run(cfg: &Config, command: RegistryCommand) -> Result<()> {
    match command {
        RegistryCommand::Validate { dir, json } => validate(dir, json),
        RegistryCommand::Push {
            file,
            registry,
            dry_run,
            yes,
        } => push(cfg, file, registry, dry_run, yes),
    }
}

/// `ketch registry validate`: every package folder, parsed and checked.
fn validate(dir: Option<PathBuf>, json: bool) -> Result<()> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let report = registry::check_tree(&dir);

    if json {
        print_validate_json(&report)?;
    } else {
        for error in &report.errors {
            // One error per line, so a CI log can be grepped for them; the
            // paths and messages come from a tree someone else wrote, so they
            // pass through the same filter a changelog does. `--json` keeps the
            // original text for whoever wants it.
            ui::out(&one_line(&format!(
                "{}: {}",
                crate::changelog::sanitize(&error.path),
                crate::changelog::sanitize(&error.message),
            )));
        }
        if report.errors.is_empty() {
            ui::success("validated", &count(report.packages, "package"));
        }
    }

    // The errors themselves are the output a failing run exists to produce, so
    // they are already on stdout; this is what sets the exit code, and it is
    // the same sentence in both formats.
    if !report.errors.is_empty() {
        return Err(Error::msg(count(report.errors.len(), "validation error")));
    }
    Ok(())
}

/// Fold a message onto one line: a parse error can span several.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `1 package` / `2 packages`, for the two counts this command reports.
fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

fn print_validate_json(report: &registry::Report) -> Result<()> {
    let status = if report.errors.is_empty() {
        "ok"
    } else {
        "fail"
    };
    let errors = report
        .errors
        .iter()
        .map(|error| {
            // JSON is a machine format, but it is printed to a terminal too:
            // serde escapes control characters and not bidi overrides, and a
            // package file someone else wrote can carry either.
            serde_json::json!({
                "path": crate::changelog::sanitize(&error.path),
                "message": crate::changelog::sanitize(&error.message),
            })
        })
        .collect::<Vec<_>>();
    let value = serde_json::json!({
        "status": status,
        "packages": report.packages,
        "errors": errors,
    });
    let text = serde_json::to_string_pretty(&value)
        .map_err(|e| Error::parse("json output".to_string(), e.to_string()))?;
    ui::out(&text);
    Ok(())
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
            for line in review_diff(&existing.text, &proposal.body).lines() {
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

/// The diff shown before an update, safe to print.
///
/// The registry's copy is a file someone else wrote, on its way to this
/// terminal. An escape sequence in it could redraw the very review the user is
/// about to approve, so it goes through the same filter a changelog does.
fn review_diff(registry: &str, local: &str) -> String {
    crate::changelog::sanitize(&crate::diff::unified(registry, local))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_review_diff_drops_escape_sequences_from_the_registry_copy() {
        let registry = "name = \"tool\"\ndescription = \"\u{1b}[2J\u{1b}]0;x\u{7}hi\u{202e}\"\n";
        let shown = review_diff(registry, "name = \"tool\"\n");
        assert!(shown.contains("-description"), "{shown}");
        for bad in ['\u{1b}', '\u{7}', '\u{202e}'] {
            assert!(
                !shown.contains(bad),
                "{bad:?} reached the terminal: {shown:?}"
            );
        }
    }
}
