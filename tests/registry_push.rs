//! Binary-level tests for `ketch registry push`'s offline surface.
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
fn registry_help_does_not_advertise_an_unimplemented_validator() {
    Command::cargo_bin("ketch")
        .unwrap()
        .args(["registry", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("push").and(predicate::str::contains("validate").not()));
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
