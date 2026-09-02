//! Binary-level smoke tests for stable, script-visible CLI behaviour.
//!
//! The install suite owns macOS pipeline coverage. These cases deliberately
//! stay platform-neutral so argument parsing and output contracts are checked
//! anywhere Cargo can compile the binary.

use assert_cmd::Command;
use assert_fs::prelude::*;
use predicates::prelude::*;

#[test]
fn help_describes_the_product_and_install_command() {
    Command::cargo_bin("ketch")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("installs command-line tools")
                .and(predicate::str::contains("install")),
        )
        .stderr(predicate::str::is_empty());
}

#[test]
fn an_unknown_command_exits_nonzero_and_explains_the_problem() {
    Command::cargo_bin("ketch")
        .unwrap()
        .arg("not-a-command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn an_empty_root_is_reported_without_touching_the_callers_home() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");

    Command::cargo_bin("ketch")
        .unwrap()
        .args(["--root", root.path().to_str().unwrap(), "list"])
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stdout("nothing installed\n")
        .stderr(predicate::str::is_empty());

    root.assert(predicate::path::is_dir());
}

#[cfg(feature = "tui")]
#[test]
fn tui_request_falls_back_without_terminal_escape_sequences_in_ci() {
    let temp = assert_fs::TempDir::new().unwrap();
    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--tui",
            "--root",
            temp.child("root").path().to_str().unwrap(),
            "list",
        ])
        .env("CI", "1")
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stdout("nothing installed\n")
        .stderr(predicate::str::contains("\x1b").not());
}
