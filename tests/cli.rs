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

#[test]
fn empty_history_and_stats_json_do_not_create_a_database() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let root_arg = root.path().to_str().unwrap();

    let history = Command::cargo_bin("ketch")
        .unwrap()
        .args(["--root", root_arg, "history", "--json"])
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let history_json: serde_json::Value =
        serde_json::from_slice(&history.get_output().stdout).expect("history JSON");
    assert_eq!(history_json, serde_json::json!([]));

    let stats = Command::cargo_bin("ketch")
        .unwrap()
        .args(["--root", root_arg, "stats", "--json"])
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
    let stats_json: serde_json::Value =
        serde_json::from_slice(&stats.get_output().stdout).expect("stats JSON");
    assert_eq!(
        stats_json,
        serde_json::json!({
            "events": 0,
            "installs": 0,
            "upgrades": 0,
            "uninstalls": 0,
            "packages": 0,
            "mean_duration_ms": null,
            "first_at": null,
            "last_at": null,
        })
    );

    root.child("stats.db").assert(predicate::path::missing());
}

#[test]
fn package_history_names_the_package_when_no_events_match() {
    let temp = assert_fs::TempDir::new().unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            temp.child("root").path().to_str().unwrap(),
            "history",
            "ripgrep",
        ])
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stdout("no history recorded for ripgrep\n")
        .stderr(predicate::str::is_empty());
}

#[test]
fn history_rejects_a_negative_limit() {
    Command::cargo_bin("ketch")
        .unwrap()
        .args(["history", "--limit", "-1"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '-1'"));
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

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
mod unsupported_host {
    use assert_cmd::Command;
    use assert_fs::prelude::*;
    use predicates::prelude::*;

    #[test]
    fn doctor_fails_when_the_host_is_unsupported() {
        let temp = assert_fs::TempDir::new().unwrap();
        let root = temp.child("ketch-root");

        let assert = Command::cargo_bin("ketch")
            .unwrap()
            .args(["--root", root.path().to_str().unwrap(), "doctor"])
            .env("NO_COLOR", "1")
            .assert()
            .failure();

        let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("macOS only"),
            "doctor did not report an unsupported host:
stdout:
{stdout}
stderr:
{stderr}"
        );
        assert!(
            !combined.to_ascii_lowercase().contains("internal error"),
            "doctor reported an internal error:
{combined}"
        );
    }

    #[test]
    fn list_still_works_on_an_unsupported_host() {
        let temp = assert_fs::TempDir::new().unwrap();
        let root = temp.child("ketch-root");

        Command::cargo_bin("ketch")
            .unwrap()
            .args(["--root", root.path().to_str().unwrap(), "list"])
            .env("NO_COLOR", "1")
            .assert()
            .success()
            .stdout(
                "nothing installed
",
            )
            .stderr(predicate::str::is_empty());
    }

    #[test]
    fn install_fails_with_macos_only_on_an_unsupported_host() {
        let temp = assert_fs::TempDir::new().unwrap();
        let root = temp.child("ketch-root");

        let assert = Command::cargo_bin("ketch")
            .unwrap()
            .args([
                "--root",
                root.path().to_str().unwrap(),
                "install",
                "ripgrep",
            ])
            .env("NO_COLOR", "1")
            .assert()
            .failure();

        let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
        let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("macOS only"),
            "install did not report macOS-only limitation:
stdout:
{stdout}
stderr:
{stderr}"
        );
        assert!(
            !combined.to_ascii_lowercase().contains("internal error"),
            "install reported an internal error:
{combined}"
        );
    }
}
