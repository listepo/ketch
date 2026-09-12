//! Binary-level tests for `ketch registry`'s offline surface.
//!
//! The review step — fetch the registry's copy, diff it, confirm — needs a
//! live GitHub token, so its three-way decision is unit-tested in
//! `src/push.rs`. What only the real binary can prove is everything short of
//! that boundary: the package file is found, parsed and validated before any
//! flag is honoured, `--registry` is checked before it can become a URL, and a
//! dry run reports the destination and the file body with no token in the
//! environment. Every case strips the three token variables so a developer's
//! real credentials can never send it to the network.

use assert_cmd::Command;
use assert_fs::prelude::*;
use predicates::prelude::*;

#[test]
fn registry_help_lists_push_and_validate() {
    Command::cargo_bin("ketch")
        .unwrap()
        .args(["registry", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("push").and(predicate::str::contains("validate")));
}

fn write_package(dir: &assert_fs::fixture::ChildPath, folder: &str, body: &str) {
    let package = dir.child(folder);
    package.create_dir_all().unwrap();
    package.child("ketch.toml").write_str(body).unwrap();
}

#[test]
fn registry_validate_accepts_a_valid_package_tree() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let registry = temp.child("registry");
    write_package(&registry, "tool", "source = \"github:a/b\"\n");

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            registry.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 package"));
}

#[test]
fn registry_validate_rejects_a_bin_name_that_would_escape() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let registry = temp.child("registry");
    write_package(
        &registry,
        "evil",
        "name = \"evil\"\nsource = \"github:a/b\"\n\
         bin = [{ name = \"../../../.zshrc\" }]\n",
    );

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            registry.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .failure()
        .stdout(predicate::str::contains(".zshrc").or(predicate::str::contains("file name")));
}

#[test]
fn registry_validate_rejects_two_packages_claiming_the_same_name() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let registry = temp.child("registry");
    write_package(
        &registry,
        "fd",
        "source = \"github:sharkdp/fd\"\nprovides = [\"fd\"]\n",
    );
    write_package(
        &registry,
        "zfd",
        "source = \"github:someone/zfd\"\nprovides = [\"fd\"]\n",
    );

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            registry.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .failure()
        .stdout(predicate::str::contains("both `fd` and `zfd`"));
}

#[test]
fn registry_validate_json_reports_failure() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let registry = temp.child("registry");
    write_package(&registry, "broken", "source = 12\n");

    let assert = Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            "--json",
            registry.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .failure();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["status"], "fail");
    assert!(value["errors"]
        .as_array()
        .is_some_and(|errors| !errors.is_empty()));
}

#[test]
fn registry_validate_keeps_bidi_out_of_the_json_it_prints() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let registry = temp.child("registry");
    // The declared name reaches the error message, and the message is printed
    // to a terminal by default — JSON escapes control characters, not bidi
    // overrides, so nothing but the filter keeps the screen intact.
    write_package(
        &registry,
        "tool",
        "name = \"\u{202e}tool\"\nsource = \"github:a/b\"\n",
    );

    let assert = Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            "--json",
            registry.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .failure();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["status"], "fail");
    assert!(
        !stdout.contains('\u{202e}'),
        "a bidi override reached the terminal: {stdout:?}"
    );
}

#[test]
fn registry_validate_rejects_a_missing_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let missing = temp.child("nope");

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            missing.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .failure()
        .stdout(predicate::str::contains("no such directory"));
}

/// The path being wrong is the most likely way a CI job gets this command
/// wrong, and `--json` exists for exactly that job: it must still answer in
/// JSON rather than with an empty stdout that `jq` cannot read.
#[test]
fn registry_validate_json_still_answers_for_a_missing_directory() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let missing = temp.child("nope");

    let assert = Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "validate",
            "--json",
            missing.path().to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .assert()
        .failure();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(value["status"], "fail");
    assert_eq!(value["packages"], 0);
    assert!(
        value["errors"][0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("no such directory")),
        "{stdout}"
    );
}

#[test]
fn registry_push_dry_run_names_the_registry_folder_and_sends_nothing() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();
    // No `name`: the folder supplies it, normalised as the registry would.
    project
        .child("ketch.toml")
        .write_str("source = \"github:acme/fancy-tool\"\n# keep me\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .success()
        .stdout("source = \"github:acme/fancy-tool\"\n# keep me\n")
        .stderr(predicate::str::contains(
            "listepo/ketch-registry:fancy-tool/ketch.toml",
        ));
}

#[test]
fn registry_push_without_a_token_explains_what_to_set_before_touching_the_network() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("name = \"tool\"\nsource = \"github:acme/tool\"\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args(["--root", root.path().to_str().unwrap(), "registry", "push"])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("KETCH_GITHUB_TOKEN"));
}

#[test]
fn registry_push_refuses_a_local_source() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("name = \"tool\"\nsource = \"local:/etc/passwd\"\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("local path"));
}

#[test]
fn registry_push_refuses_a_package_file_with_an_unknown_key() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("name = \"tool\"\nsource = \"github:acme/tool\"\nbinary = \"tool\"\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("binary"));
}

#[test]
fn registry_push_names_a_missing_package_file() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("ketch.toml"));
}

#[test]
fn registry_push_accepts_an_explicit_registry() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("name = \"tool\"\nsource = \"github:acme/tool\"\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--registry",
            "acme/other-registry",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "acme/other-registry:tool/ketch.toml",
        ));
}

#[test]
fn registry_push_help_lists_the_review_flags() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--help",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .success()
        .stdout(predicate::str::contains("--dry-run").and(predicate::str::contains("--yes")));
}

#[test]
fn registry_push_rejects_a_malformed_registry_argument() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("name = \"tool\"\nsource = \"github:acme/tool\"\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args([
            "--root",
            root.path().to_str().unwrap(),
            "registry",
            "push",
            "--registry",
            "not-a-repo",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not-a-repo")
                .and(predicate::str::contains("expected `owner/repo`")),
        );
}

// The old top-level spelling was replaced by `registry push`; this pins the
// removal, so an accidental revival cannot pass unnoticed.
#[test]
fn the_retired_push_spelling_is_recognized_no_more() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("name = \"tool\"\nsource = \"github:acme/tool\"\n")
        .unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .current_dir(project.path())
        .args(["--root", root.path().to_str().unwrap(), "push", "--dry-run"])
        .env("NO_COLOR", "1")
        .env_remove("KETCH_GITHUB_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}
