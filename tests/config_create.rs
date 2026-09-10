//! End-to-end tests for `ketch config create`, the questionnaire that writes
//! a project's `ketch.toml`.
//!
//! Every case drives the real binary through the questionnaire's stdin
//! contract: with a pipe attached, each `ui::prompt` question reads one line
//! and an empty line takes that question's default. Questionnaire booleans such
//! as prereleases and the `bin` loop also consume one line, while the final
//! consent confirmation reads nothing and answers itself with its default. The
//! transcript therefore decides exactly which fields the written file has,
//! which is the same view of the questionnaire a script or CI job gets.

use assert_cmd::Command;
use assert_fs::prelude::*;
use predicates::prelude::*;
use std::path::Path;

/// The file the all-defaults questionnaire writes in a project directory
/// named `Fancy-Tool` for the source `github:acme/fancy-tool`: the header it
/// always adds, then the only two fields every manifest carries — every
/// defaulted field is omitted.
const MINIMAL_FILE: &str = concat!(
    "# Written by `ketch config create`. Schema: docs/MANIFESTS.md.\n",
    "name = \"fancy-tool\"\n",
    "source = \"github:acme/fancy-tool\"\n",
);

/// The empty answers that leave every question after `source` at its
/// default: name, description, homepage, kind, prereleases, strip prefix,
/// aliases, notes, the `bin` loop, extra paths, asset include, asset exclude,
/// and the asset-target loop terminator each consume one empty line.
fn empty_answers_for_the_rest() -> String {
    "\n".repeat(13)
}

/// The command under test: the questionnaire run inside `project`, with the
/// ketch root pointed at a throwaway. The questionnaire never touches the
/// root, but the caller's real one must not be involved either. Each test
/// adds its own flags and stdin transcript.
fn config_create(project: &Path, root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("ketch").unwrap();
    cmd.current_dir(project)
        .args(["--root", root.to_str().unwrap(), "config", "create"])
        .env("NO_COLOR", "1");
    cmd
}

#[test]
fn defaults_write_only_name_and_source() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    config_create(project.path(), root.path())
        .arg("--yes")
        .write_stdin(format!(
            "github:acme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"));

    project
        .child("ketch.toml")
        .assert(predicate::str::diff(MINIMAL_FILE));
}

/// An empty answer takes the default, so a default the name check rejects
/// would be re-offered forever. A folder name with a leading dash is one.
#[test]
fn a_folder_name_that_is_not_a_package_name_still_gives_a_usable_default() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("-Fancy-Tool-");
    project.create_dir_all().unwrap();

    config_create(project.path(), root.path())
        .arg("--yes")
        .timeout(std::time::Duration::from_secs(30))
        .write_stdin(format!(
            "github:acme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success();

    project
        .child("ketch.toml")
        .assert(predicate::str::diff(MINIMAL_FILE));
}

#[test]
fn answers_fill_in_the_optional_fields() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    // After `source`: an empty line takes the name default, then description
    // and homepage, then the empty answers for everything after them.
    config_create(project.path(), root.path())
        .arg("--yes")
        .write_stdin(format!(
            "github:acme/fancy-tool\n\nSearches files for a pattern\nhttps://acme.example/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success();

    let expected = concat!(
        "# Written by `ketch config create`. Schema: docs/MANIFESTS.md.\n",
        "name = \"fancy-tool\"\n",
        "source = \"github:acme/fancy-tool\"\n",
        "description = \"Searches files for a pattern\"\n",
        "homepage = \"https://acme.example/fancy-tool\"\n",
    );
    project
        .child("ketch.toml")
        .assert(predicate::str::diff(expected));
}

#[test]
fn piped_boolean_answers_are_consumed_by_their_questions() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();
    let answers = [
        "github:acme/fancy-tool",
        "",
        "",
        "",
        "",
        "yes",
        "",
        "",
        "",
        "yes",
        "dist/fancy-tool",
        "",
        "no",
        "",
        "",
        "",
        "",
    ]
    .join("\n");

    config_create(project.path(), root.path())
        .arg("--yes")
        .write_stdin(format!("{answers}\n"))
        .assert()
        .success();

    let expected = concat!(
        "# Written by `ketch config create`. Schema: docs/MANIFESTS.md.\n",
        "name = \"fancy-tool\"\n",
        "source = \"github:acme/fancy-tool\"\n",
        "prerelease = true\n",
        "\n",
        "bin = [{ path = \"dist/fancy-tool\", name = \"fancy-tool\" }]\n",
    );
    project
        .child("ketch.toml")
        .assert(predicate::str::diff(expected));
}

#[test]
fn an_existing_file_is_refused_before_any_question() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();
    let original = "source = \"github:acme/old\"\n# do not keep me\n";
    project.child("ketch.toml").write_str(original).unwrap();

    // Stdin is empty: had the questionnaire started asking anyway, the first
    // question would have failed with `needs an answer` instead of the
    // refusal, so that complaint not appearing is what proves the file was
    // checked before anything was asked.
    config_create(project.path(), root.path())
        .write_stdin("")
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("already exists")
                .and(predicate::str::contains("--force"))
                .and(predicate::str::contains("needs an answer").not()),
        );

    project
        .child("ketch.toml")
        .assert(predicate::str::diff(original));
}

#[test]
fn force_replaces_an_existing_file() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();
    project
        .child("ketch.toml")
        .write_str("source = \"github:acme/old\"\n# do not keep me\n")
        .unwrap();

    config_create(project.path(), root.path())
        .args(["--force", "--yes"])
        .write_stdin(format!(
            "github:acme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success()
        .stderr(predicate::str::contains("wrote"));

    // The exact defaults file, original content gone: the whole file was
    // replaced rather than merged.
    project
        .child("ketch.toml")
        .assert(predicate::str::diff(MINIMAL_FILE));
}

#[test]
fn no_input_at_all_leaves_the_required_source_unanswered() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    config_create(project.path(), root.path())
        .arg("--yes")
        .write_stdin("")
        .assert()
        .failure()
        .stderr(predicate::str::contains("needs an answer"));

    project
        .child("ketch.toml")
        .assert(predicate::path::missing());
}

#[test]
fn without_yes_the_write_confirmation_declines_itself() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    // A full valid transcript, but the write confirmation reads nothing from
    // a pipe and takes its `no`, so the run succeeds without writing.
    config_create(project.path(), root.path())
        .write_stdin(format!(
            "github:acme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success()
        .stderr(predicate::str::contains("cancelled"));

    project
        .child("ketch.toml")
        .assert(predicate::path::missing());
}

#[test]
fn a_source_that_does_not_parse_is_reasked() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    config_create(project.path(), root.path())
        .arg("--yes")
        .write_stdin(format!(
            "not a repo!\nacme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success()
        // The first answer drew the parse complaint rather than failing the
        // run, and the questionnaire carried on to the good one.
        .stderr(
            predicate::str::contains("is not a package reference")
                .and(predicate::str::contains("wrote")),
        );

    // A bare `owner/repo` is stored the way every other command spells it,
    // with the GitHub scheme made explicit.
    project
        .child("ketch.toml")
        .assert(predicate::str::diff(MINIMAL_FILE));
}

#[test]
fn file_writes_where_it_is_told() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    config_create(project.path(), root.path())
        .args(["--file", "custom.toml", "--yes"])
        .write_stdin(format!(
            "github:acme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success()
        .stderr(predicate::str::contains("custom.toml"));

    project
        .child("custom.toml")
        .assert(predicate::str::diff(MINIMAL_FILE));
    project
        .child("ketch.toml")
        .assert(predicate::path::missing());
}

#[test]
fn config_create_does_not_create_the_ketch_root() {
    let temp = assert_fs::TempDir::new().unwrap();
    let root = temp.child("ketch-root");
    let project = temp.child("Fancy-Tool");
    project.create_dir_all().unwrap();

    config_create(project.path(), root.path())
        .arg("--yes")
        .write_stdin(format!(
            "github:acme/fancy-tool\n{}",
            empty_answers_for_the_rest()
        ))
        .assert()
        .success();

    assert!(
        !root.path().exists(),
        "config create must not mkdir the ketch root"
    );
    project
        .child("ketch.toml")
        .assert(predicate::path::exists());
}
