//! Local filesystem installs: archive, bare binary, symlink, missing path.
//!
//! Offline, like the rest of the end-to-end suite — the `local` source never
//! touches the network.
#![cfg(unix)]

mod support;

use std::os::unix::fs::PermissionsExt;
use support::{Archive, Entry, Sandbox};

fn write_program(path: &std::path::Path, says: &str) {
    std::fs::write(path, format!("#!/bin/sh\necho '{says}'\n")).expect("write program");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// A minimal `.app` bundle whose executable prints `says`.
#[cfg(target_os = "macos")]
fn write_app(app: &std::path::Path, says: &str) {
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).expect("create bundle");
    write_program(&macos.join("Thing"), says);
}

/// The file is staged under its sanitized name, so the link must look for the
/// same spelling — a leading dot is one `sanitize_component` strips.
#[test]
fn a_local_binary_whose_name_starts_with_a_dot_still_links() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture(".dottool");
    write_program(&fixture, "dotted");

    sandbox.ok(&["install", "--path", fixture.to_str().unwrap(), "-y"]);

    let out = std::process::Command::new(sandbox.bin().join("dottool"))
        .output()
        .expect("run linked binary");
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "dotted");
}

/// A bundle is hashed as a tree rather than downloaded, and that path has to
/// be held to the lockfile exactly like a file is.
#[cfg(target_os = "macos")]
#[test]
fn sync_refuses_a_local_app_that_changed_since_the_lock() {
    let sandbox = Sandbox::new();
    let app = sandbox.fixture("Thing.app");
    write_app(&app, "v1");
    let lock = sandbox.home().join("ketch.lock");
    let lock_arg = lock.display().to_string();

    sandbox.ok(&["install", "--path", app.to_str().unwrap(), "-y"]);
    sandbox.ok(&["lock", "--file", &lock_arg]);
    sandbox.ok(&["uninstall", "thing.app", "--yes"]);
    write_app(&app, "v2");

    let said = sandbox.fails(&["sync", "--file", &lock_arg]);
    assert!(said.contains("does not match the lockfile"), "{said}");
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
}

/// A local path has no published checksum to require; the policy has to treat
/// a bundle the same as a file rather than refuse one and wave the other on.
#[cfg(target_os = "macos")]
#[test]
fn require_checksum_treats_a_local_app_like_a_local_binary() {
    let sandbox = Sandbox::new();
    let tool = sandbox.fixture("btool");
    write_program(&tool, "b");
    let app = sandbox.fixture("Thing.app");
    write_app(&app, "v1");

    sandbox.ok(&[
        "install",
        "--require-checksum",
        "--path",
        tool.to_str().unwrap(),
        "-y",
    ]);
    sandbox.ok(&[
        "install",
        "--require-checksum",
        "--path",
        app.to_str().unwrap(),
        "-y",
    ]);
}

#[test]
fn local_binary_install_appears_in_list_and_info_json() {
    let sandbox = Sandbox::new();
    // Put the fixture outside the ketch root's bin dir so we do not collide
    // with links ketch creates.
    let fixture = sandbox.fixture("localtool");
    write_program(&fixture, "local-binary-1");

    sandbox.ok(&[
        "install",
        "--path",
        fixture.to_str().unwrap(),
        "--name",
        "localtool",
        "-y",
    ]);

    let list = sandbox.ok(&["list", "--json"]);
    assert!(
        list.contains("\"name\": \"localtool\""),
        "list json missing name: {list}"
    );
    assert!(
        list.contains("local:"),
        "list json missing local source: {list}"
    );
    assert!(
        list.contains("\"local_kind\": \"binary\""),
        "list json missing local_kind: {list}"
    );

    let info = sandbox.ok(&["info", "localtool", "--json"]);
    assert!(
        info.contains("\"local_kind\": \"binary\""),
        "info json missing kind: {info}"
    );
    assert!(
        info.contains("\"local_path\""),
        "info json missing local_path: {info}"
    );

    let linked = sandbox.bin().join("localtool");
    assert!(linked.exists(), "binary was not linked into bin");
    let out = std::process::Command::new(&linked)
        .output()
        .expect("run linked binary");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "local-binary-1"
    );

    // outdated must not error on a tree that is only local packages
    let outdated = sandbox.ok(&["outdated", "--json"]);
    assert!(
        outdated.trim() == "[]" || outdated.contains('['),
        "{outdated}"
    );
}

#[test]
fn local_archive_install_via_local_scheme() {
    let sandbox = Sandbox::new();
    let archive_path = sandbox.fixture("tiny-tool.tar.gz");
    Archive::TarGz(vec![
        Entry::program("tiny-tool/bin/tinytool", "from-archive"),
        Entry::file("tiny-tool/README.md", "hi\n"),
    ])
    .write_to(&archive_path);

    let pkg = format!("local:{}", archive_path.display());
    sandbox.ok(&["install", &pkg, "--name", "tinytool", "-y"]);

    let list = sandbox.ok(&["list", "--json"]);
    assert!(list.contains("\"local_kind\": \"archive\""), "{list}");
    assert!(list.contains("tinytool"), "{list}");

    let linked = sandbox.bin().join("tinytool");
    assert!(linked.exists(), "archive binary not linked");
    let out = std::process::Command::new(&linked).output().expect("run");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "from-archive");
}

#[test]
fn local_symlink_to_binary_records_symlink_kind() {
    let sandbox = Sandbox::new();
    let target = sandbox.fixture("real-tool");
    let link = sandbox.fixture("link-tool");
    write_program(&target, "via-symlink");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    sandbox.ok(&[
        "install",
        "--path",
        link.to_str().unwrap(),
        "--name",
        "linktool",
        "-y",
    ]);

    let list = sandbox.ok(&["list", "--json"]);
    assert!(
        list.contains("\"local_kind\": \"symlink\""),
        "expected symlink kind: {list}"
    );
    let linked = sandbox.bin().join("linktool");
    let out = std::process::Command::new(&linked).output().expect("run");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "via-symlink");
}

#[test]
fn missing_local_path_errors_clearly() {
    let sandbox = Sandbox::new();
    let missing = sandbox.fixture("no-such-local-file");
    let err = sandbox.fails(&["install", "--path", missing.to_str().unwrap(), "-y"]);
    assert!(
        err.contains("does not exist") || err.contains("local path"),
        "unclear error: {err}"
    );
}

#[test]
fn local_plain_directory_is_refused() {
    let sandbox = Sandbox::new();
    let dir = sandbox.fixture("plain-dir");
    std::fs::create_dir_all(&dir).unwrap();
    let err = sandbox.fails(&["install", "--path", dir.to_str().unwrap(), "-y"]);
    assert!(
        err.contains("directory") || err.contains("refusing"),
        "unclear error: {err}"
    );
}

#[test]
fn uninstall_local_binary_clears_links_and_keeps_source() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("localtool");
    write_program(&fixture, "local-binary-1");

    sandbox.ok(&[
        "install",
        "--path",
        fixture.to_str().unwrap(),
        "--name",
        "localtool",
        "-y",
    ]);
    assert!(sandbox.bin().join("localtool").exists());

    sandbox.ok(&["uninstall", "localtool", "--yes"]);

    assert!(!sandbox.bin().join("localtool").exists());
    assert!(!sandbox.store().join("localtool").exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
    assert!(
        fixture.exists(),
        "uninstall must not delete the user's original binary"
    );
    let out = std::process::Command::new(&fixture)
        .output()
        .expect("run original");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "local-binary-1"
    );
}

#[test]
fn uninstall_local_symlink_clears_links_and_keeps_origin() {
    let sandbox = Sandbox::new();
    let target = sandbox.fixture("real-tool");
    let link = sandbox.fixture("link-tool");
    write_program(&target, "via-symlink");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    sandbox.ok(&[
        "install",
        "--path",
        link.to_str().unwrap(),
        "--name",
        "linktool",
        "-y",
    ]);
    assert!(sandbox.bin().join("linktool").exists());

    sandbox.ok(&["uninstall", "linktool", "--yes"]);

    assert!(!sandbox.bin().join("linktool").exists());
    assert!(!sandbox.store().join("linktool").exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
    assert!(target.exists(), "uninstall must keep the symlink target");
    assert!(
        link.symlink_metadata().is_ok(),
        "uninstall must keep the user's install-source symlink"
    );
    let out = std::process::Command::new(&link)
        .output()
        .expect("run origin link");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "via-symlink");
}

#[test]
fn uninstall_local_archive_clears_links() {
    let sandbox = Sandbox::new();
    let archive_path = sandbox.fixture("tiny-tool.tar.gz");
    Archive::TarGz(vec![
        Entry::program("tiny-tool/bin/tinytool", "from-archive"),
        Entry::file("tiny-tool/README.md", "hi\n"),
    ])
    .write_to(&archive_path);

    let pkg = format!("local:{}", archive_path.display());
    sandbox.ok(&["install", &pkg, "--name", "tinytool", "-y"]);
    assert!(sandbox.bin().join("tinytool").exists());

    sandbox.ok(&["uninstall", "tinytool", "--yes"]);

    assert!(!sandbox.bin().join("tinytool").exists());
    assert!(!sandbox.store().join("tinytool").exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
    assert!(
        archive_path.exists(),
        "uninstall must not delete the user's archive"
    );
}
