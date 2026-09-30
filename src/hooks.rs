//! Lifecycle hooks: the commands a manifest's `[hooks]` table runs around an
//! install, an update and an uninstall.
//!
//! Kept apart from `install.rs` because this is the one place ketch runs a
//! command a manifest wrote. That turns a manifest into a program, so hooks are
//! honoured only from the user's own manifest directory: a registry entry or a
//! built-in manifest is someone else's file, and installing a package must
//! never mean running their shell. `allowed` is that rule; `install.rs`
//! applies it before anything is placed or removed.

use crate::error::{Error, Result};
use crate::model::{Hooks, ManifestOrigin};
use crate::source::plugin::run_with_deadline;
use crate::ui;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// How long a hook may run. Long enough for a real setup step, short enough
/// that a hook waiting on something that never comes does not hold the ketch
/// lock for the rest of the day.
const HOOK_TIMEOUT: Duration = Duration::from_secs(600);

/// Which moment in a package's life a hook runs at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    BeforeInstall,
    AfterInstall,
    BeforeUpdate,
    AfterUpdate,
    BeforeUninstall,
    AfterUninstall,
}

impl Event {
    /// The manifest key, which is also what `KETCH_HOOK` carries.
    pub fn key(self) -> &'static str {
        match self {
            Event::BeforeInstall => "before_install",
            Event::AfterInstall => "after_install",
            Event::BeforeUpdate => "before_update",
            Event::AfterUpdate => "after_update",
            Event::BeforeUninstall => "before_uninstall",
            Event::AfterUninstall => "after_uninstall",
        }
    }

    fn command(self, hooks: &Hooks) -> Option<&str> {
        match self {
            Event::BeforeInstall => hooks.before_install.as_deref(),
            Event::AfterInstall => hooks.after_install.as_deref(),
            Event::BeforeUpdate => hooks.before_update.as_deref(),
            Event::AfterUpdate => hooks.after_update.as_deref(),
            Event::BeforeUninstall => hooks.before_uninstall.as_deref(),
            Event::AfterUninstall => hooks.after_uninstall.as_deref(),
        }
    }
}

/// What a hook is told about the package it runs for, as `KETCH_*` variables.
pub struct Context<'a> {
    pub name: &'a str,
    pub version: &'a str,
    /// The version being replaced; only update events have one.
    pub previous: Option<&'a str>,
    /// The package's store prefix. Missing before an install and after an
    /// uninstall, and the hook's working directory whenever it exists.
    pub prefix: &'a Path,
    pub bin_dir: &'a Path,
    pub root: &'a Path,
}

/// Whether a manifest of this origin may run hooks at all.
pub fn allowed(origin: &ManifestOrigin) -> bool {
    matches!(origin, ManifestOrigin::User(_))
}

/// The origin that would be refused, in the words the user needs to fix it.
pub fn refusal(origin: &ManifestOrigin, name: &str) -> Error {
    Error::msg(format!(
        "`{name}` has hooks in a {} manifest ({}); hooks run only from a manifest in your own \
         manifest directory — copy it there to opt in",
        origin.tier(),
        origin.location()
    ))
}

/// Run the hook for `event`, if the manifest has one. No hook is not an error;
/// a hook that exits non-zero is, and its stderr is the error's detail.
pub fn run(hooks: &Hooks, event: Event, ctx: &Context<'_>) -> Result<()> {
    run_with(hooks, event, ctx, HOOK_TIMEOUT)
}

fn run_with(hooks: &Hooks, event: Event, ctx: &Context<'_>, timeout: Duration) -> Result<()> {
    let Some(script) = event.command(hooks) else {
        return Ok(());
    };
    let label = format!("{} hook for {}", event.key(), ctx.name);
    ui::step("running", &label);
    let mut cmd = shell(script);
    cmd.env("KETCH_HOOK", event.key())
        .env("KETCH_PACKAGE", ctx.name)
        .env("KETCH_VERSION", ctx.version)
        .env("KETCH_PREFIX", ctx.prefix)
        .env("KETCH_BIN_DIR", ctx.bin_dir)
        .env("KETCH_ROOT", ctx.root);
    if let Some(previous) = ctx.previous {
        cmd.env("KETCH_PREVIOUS_VERSION", previous);
    }
    if ctx.prefix.is_dir() {
        cmd.current_dir(ctx.prefix);
    }
    // The same guards a source plugin gets: no stdin, bounded output, and a
    // deadline after which the whole process tree is killed. Output is
    // captured rather than inherited because a progress bar may own the
    // terminal, and the log should have it whether or not the user saw it.
    let (status, out, err) =
        run_with_deadline(&mut cmd, timeout).map_err(|(detail, err)| Error::Command {
            cmd: label.clone(),
            status: detail,
            stderr: String::from_utf8_lossy(&err).to_string(),
        })?;
    for line in String::from_utf8_lossy(&out).lines() {
        ui::debug(&format!("{}: {line}", event.key()));
    }
    if status.success() {
        return Ok(());
    }
    Err(Error::Command {
        cmd: label,
        status: status.to_string(),
        stderr: String::from_utf8_lossy(&err).to_string(),
    })
}

/// `run` for the `after_*` events: the operation has already happened and
/// `state` records it, so a hook failing now is reported, never propagated.
pub fn run_or_warn(hooks: &Hooks, event: Event, ctx: &Context<'_>) {
    if let Err(e) = run(hooks, event, ctx) {
        ui::warn(&e.to_string());
        for line in e.details() {
            ui::warn(&line);
        }
    }
}

/// The platform shell, so a hook is one line of what the user already types.
///
/// On Windows the script goes to `cmd /C` exactly as written. `Command`'s
/// usual escaping wraps it for `CommandLineToArgvW`, and `cmd.exe` then
/// strips those quotes by its own rules, which turns a quoted path in the
/// script into a filename Windows rejects.
fn shell(script: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("cmd");
        cmd.arg("/C");
        cmd.raw_arg(script);
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }
}

/// `shell` for the `hook_line` fuzz target (`src/lib.rs`).
#[cfg(fuzzing)]
pub(crate) fn fuzz_shell(script: &str) -> Command {
    shell(script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn hooks(after_install: &str) -> Hooks {
        Hooks {
            after_install: Some(after_install.to_string()),
            ..Hooks::default()
        }
    }

    fn context<'a>(prefix: &'a Path, previous: Option<&'a str>) -> Context<'a> {
        Context {
            name: "tool",
            version: "2.0.0",
            previous,
            prefix,
            bin_dir: Path::new("bin"),
            root: Path::new("root"),
        }
    }

    #[test]
    fn a_missing_hook_is_a_no_op() {
        let ctx = context(Path::new("nowhere"), None);
        assert!(run(&Hooks::default(), Event::AfterInstall, &ctx).is_ok());
        // Only the event asked for runs: an `after_install` hook is silent on uninstall.
        assert!(run(&hooks("exit 1"), Event::AfterUninstall, &ctx).is_ok());
    }

    #[test]
    fn a_hook_runs_in_the_prefix_with_the_package_in_its_environment() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path().join("prefix");
        std::fs::create_dir(&prefix).unwrap();
        let out = tmp.path().join("out");
        let script = if cfg!(windows) {
            format!(
                "(echo %KETCH_HOOK% %KETCH_PACKAGE% %KETCH_VERSION% %KETCH_PREVIOUS_VERSION% && cd) > \"{}\"",
                out.display()
            )
        } else {
            format!(
                "printf '%s %s %s %s\\n%s\\n' \"$KETCH_HOOK\" \"$KETCH_PACKAGE\" \"$KETCH_VERSION\" \"$KETCH_PREVIOUS_VERSION\" \"$PWD\" > '{}'",
                out.display()
            )
        };
        let hooks = Hooks {
            after_update: Some(script),
            ..Hooks::default()
        };
        run(&hooks, Event::AfterUpdate, &context(&prefix, Some("1.0.0"))).unwrap();
        let written = std::fs::read_to_string(&out).unwrap();
        let mut lines = written.lines();
        assert_eq!(
            lines.next().unwrap().trim(),
            "after_update tool 2.0.0 1.0.0"
        );
        let cwd = PathBuf::from(lines.next().unwrap().trim());
        assert_eq!(cwd.canonicalize().unwrap(), prefix.canonicalize().unwrap());
    }

    #[test]
    fn a_failing_hook_reports_its_stderr() {
        let script = if cfg!(windows) {
            "echo broken 1>&2 && exit 3"
        } else {
            "echo broken >&2; exit 3"
        };
        let err = run(
            &hooks(script),
            Event::AfterInstall,
            &context(Path::new("nowhere"), None),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("after_install hook for tool"),
            "{err}"
        );
        assert_eq!(err.details(), vec!["broken".to_string()]);
    }

    #[test]
    fn a_hook_that_hangs_is_stopped_at_the_deadline() {
        let script = if cfg!(windows) {
            "ping -n 30 127.0.0.1 > NUL"
        } else {
            "sleep 30"
        };
        let started = std::time::Instant::now();
        let err = run_with(
            &hooks(script),
            Event::AfterInstall,
            &context(Path::new("nowhere"), None),
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "waited the hook out instead of stopping it"
        );
        assert!(err.to_string().contains("was stopped"), "{err}");
    }

    #[test]
    fn only_the_users_own_manifests_may_run_hooks() {
        assert!(allowed(&ManifestOrigin::User(PathBuf::from("m/tool.toml"))));
        assert!(!allowed(&ManifestOrigin::Registry(PathBuf::from(
            "r/tool/ketch.toml"
        ))));
        assert!(!allowed(&ManifestOrigin::Builtin));
        assert!(!allowed(&ManifestOrigin::Inferred));
    }
}
