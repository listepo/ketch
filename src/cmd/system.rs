//! Commands about ketch itself and its environment.

use crate::cli::{
    CompletionsArgs, DoctorArgs, ManArgs, PathArgs, PathCommand, PathInstallArgs, PluginCommand,
    SelfCommand,
};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::platform::{worst_status, CheckStatus, DoctorCheck};
use crate::registry;
use crate::self_update;
use crate::shell::{self, Outcome, Shell};
use crate::source::{plugin, Source};
use crate::state::State;
use crate::ui;

pub fn doctor(cfg: &Config, args: DoctorArgs) -> Result<()> {
    if args.fix {
        fix(cfg);
    }

    let checks = crate::doctor::checks(&crate::ui::ctx(cfg));

    if args.json {
        print_doctor_json(&checks)?;
    } else {
        for check in &checks {
            let (mark, name) = match check.status {
                CheckStatus::Ok => (ui::tone(ui::Tone::Success, "ok  "), ui::dim(&check.name)),
                CheckStatus::Warn => (ui::tone(ui::Tone::Warning, "warn"), ui::bold(&check.name)),
                CheckStatus::Fail => (ui::tone(ui::Tone::Error, "fail"), ui::bold(&check.name)),
            };
            ui::out(&format!(
                "{mark} {name}  {}",
                crate::changelog::sanitize(&check.detail)
            ));
            // The fix belongs with the problem, not in a summary the user has to
            // map back onto the list.
            if let Some(fix) = &check.fix {
                ui::out(&format!(
                    "     {}",
                    ui::dim(&crate::changelog::sanitize(fix))
                ));
            }
        }
    }

    let failed = crate::doctor::failed(&checks);
    if failed > 0 {
        return Err(Error::msg(format!("{failed} checks failed")));
    }
    Ok(())
}

fn print_doctor_json(checks: &[DoctorCheck]) -> Result<()> {
    let text = serde_json::to_string_pretty(&doctor_report(checks))
        .map_err(|e| Error::parse("json output".to_string(), e.to_string()))?;
    ui::out(&text);
    Ok(())
}

fn doctor_report(checks: &[DoctorCheck]) -> serde_json::Value {
    fn status_name(status: CheckStatus) -> &'static str {
        match status {
            CheckStatus::Ok => "ok",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
    serde_json::json!({
        "status": status_name(worst_status(checks)),
        "checks": checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "status": status_name(c.status),
                    "detail": c.detail,
                    "fix": c.fix,
                })
            })
            .collect::<Vec<_>>(),
    })
}

/// Repair what `doctor` can repair on its own.
///
/// Only the PATH setup qualifies today: it needs no network and no choice from
/// the user. Everything else doctor reports either is already a one-line
/// command or needs a decision ketch has no business making, and a `--fix`
/// that quietly reinstalls packages would be a worse tool than one that says
/// what to run.
///
/// Failures are warned about rather than returned: `doctor` exists to finish
/// its report even when part of the machine is broken.
fn fix(cfg: &Config) {
    if cfg.bin_dir_on_path()
        || !shell::configured_in(cfg).is_empty()
        || shell::user_path_configured(cfg)
    {
        return;
    }
    #[cfg(windows)]
    {
        match shell::install_user(cfg, false) {
            Ok(outcome) => report_user(outcome, false),
            Err(e) => ui::warn(&e.to_string()),
        }
    }
    #[cfg(not(windows))]
    {
        let shells = match shell::detect() {
            Ok(shells) if !shells.is_empty() => shells,
            Ok(_) => {
                ui::warn(
                    "could not tell which shell you use; run `ketch path install --shell <name>`",
                );
                return;
            }
            Err(e) => {
                ui::warn(&e.to_string());
                return;
            }
        };
        for sh in shells {
            match shell::install(cfg, sh, false) {
                Ok(change) => report(&change, false),
                Err(e) => ui::warn(&format!("{}: {e}", sh.name())),
            }
        }
    }
}

/// `ketch man --out <dir>`: every page, for a package to ship.
pub fn man(args: &ManArgs) -> Result<()> {
    let written = crate::man::write_to(&args.out)?;
    ui::success(
        "Wrote",
        &format!("{} man pages to {}", written.len(), args.out.display()),
    );
    Ok(())
}

/// Write one shell's completion script into the platform destination and
/// record it on the installed `ketch` package so uninstall can take it back.
pub fn install_completions(cfg: &Config, args: CompletionsArgs) -> Result<()> {
    crate::self_update::install_completion_script(
        &crate::ui::ctx(cfg),
        args.shell,
        crate::self_docs::SELF_DOCS,
    )
}

/// Refresh the local copy of the package registry.
pub fn update(cfg: &Config) -> Result<()> {
    let count = match registry::update(&crate::ui::ctx(cfg)) {
        Ok(count) => count,
        Err(error) => {
            ui::completed("registry", false);
            return Err(error);
        }
    };
    ui::completed("registry", true);
    ui::success(
        "updated",
        &format!("{count} packages from {}", cfg.registry),
    );
    Ok(())
}

/// Refresh the registry when `auto_update` is on. A failed fetch is a warning:
/// install and upgrade still proceed with whatever copy is already on disk.
pub(crate) fn maybe_auto_update(cfg: &Config) {
    if !cfg.auto_update {
        return;
    }
    ui::step("auto-update", "enabled");
    if let Err(error) = update(cfg) {
        ui::warn(&format!("{error}"));
    }
}

/// `ketch path` — the shell setup the rest of ketch only ever hints at.
///
/// This is the one command that writes outside the ketch root, which is why it
/// is a command at all rather than something `install` does behind the user's
/// back.
pub fn path(cfg: &Config, command: Option<PathCommand>) -> Result<()> {
    match command.unwrap_or(PathCommand::Status) {
        PathCommand::Status => status(cfg),
        PathCommand::Install(args) => install(cfg, args),
        PathCommand::Uninstall(args) => uninstall(cfg, args),
    }
}

fn status(cfg: &Config) -> Result<()> {
    let check = shell::path_check(cfg);
    ui::out(&format!("{}  {}", ui::bold("bin"), cfg.bin_dir.display()));
    ui::out(&format!(
        "{}  {}",
        ui::bold("now"),
        if cfg.bin_dir_on_path() {
            "on PATH".to_string()
        } else {
            check.detail
        }
    ));

    let detected = shell::detect().unwrap_or_default();
    let configured = shell::configured_in(cfg);
    let mut rows = Vec::new();
    if cfg!(windows) {
        rows.push(vec![
            "user PATH".to_string(),
            if shell::user_path_configured(cfg) {
                "configured".to_string()
            } else {
                "not set up".to_string()
            },
            r"HKCU\Environment\Path".to_string(),
        ]);
    }
    for sh in Shell::ALL {
        let file = shell_file(sh)?;
        let state = if configured.contains(&file) {
            "configured"
        } else if detected.contains(&sh) {
            "not set up"
        } else {
            "not in use"
        };
        rows.push(vec![
            sh.name().to_string(),
            state.to_string(),
            file.display().to_string(),
        ]);
    }
    ui::table(&["shell", "state", "file"], &rows);
    Ok(())
}

fn install(cfg: &Config, args: PathInstallArgs) -> Result<()> {
    if args.print {
        ui::out(&shell::manual_line(cfg)?);
        return Ok(());
    }
    let mut changed = false;
    #[cfg(windows)]
    {
        let want_user = args.common.shell.is_empty() || args.common.all;
        if want_user {
            let outcome = shell::install_user(cfg, args.common.dry_run)?;
            changed |= outcome != Outcome::Unchanged;
            report_user(outcome, args.common.dry_run);
            if args.common.shell.is_empty() && !args.common.all {
                if changed && !args.common.dry_run {
                    ui::out("Open a new terminal to pick it up.");
                }
                return Ok(());
            }
        }
    }
    for sh in chosen(&args.common)? {
        let change = shell::install(cfg, sh, args.common.dry_run)?;
        changed |= change.outcome != Outcome::Unchanged;
        report(&change, args.common.dry_run);
    }
    if changed && !args.common.dry_run {
        ui::out("Open a new shell to pick it up.");
    }
    Ok(())
}

fn uninstall(cfg: &Config, args: PathArgs) -> Result<()> {
    #[cfg(not(windows))]
    let _ = cfg;
    #[cfg(windows)]
    {
        let want_user = args.shell.is_empty() || args.all;
        if want_user {
            match shell::uninstall_user(cfg, args.dry_run) {
                Ok(outcome) => report_user(outcome, args.dry_run),
                Err(e) => ui::warn(&e.to_string()),
            }
            if args.shell.is_empty() && !args.all {
                return Ok(());
            }
        }
    }
    for sh in chosen(&args)? {
        report(&shell::uninstall(sh, args.dry_run)?, args.dry_run);
    }
    Ok(())
}

/// Which shells this invocation acts on: what was asked for, or what the
/// machine looks like.
fn chosen(args: &PathArgs) -> Result<Vec<Shell>> {
    if args.all {
        return Ok(Shell::ALL.to_vec());
    }
    if !args.shell.is_empty() {
        let mut shells = args.shell.clone();
        shells.dedup();
        return Ok(shells);
    }
    let detected = shell::detect()?;
    if detected.is_empty() {
        // Guessing here would edit a startup file the user's shell never
        // reads, and they would have no reason to look for it.
        return Err(Error::msg(format!(
            "could not tell which shell you use (SHELL={}). \
             Pass --shell bash|zsh|fish, or --all, or `ketch path install --print` \
             for the line to add by hand.",
            std::env::var("SHELL").unwrap_or_else(|_| "unset".to_string())
        )));
    }
    Ok(detected)
}

/// Where a shell would be edited, without editing it.
fn shell_file(sh: Shell) -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| Error::msg("no home directory; set HOME"))?;
    Ok(sh.config_file(&home))
}

/// What `self uninstall` is about to delete, one line per kind of thing.
///
/// Printed before the question rather than after it: an answer to "remove
/// everything?" means nothing unless everything is on screen next to it.
fn plan_lines(plan: &self_update::UninstallPlan) -> Vec<String> {
    let mut lines = Vec::new();
    if !plan.packages.is_empty() {
        lines.push(format!("packages: {}", plan.packages.join(", ")));
    }
    if let Some(root) = &plan.root {
        lines.push(format!("{} and everything in it", root.display()));
    } else if let Some(exe) = &plan.exe {
        lines.push(exe.display().to_string());
    }
    for file in &plan.shell_files {
        lines.push(format!("the PATH block in {}", file.display()));
    }
    for entry in &plan.registry {
        lines.push(entry.describe().to_string());
    }
    for file in &plan.powershell_profiles {
        lines.push(format!("the completion block in {}", file.display()));
    }
    if plan.cask.is_some() {
        lines.push("the Homebrew cask".to_string());
    }
    lines
}

#[cfg(windows)]
fn report_user(outcome: Outcome, dry_run: bool) {
    let detail = r"user PATH (HKCU\Environment\Path)";
    match outcome {
        Outcome::Added if dry_run => ui::step("would add", detail),
        Outcome::Updated if dry_run => ui::step("would update", detail),
        Outcome::Removed if dry_run => ui::step("would remove", detail),
        Outcome::Added => ui::success("added", detail),
        Outcome::Updated => ui::success("updated", detail),
        Outcome::Removed => ui::success("removed", detail),
        Outcome::Unchanged => ui::step("unchanged", detail),
    }
}

fn report(change: &shell::Change, dry_run: bool) {
    let file = change.file.display().to_string();
    let detail = format!("{file} ({})", change.shell.name());
    match change.outcome {
        Outcome::Added if dry_run => ui::step("would add", &detail),
        Outcome::Updated if dry_run => ui::step("would update", &detail),
        Outcome::Removed if dry_run => ui::step("would remove", &detail),
        Outcome::Added => ui::success("added", &detail),
        Outcome::Updated => ui::success("updated", &detail),
        Outcome::Removed => ui::success("removed", &detail),
        Outcome::Unchanged => ui::step("unchanged", &detail),
    }
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
            for result in plugin::discover(&crate::ui::ctx(cfg)) {
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
        SelfCommand::Install { force, link_dir } => {
            let version = self_update::current_version();
            match self_update::install_self(
                &crate::ui::ctx(cfg),
                force,
                link_dir.as_deref(),
                crate::self_docs::SELF_DOCS,
            ) {
                Ok(out) => {
                    let detail = match &out.replaced {
                        Some(old) if old != &out.package.version => {
                            format!("ketch {version} (was {old})")
                        }
                        _ => format!("ketch {version}"),
                    };
                    ui::success("installed", &detail);
                }
                // Installers run this on every invocation; the second run is
                // a success, not a complaint.
                Err(Error::AlreadyInstalled { .. }) => {
                    ui::success("already installed", &format!("ketch {version}"));
                }
                Err(e) => return Err(e),
            }
            Ok(())
        }
        SelfCommand::Version => {
            ui::out(&format!("ketch {}", self_update::display_version()));
            ui::out(&format!("target {}", cfg.target));
            ui::out(&format!("root   {}", cfg.root.display()));
            if let Ok(exe) = self_update::current_exe() {
                ui::out(&format!("binary {}", exe.display()));
            }
            if let Some(on_path) = crate::doctor::first_ketch_on_path() {
                let linked = crate::doctor::store_ketch_link(cfg);
                if !crate::doctor::same_binary(&on_path, &linked) {
                    ui::out(&format!("PATH   {}", on_path.display()));
                }
            }
            Ok(())
        }
        SelfCommand::Upgrade {
            dry_run,
            force,
            yes,
        } => {
            if !dry_run {
                crate::process::offer_to_stop(
                    &self_replacement_paths(cfg),
                    yes,
                    &crate::ui::ctx_asking(cfg, !yes),
                );
            }
            let out = self_update::update(
                &crate::ui::ctx(cfg),
                force,
                dry_run,
                crate::self_docs::SELF_DOCS,
            )?;
            // `replaced` is false both when already current and on dry-run, so
            // the verb has to look at whether an upgrade is actually needed.
            let needs_update = out.to > out.from || force;
            let verb = if out.replaced {
                "upgraded"
            } else if dry_run && needs_update {
                "would upgrade"
            } else {
                "already current"
            };
            ui::success(verb, &format!("{} -> {}", out.from, out.to));
            if let Some(notes) = &out.notes {
                ui::out(&ui::printable(notes));
            }
            Ok(())
        }
        SelfCommand::Uninstall {
            keep_packages,
            no_brew,
            dry_run,
            yes,
        } => {
            let mut plan = self_update::uninstall_plan(cfg, keep_packages, no_brew)?;
            for line in plan_lines(&plan) {
                ui::step(
                    if dry_run {
                        "would remove"
                    } else {
                        "will remove"
                    },
                    &line,
                );
            }
            if let Some(mise) = &plan.mise {
                ui::step(
                    if dry_run { "would ask" } else { "will ask" },
                    &format!("to run `mise unuse -g {}`", mise.tool),
                );
            }
            if dry_run {
                return Ok(());
            }
            let question = if keep_packages {
                "remove ketch? this is permanent: nothing here can be recovered"
            } else {
                "remove ketch and every package it installed? this is permanent: \
                 all of this data is deleted for good, with no way to undo it"
            };
            if !yes && !ui::confirm(question, false) {
                return Ok(());
            }
            // A question of its own: mise's copy belongs to another tool's
            // config, and someone may want ketch's tree gone but mise's entry
            // kept — to reinstall from it, or because a project pins it.
            if let Some(mise) = plan.mise.take() {
                let run = format!("mise unuse -g {}", mise.tool);
                if yes || ui::confirm(&format!("also run `{run}`?"), false) {
                    plan.mise = Some(mise);
                } else {
                    ui::note(&format!("mise still has this ketch; `{run}` removes it"));
                }
            }
            for path in self_update::uninstall_self(&crate::ui::ctx(cfg), &plan)? {
                ui::success("removed", &path.display().to_string());
            }
            Ok(())
        }
    }
}

/// Binaries `self upgrade` may overwrite: the running image, the store link,
/// and any recorded links for the `ketch` package.
fn self_replacement_paths(cfg: &Config) -> Vec<std::path::PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = self_update::current_exe() {
        paths.push(exe);
    }
    paths.push(crate::doctor::store_ketch_link(cfg));
    if let Ok(state) = State::load(cfg) {
        if let Some(pkg) = state.get(self_update::SELF_NAME) {
            for link in &pkg.links {
                paths.push(link.link.clone());
                paths.push(link.target.clone());
            }
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_json_names_the_worst_status_and_each_check() {
        let checks = vec![
            DoctorCheck::ok("version", "ketch 0.3.2"),
            DoctorCheck::warn("links", "1 broken", "ketch link x"),
        ];
        let report = doctor_report(&checks);
        assert_eq!(report["status"], "warn");
        assert_eq!(report["checks"][0]["name"], "version");
        assert_eq!(report["checks"][0]["status"], "ok");
        assert_eq!(report["checks"][0]["fix"], serde_json::Value::Null);
        assert_eq!(report["checks"][1]["status"], "warn");
        assert_eq!(report["checks"][1]["fix"], "ketch link x");
    }

    #[test]
    fn doctor_json_fails_when_any_check_fails() {
        let checks = vec![
            DoctorCheck::ok("version", "ketch 0.3.2"),
            DoctorCheck::fail("packages", "1 missing", "ketch install --force x"),
        ];
        assert_eq!(doctor_report(&checks)["status"], "fail");
    }
}
