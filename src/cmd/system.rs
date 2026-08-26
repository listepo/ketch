//! Commands about ketch itself and its environment.

use crate::cli::{PluginCommand, SelfCommand};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::platform::{self, worst_status, CheckStatus, DoctorCheck};
use crate::registry;
use crate::selfupdate;
use crate::source::{plugin, Source};
use crate::state::State;
use crate::ui;

pub fn doctor(cfg: &Config) -> Result<()> {
    let mut checks = vec![DoctorCheck::ok(
        "version",
        format!("ketch {} for {}", selfupdate::current_version(), cfg.target),
    )];

    match platform::host() {
        Ok(host) => checks.extend(host.doctor(cfg)),
        Err(e) => checks.push(DoctorCheck::fail(
            "platform",
            e.to_string(),
            "This build of ketch does not support this operating system.",
        )),
    }
    checks.push(registry_check(cfg));
    checks.extend(store_checks(cfg));

    for check in &checks {
        let (mark, name) = match check.status {
            CheckStatus::Ok => (ui::green("ok  "), ui::dim(&check.name)),
            CheckStatus::Warn => (ui::yellow("warn"), ui::bold(&check.name)),
            CheckStatus::Fail => (ui::red("fail"), ui::bold(&check.name)),
        };
        ui::out(&format!("{mark} {name}  {}", check.detail));
        // The fix belongs with the problem, not in a summary the user has to
        // map back onto the list.
        if let Some(fix) = &check.fix {
            ui::out(&format!("     {}", ui::dim(fix)));
        }
    }

    if worst_status(&checks) == CheckStatus::Fail {
        let failed = checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count();
        return Err(Error::msg(format!("{failed} checks failed")));
    }
    Ok(())
}

/// Refresh the local copy of the package registry.
pub fn update(cfg: &Config) -> Result<()> {
    let count = registry::update(cfg)?;
    ui::success(
        "updated",
        &format!("{count} packages from {}", cfg.registry),
    );
    Ok(())
}

fn registry_check(cfg: &Config) -> DoctorCheck {
    if !registry::exists(cfg) {
        return DoctorCheck::warn(
            "registry",
            format!("no local copy of {}", cfg.registry),
            "Run `ketch update`.",
        );
    }
    DoctorCheck::ok(
        "registry",
        format!(
            "{} packages from {}",
            registry::load(cfg).len(),
            cfg.registry
        ),
    )
}

/// Everything ketch itself owns: the store matches the state file, and every
/// link still points at the package that claims it.
fn store_checks(cfg: &Config) -> Vec<DoctorCheck> {
    let state = match State::load(cfg) {
        Ok(s) => s,
        Err(e) => {
            return vec![DoctorCheck::fail(
                "state",
                e.to_string(),
                format!("Inspect or remove {}.", cfg.state_file.display()),
            )]
        }
    };

    let mut checks = Vec::new();
    let mut missing_payloads = Vec::new();
    let mut broken_links = Vec::new();
    for pkg in state.iter() {
        if !pkg.prefix.exists() {
            missing_payloads.push(pkg.name.clone());
            // Its links cannot be sound either; one message per package is enough.
            continue;
        }
        for link in &pkg.links {
            // `exists` follows symlinks, so this catches both a deleted link and
            // one left dangling by a manual removal inside the store.
            if !link.link.exists() {
                broken_links.push(format!("{} -> {}", pkg.name, link.link.display()));
            }
        }
    }

    checks.push(match missing_payloads.len() {
        0 => DoctorCheck::ok("packages", format!("{} installed", state.iter().count())),
        n => DoctorCheck::fail(
            "packages",
            format!(
                "{n} packages have no files: {}",
                missing_payloads.join(", ")
            ),
            format!(
                "Run `ketch install --force {}`.",
                missing_payloads.join(" ")
            ),
        ),
    });

    if !broken_links.is_empty() {
        checks.push(DoctorCheck::warn(
            "links",
            format!(
                "{} broken links: {}",
                broken_links.len(),
                broken_links.join(", ")
            ),
            "Run `ketch link <pkg>` to recreate them.",
        ));
    }

    checks
}

pub fn plugin(cfg: &Config, command: PluginCommand) -> Result<()> {
    match command {
        PluginCommand::Dir => {
            ui::out(&cfg.plugin_dir.display().to_string());
            Ok(())
        }
        PluginCommand::List { json } => {
            let mut rows = Vec::new();
            let mut found = Vec::new();
            for result in plugin::discover(cfg) {
                match result {
                    Ok(p) => {
                        rows.push(vec![
                            p.scheme().to_string(),
                            p.name().to_string(),
                            p.path().display().to_string(),
                        ]);
                        found.push(serde_json::json!({
                            "scheme": p.scheme(),
                            "name": p.name(),
                            "path": p.path(),
                            "protocol": plugin::PROTOCOL_VERSION,
                        }));
                    }
                    // A plugin ketch cannot speak to is still worth naming: the
                    // alternative is a scheme that silently does not exist.
                    Err(e) => ui::warn(&e.to_string()),
                }
            }
            if json {
                let text = serde_json::to_string_pretty(&found)
                    .map_err(|e| Error::parse("json output", e.to_string()))?;
                ui::out(&text);
            } else if rows.is_empty() {
                ui::out(&format!("no plugins in {}", cfg.plugin_dir.display()));
            } else {
                ui::table(&["scheme", "plugin", "path"], &rows);
            }
            Ok(())
        }
    }
}

pub fn zelf(cfg: &Config, command: SelfCommand) -> Result<()> {
    match command {
        SelfCommand::Version => {
            ui::out(&format!("ketch {}", selfupdate::current_version()));
            ui::out(&format!("target {}", cfg.target));
            ui::out(&format!("root   {}", cfg.root.display()));
            if let Ok(exe) = selfupdate::current_exe() {
                ui::out(&format!("binary {}", exe.display()));
            }
            Ok(())
        }
        SelfCommand::Update { dry_run, force } => {
            let out = selfupdate::update(cfg, force, dry_run)?;
            if !out.replaced {
                let verb = if dry_run {
                    "would update"
                } else {
                    "already current"
                };
                ui::success(verb, &format!("{} -> {}", out.from, out.to));
            } else {
                ui::success("updated", &format!("{} -> {}", out.from, out.to));
            }
            if let Some(notes) = &out.notes {
                ui::out(notes);
            }
            Ok(())
        }
        SelfCommand::Uninstall { purge, yes } => {
            let question = if purge {
                "remove ketch and everything it installed?"
            } else {
                "remove ketch itself? (installed packages are kept)"
            };
            if !yes && !ui::confirm(question, false) {
                return Ok(());
            }
            for path in selfupdate::uninstall_self(cfg, purge)? {
                ui::success("removed", &path.display().to_string());
            }
            Ok(())
        }
    }
}
