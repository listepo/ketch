//! Linux pipeline coverage: the real binary against a throwaway root.
//!
//! Compiles and runs only on Linux (`cargo test` on a Linux host or runner).
#![cfg(target_os = "linux")]

mod support;

use support::{host_arch, Archive, Entry, Release, Sandbox};

fn tool_archive(version: &str) -> Archive {
    Archive::TarGz(vec![
        Entry::program(
            &format!("testtool-{version}/bin/testtool"),
            &format!("testtool {version}"),
        ),
        Entry::file(&format!("testtool-{version}/README.md"), "# testtool\n"),
    ])
}

fn publish_tool(sandbox: &Sandbox, version: &str) {
    let arch = host_arch();
    let native = sandbox.asset(
        &format!("testtool-{version}-{arch}-unknown-linux-gnu.tar.gz"),
        tool_archive(version),
    );
    let macos = sandbox.asset(
        &format!("testtool-{version}-{arch}-apple-darwin.tar.gz"),
        tool_archive("macos-decoy"),
    );
    sandbox.publish("testtool", &[Release::new(version, vec![macos, native])]);
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
    assert!(listed.contains(r#""checksum_verified": true"#), "{listed}");
    assert!(
        listed.contains(&format!(
            "testtool-1.0.0-{}-unknown-linux-gnu.tar.gz",
            host_arch()
        )),
        "{listed}"
    );
}

#[test]
fn an_upgrade_replaces_the_payload_and_the_link_still_works() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);
    let link = sandbox.bin().join("testtool");
    assert_eq!(run(&link), "testtool 1.0.0");

    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["upgrade", "--yes"]);
    assert_eq!(run(&link), "testtool 2.0.0");
}

#[test]
fn a_download_that_does_not_match_its_checksum_installs_nothing() {
    let sandbox = Sandbox::new();
    let arch = host_arch();
    let asset = sandbox
        .asset(
            &format!("testtool-1.0.0-{arch}-unknown-linux-gnu.tar.gz"),
            tool_archive("1.0.0"),
        )
        .with_wrong_digest();
    sandbox.publish("testtool", &[Release::new("1.0.0", vec![asset])]);
    sandbox.fails(&["install", "test:testtool", "--yes"]);
    assert!(!sandbox.bin().join("testtool").exists());
}

#[test]
fn a_binary_the_user_put_there_is_never_overwritten() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    std::fs::create_dir_all(sandbox.bin()).unwrap();
    std::fs::write(sandbox.bin().join("testtool"), b"mine").unwrap();
    let err = sandbox.fails(&["install", "test:testtool", "--yes"]);
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(
        std::fs::read(sandbox.bin().join("testtool")).unwrap(),
        b"mine"
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
fn uninstall_removes_every_trace_of_a_tool() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["uninstall", "testtool", "--yes"]);
    assert!(!sandbox.bin().join("testtool").exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
}

#[test]
fn doctor_reports_a_healthy_tree() {
    let sandbox = Sandbox::new();
    let out = sandbox.ok(&["doctor"]);
    assert!(out.contains("writable"), "{out}");
    assert!(!out.to_ascii_lowercase().contains("macos only"), "{out}");
}

#[test]
fn path_install_edits_a_shell_config_and_can_undo_itself() {
    let sandbox = Sandbox::new();
    sandbox.ok(&["path", "install", "--shell", "bash"]);
    let bashrc = sandbox.home().join(".bashrc");
    let text = std::fs::read_to_string(&bashrc).unwrap();
    assert!(text.contains("# >>> ketch >>>"), "{text}");
    sandbox.ok(&["path", "uninstall", "--shell", "bash"]);
    let text = std::fs::read_to_string(&bashrc).unwrap();
    assert!(!text.contains("# >>> ketch >>>"), "{text}");
}
