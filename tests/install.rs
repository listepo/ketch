//! End-to-end tests: the real binary, a real install tree, real archives.
//!
//! These exist because the unit tests each prove one function and none of them
//! prove the pipeline. Every bug this suite was written against — a bundle
//! unwrapped into its own `Contents`, a link left pointing at a deleted store,
//! an upgrade that removed the old version before the new one was in place —
//! passed every unit test in the tree.
//!
//! macOS-only, like the platform layer they exercise.
#![cfg(target_os = "macos")]

mod support;

use support::{host_arch, Archive, Entry, Release, Sandbox};

/// A command-line tool, shaped like a real release tarball: a version-stamped
/// wrapper directory with the binary under `bin/`.
fn tool_archive(version: &str) -> Archive {
    Archive::TarGz(vec![
        Entry::program(
            &format!("testtool-{version}/bin/testtool"),
            &format!("testtool {version}"),
        ),
        Entry::file(&format!("testtool-{version}/README.md"), "# testtool\n"),
    ])
}

/// A macOS app, shaped like a real release zip: the bundle alone at the root.
fn app_archive(version: &str) -> Archive {
    Archive::Zip(vec![
        Entry::file(
            "TestApp.app/Contents/Info.plist",
            "<?xml version=\"1.0\"?><plist version=\"1.0\"><dict></dict></plist>",
        ),
        Entry::program(
            "TestApp.app/Contents/MacOS/TestApp",
            &format!("app {version}"),
        ),
    ])
}

/// Publish `testtool` at one version, with a decoy for every other platform so
/// asset selection is doing real work rather than picking the only candidate.
fn publish_tool(sandbox: &Sandbox, version: &str) {
    let arch = host_arch();
    let native = sandbox.asset(
        &format!("testtool-{version}-{arch}-apple-darwin.tar.gz"),
        tool_archive(version),
    );
    let linux = sandbox.asset(
        &format!("testtool-{version}-{arch}-unknown-linux-gnu.tar.gz"),
        tool_archive("linux-decoy"),
    );
    sandbox.publish("testtool", &[Release::new(version, vec![linux, native])]);
}

fn run(path: &std::path::Path) -> String {
    let out = std::process::Command::new(path)
        .output()
        .expect("run installed program");
    assert!(out.status.success(), "{} did not run", path.display());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn a_tool_is_downloaded_verified_linked_and_runnable() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");

    sandbox.ok(&["install", "test:testtool", "--yes"]);

    let link = sandbox.bin().join("testtool");
    assert!(
        link.is_symlink(),
        "expected a symlink at {}",
        link.display()
    );
    assert_eq!(run(&link), "testtool 1.0.0");

    let listed = sandbox.ok(&["list", "--json"]);
    assert!(listed.contains(r#""name": "testtool""#), "{listed}");
    // The plugin published a digest, so this was verified rather than trusted
    // on first use.
    assert!(listed.contains(r#""checksum_verified": true"#), "{listed}");
    // The asset naming this machine's architecture beat the Linux decoy.
    assert!(
        listed.contains(&format!(
            "testtool-1.0.0-{}-apple-darwin.tar.gz",
            host_arch()
        )),
        "{listed}"
    );
}

#[test]
fn an_app_bundle_is_placed_whole_and_removed_again() {
    let sandbox = Sandbox::new();
    let asset = sandbox.asset("TestApp-1.0.0-macos.zip", app_archive("1.0.0"));
    sandbox.publish("testapp", &[Release::new("1.0.0", vec![asset])]);

    sandbox.ok(&["install", "test:testapp", "--yes"]);

    // The bundle is the payload. Unwrapping it as though it were a wrapper
    // directory would place `Contents` and leave no app at all.
    let app = sandbox.apps().join("TestApp.app");
    assert!(app.is_dir(), "expected {} to exist", app.display());
    assert_eq!(run(&app.join("Contents/MacOS/TestApp")), "app 1.0.0");
    assert!(app.join("Contents/Info.plist").is_file());

    // An app is not a command-line tool: its executables stay out of PATH.
    assert!(!sandbox.bin().join("TestApp").exists());

    sandbox.ok(&["uninstall", "testapp", "--yes"]);
    assert!(!app.exists(), "{} outlived its package", app.display());
}

#[test]
fn an_upgrade_replaces_the_payload_and_the_link_still_works() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);

    let link = sandbox.bin().join("testtool");
    assert_eq!(run(&link), "testtool 1.0.0");

    publish_tool(&sandbox, "2.0.0");
    assert!(sandbox.ok(&["outdated"]).contains("2.0.0"));
    sandbox.ok(&["upgrade", "--yes"]);

    assert_eq!(run(&link), "testtool 2.0.0");
    assert!(sandbox
        .ok(&["list", "--json"])
        .contains(r#""version": "2.0.0""#));
}

#[test]
fn a_pinned_package_is_left_where_it_is() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["pin", "testtool"]);

    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["upgrade", "--yes"]);

    assert_eq!(run(&sandbox.bin().join("testtool")), "testtool 1.0.0");
}

#[test]
fn a_download_that_does_not_match_its_checksum_installs_nothing() {
    let sandbox = Sandbox::new();
    let arch = host_arch();
    let tampered = sandbox
        .asset(
            &format!("testtool-1.0.0-{arch}-apple-darwin.tar.gz"),
            tool_archive("1.0.0"),
        )
        .with_wrong_digest();
    sandbox.publish("testtool", &[Release::new("1.0.0", vec![tampered])]);

    let stderr = sandbox.fails(&["install", "test:testtool", "--yes"]);
    assert!(stderr.to_lowercase().contains("checksum"), "{stderr}");

    // A refused install leaves nothing behind: no link, no store directory,
    // and nothing recorded.
    assert!(!sandbox.bin().join("testtool").exists());
    assert!(!sandbox.store().join("testtool").exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
}

#[test]
fn uninstall_removes_every_trace_of_a_tool() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    sandbox.ok(&["uninstall", "testtool", "--yes"]);

    assert!(!sandbox.bin().join("testtool").exists());
    assert!(!sandbox.store().join("testtool").exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
}

#[test]
fn a_binary_the_user_put_there_is_never_overwritten() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");

    // Something already occupies the name ketch wants.
    std::fs::create_dir_all(sandbox.bin()).unwrap();
    let squatter = sandbox.bin().join("testtool");
    std::fs::write(&squatter, "#!/bin/sh\necho 'not ketch'\n").unwrap();

    let stderr = sandbox.fails(&["install", "test:testtool", "--yes"]);
    assert!(stderr.contains("not installed by ketch"), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(&squatter).unwrap(),
        "#!/bin/sh\necho 'not ketch'\n"
    );
}

#[test]
fn relink_rebuilds_a_link_that_was_deleted_by_hand() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    let link = sandbox.bin().join("testtool");
    std::fs::remove_file(&link).unwrap();

    sandbox.ok(&["link", "testtool"]);
    assert_eq!(run(&link), "testtool 1.0.0");
}

#[test]
fn doctor_reports_a_healthy_tree() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    // Exit status is the assertion: doctor fails when the tree is broken.
    sandbox.ok(&["doctor"]);
}
