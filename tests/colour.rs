//! When the real binary paints its status lines, and when it must not.
//!
//! The line shapes themselves are snapshotted beside `src/ui.rs`. What only the
//! binary can show is the decision: `CLICOLOR_FORCE`, `NO_COLOR`, `--no-color`
//! and a pipe all reach `ui::init` through the environment and the argument
//! parser, not through a test calling it. `uninstall` of a package that was
//! never installed is the failure used throughout: it is offline, touches
//! nothing, and ends in an error headline on every OS.

use assert_cmd::Command;
use assert_fs::TempDir;

/// Run `ketch uninstall ghost` in a throwaway root and home, with nothing
/// colour-related inherited from the shell running the suite.
fn uninstall_ghost(extra_args: &[&str], envs: &[(&str, &str)]) -> String {
    let root = TempDir::new().expect("temp root");
    let home = TempDir::new().expect("temp home");
    let mut cmd = Command::cargo_bin("ketch").expect("ketch binary");
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|k| k.starts_with("KETCH_")) {
            cmd.env_remove(key);
        }
    }
    cmd.env("KETCH_ROOT", root.path())
        .env("KETCH_AUTO_UPDATE", "false")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .args(extra_args)
        .args(["uninstall", "ghost"]);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("run ketch");
    assert!(!out.status.success(), "uninstalling nothing must fail");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn a_forced_error_headline_is_red_from_label_to_end() {
    let stderr = uninstall_ghost(&[], &[("CLICOLOR_FORCE", "1")]);
    let headline = stderr.lines().next().unwrap_or_default();
    insta::assert_snapshot!(headline.replace('\u{1b}', "\\e"), @r"\e[31m     error `ghost` is not installed\e[0m");
}

#[test]
fn a_pipe_gets_no_escape_bytes() {
    let stderr = uninstall_ghost(&[], &[]);
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
    assert!(
        stderr.contains("error `ghost` is not installed"),
        "{stderr:?}"
    );
}

#[test]
fn no_color_keeps_escape_bytes_out() {
    let stderr = uninstall_ghost(&[], &[("NO_COLOR", "1")]);
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn the_no_color_flag_outranks_a_forced_colour() {
    let stderr = uninstall_ghost(&["--no-color"], &[("CLICOLOR_FORCE", "1")]);
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn the_no_emoji_flag_is_accepted_everywhere_and_a_pipe_gets_no_icon() {
    let stderr = uninstall_ghost(&["--no-emoji"], &[("KETCH_EMOJI", "1")]);
    assert!(
        stderr.starts_with("     error `ghost` is not installed"),
        "{stderr:?}"
    );
    assert!(!stderr.contains('❌'), "{stderr:?}");
}
