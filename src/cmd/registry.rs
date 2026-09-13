//! `ketch registry`: validate a registry tree, show the local copy, offer packages.
//!
//! `validate` is the fail-closed check registry CI runs over every package
//! folder — parse, collisions, and optional offline-install against a fixture.
//! `status` reports the local copy's age and source with no network. `push`
//! puts a project's `ketch.toml` in front of the people who curate the registry.

use crate::cli::RegistryCommand;
use crate::config::{self, Config};
use crate::error::{Error, Result};
use crate::install::{self, InstallRequest};
use crate::model::PackageSpec;
use crate::push;
use crate::registry;
use crate::source::SourceRegistry;
use crate::state::State;
use crate::ui;
use std::path::{Path, PathBuf};

/// Entry point for `ketch registry <command>`.
pub fn run(cfg: &Config, command: RegistryCommand) -> Result<()> {
    match command {
        RegistryCommand::Validate {
            dir,
            fixture,
            changed,
            json,
        } => validate(dir, fixture, changed, json),
        RegistryCommand::Status { json } => status(cfg, json),
        RegistryCommand::Push {
            file,
            registry,
            dry_run,
            yes,
        } => push(cfg, file, registry, dry_run, yes),
    }
}

/// `ketch registry validate`: every package folder, parsed and checked.
fn validate(
    dir: Option<PathBuf>,
    fixture: Option<PathBuf>,
    changed: Vec<String>,
    json: bool,
) -> Result<()> {
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let mut report = registry::check_tree(&dir);
    if fixture.is_none() && !changed.is_empty() {
        report.errors.push(registry::ValidationError {
            path: dir.display().to_string(),
            message: "offline-install of --changed packages needs --fixture".to_string(),
        });
    } else if let Some(fixture) = fixture {
        let extra = probe_fixtures(&dir, &fixture, &changed, &report.parsed);
        report.errors.extend(extra);
    }

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

/// `ketch registry status`: age and source of the local copy. No network.
fn status(cfg: &Config, json: bool) -> Result<()> {
    let packages = if registry::exists(cfg) {
        Some(registry::load(cfg).len())
    } else {
        None
    };
    let meta = match registry::load_meta(cfg) {
        Ok(meta) => meta,
        Err(error) => {
            if json {
                print_status_json("fail", cfg, None, packages, Some(&error.to_string()))?;
            }
            return Err(error);
        }
    };

    let kind = match (packages, meta.as_ref()) {
        (None, _) => "missing",
        (Some(_), None) => "unrecorded",
        (Some(_), Some(_)) => "ok",
    };

    if json {
        print_status_json(kind, cfg, meta.as_ref(), packages, None)?;
        return Ok(());
    }

    ui::out(&format!(
        "{}  {}",
        ui::bold("registry"),
        crate::changelog::sanitize(&cfg.registry)
    ));
    match (packages, meta.as_ref()) {
        (None, _) => ui::warn(&format!(
            "no local copy; run `ketch update` to fetch {}",
            cfg.registry
        )),
        (Some(n), None) => {
            ui::out(&format!("{}  {n}", ui::bold("packages")));
            ui::warn("fetch time unknown; run `ketch update`");
        }
        (Some(n), Some(meta)) => {
            let fetched = crate::log::timestamp(meta.fetched_at as i64);
            let age = registry::age_phrase(meta.fetched_at);
            if let Some(revision) = &meta.revision {
                ui::out(&format!(
                    "{}  {}",
                    ui::bold("revision"),
                    crate::changelog::sanitize(revision)
                ));
            }
            if let Some(etag) = &meta.etag {
                ui::out(&format!(
                    "{}  {}",
                    ui::bold("etag"),
                    crate::changelog::sanitize(etag)
                ));
            }
            ui::out(&format!("{}  {fetched} ({age})", ui::bold("fetched")));
            ui::out(&format!("{}  {n}", ui::bold("packages")));
        }
    }
    Ok(())
}

fn print_status_json(
    status: &str,
    cfg: &Config,
    meta: Option<&registry::UpdateMeta>,
    packages: Option<usize>,
    error: Option<&str>,
) -> Result<()> {
    let now = crate::model::now_unix();
    let value = serde_json::json!({
        "status": status,
        "repo": crate::changelog::sanitize(&cfg.registry),
        "revision": meta.and_then(|m| m.revision.as_ref()).map(|r| crate::changelog::sanitize(r)),
        "etag": meta.and_then(|m| m.etag.as_ref()).map(|e| crate::changelog::sanitize(e)),
        "fetched_at": meta.map(|m| m.fetched_at),
        "fetched": meta.map(|m| crate::log::timestamp(m.fetched_at as i64)),
        "age_seconds": meta.map(|m| now.saturating_sub(m.fetched_at)),
        "packages": packages,
        "error": error,
    });
    let text = serde_json::to_string_pretty(&value)
        .map_err(|e| Error::parse("json output".to_string(), e.to_string()))?;
    ui::out(&text);
    Ok(())
}

/// Offline-install selected registry entries from local assets into a throwaway root.
fn probe_fixtures(
    registry_dir: &Path,
    fixture: &Path,
    changed: &[String],
    parsed: &[(crate::model::Manifest, PathBuf)],
) -> Vec<registry::ValidationError> {
    if !fixture.is_dir() {
        return vec![registry::ValidationError {
            path: fixture.display().to_string(),
            message: "no such directory".to_string(),
        }];
    }

    let mut names: Vec<String> = if changed.is_empty() {
        parsed
            .iter()
            .map(|(m, _)| m.name.clone())
            .filter(|name| fixture.join(name).exists())
            .collect()
    } else {
        changed.to_vec()
    };
    names.sort();
    names.dedup();

    let mut errors = Vec::new();
    let by_name: std::collections::BTreeMap<&str, &crate::model::Manifest> =
        parsed.iter().map(|(m, _)| (m.name.as_str(), m)).collect();

    let probe = match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(e) => {
            return vec![registry::ValidationError {
                path: registry_dir.display().to_string(),
                message: format!("cannot create a throwaway root for offline-install: {e}"),
            }];
        }
    };
    let probe_cfg = match Config::load(Some(probe.path().to_path_buf())) {
        Ok(cfg) => cfg,
        Err(e) => {
            return vec![registry::ValidationError {
                path: registry_dir.display().to_string(),
                message: format!("cannot load a throwaway root for offline-install: {e}"),
            }];
        }
    };
    if let Err(e) = probe_cfg.ensure_dirs() {
        return vec![registry::ValidationError {
            path: registry_dir.display().to_string(),
            message: e.to_string(),
        }];
    }

    for name in names {
        let Some(manifest) = by_name.get(name.as_str()) else {
            errors.push(registry::ValidationError {
                path: registry_dir.display().to_string(),
                message: format!("no package named `{name}` to offline-install"),
            });
            continue;
        };
        let payload = match registry::fixture_payload(fixture, &name) {
            Ok(path) => path,
            Err(e) => {
                errors.push(registry::ValidationError {
                    path: fixture.join(&name).display().to_string(),
                    message: message_without_path(&e),
                });
                continue;
            }
        };
        if let Err(e) = install_from_fixture(&probe_cfg, manifest, &payload) {
            errors.push(registry::ValidationError {
                path: fixture.join(&name).display().to_string(),
                message: format!("offline-install failed: {e}"),
            });
        }
    }
    errors
}

fn message_without_path(error: &Error) -> String {
    error.to_string()
}

/// Install `manifest` from a local asset, using a throwaway ketch root.
fn install_from_fixture(
    cfg: &Config,
    manifest: &crate::model::Manifest,
    payload: &Path,
) -> Result<()> {
    let abs = crate::source::local::absolute_path(&payload.to_string_lossy())?;
    let mut rewritten = manifest.clone();
    rewritten.source = crate::source::local::package_ref(&abs);
    let path = crate::manifest::user_manifest_path(cfg, &rewritten.name);
    std::fs::write(&path, crate::manifest::to_toml(&rewritten)?)
        .map_err(|e| Error::io(&path, e))?;
    let sources = SourceRegistry::load(cfg);
    let mut state = State::default();
    install::install(
        cfg,
        &sources,
        &mut state,
        &InstallRequest::new(PackageSpec::parse(&rewritten.name)),
    )?;
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
