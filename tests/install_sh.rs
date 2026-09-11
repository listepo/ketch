//! `install.sh`'s argument handling, up to the point it would touch anything.
//!
//! The script is the one piece of the release that runs before any ketch
//! exists, so its flags cannot be covered by driving the binary. PATH is
//! emptied for every case: a flag check that fails to stop the script there
//! runs into a missing `id`/`curl` rather than the network.

use assert_cmd::Command;
use predicates::prelude::*;

fn install_sh() -> Command {
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .env("PATH", "/nonexistent")
        .env("HOME", "/nonexistent-home");
    cmd
}

/// `--install-dir` is the root's bin dir. Given beside a `--root` it
/// disagrees with, one of the two would be silently dropped.
#[test]
fn an_install_dir_outside_the_root_is_refused_before_anything_runs() {
    install_sh()
        .args(["--root", "/tmp/a", "--install-dir", "/tmp/b/bin"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--install-dir"));
}

/// `--install-dir` alone names the root as its parent, but ketch still
/// installs into `<root>/bin`. A path whose last component is not `bin`
/// would silently land in a sibling of what was asked for.
#[test]
fn an_install_dir_must_be_named_bin_even_when_it_names_the_root() {
    install_sh()
        .args(["--install-dir", "/tmp/myapp/tools"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--install-dir"));
}
