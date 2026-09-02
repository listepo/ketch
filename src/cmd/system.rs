//! Commands about ketch itself and its environment.

use crate::cli::{DoctorArgs, PathArgs, PathCommand, PathInstallArgs, PluginCommand, SelfCommand};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::platform::{self, worst_status, CheckStatus, DoctorCheck};
use crate::registry;
use crate::selfupdate;
use crate::shell::{self, Outcome, Shell};
use crate::source::{plugin, Source};
use crate::state::State;
use crate::ui;

pub fn doctor(cfg: &Config, args: DoctorArgs) -> Result<()> {
    if args.fix {
        fix(cfg);
    }

    let mut checks = vec![DoctorCheck::ok(
        "version",
        format!("ketch {} for {}", selfupdate::current_version(), cfg.target),
    )];

    // Not a platform check: every shell reads the same startup files wherever
    // it runs, so a second platform would only duplicate this.
    checks.push(shell::path_check(cfg));

    match platform::host() {
        Ok(host) => checks.extend(host.doctor(cfg)),
        Err(e) => checks.push(DoctorCheck::fail(
            "platform",
            e.to_string(),
            "This build of ketch does not support this operating system.",
        )),
    }
    checks.push(log_check(cfg));
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
    if cfg.bin_dir_on_path() || !shell::configured_in(cfg).is_empty() {
        return;
    }
    let shells = match shell::detect() {
        Ok(shells) if !shells.is_empty() => shells,
        Ok(_) => {
            ui::warn("could not tell which shell you use; run `ketch path install --shell <name>`");
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

/// Refresh the local copy of the package registry.
pub fn update(cfg: &Config) -> Result<()> {
    let count = match registry::update(cfg) {
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

/// Where this machine's log is, so nobody has to be told twice.
fn log_check(cfg: &Config) -> DoctorCheck {
    if cfg.log_level == crate::log::Level::Off {
        return DoctorCheck::ok("log", "off".to_string());
    }
    let size = std::fs::metadata(&cfg.log_file)
        .map(|m| format!(" · {}", ui::bytes(m.len())))
        .unwrap_or_default();
    DoctorCheck::ok(
        "log",
        format!(
            "{} ({}, {}){size}",
            cfg.log_file.display(),
            cfg.log_level,
            cfg.log_format
        ),
    )
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

/// `ketch path` — the shell setup the rest of ketch only ever hints at.
///
/// This is the one command that writes outside the ketch root, which is why it
/// is a command at all rather than something `install` does behind the user's
/// back.
pub fn path(cfg: &Config, command: Option<PathCommand>) -> Result<()> {
    match command.unwrap_or(PathCommand::Status) {
        PathCommand::Status => status(cfg),
        PathCommand::Install(args) => install(cfg, args),
        PathCommand::Uninstall(args) => uninstall(args),
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

fn uninstall(args: PathArgs) -> Result<()> {
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
