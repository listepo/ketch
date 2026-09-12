//! Windows pipeline coverage: the real binary against a throwaway root.
//!
//! Compiles and runs only on Windows (`cargo test` on a Windows host or runner).
#![cfg(target_os = "windows")]

mod support;

use support::{host_arch, Archive, Entry, Release, Sandbox};

fn tool_archive(version: &str) -> Archive {
    Archive::Zip(vec![
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
        &format!("testtool-{version}-{arch}-pc-windows-msvc.zip"),
        tool_archive(version),
    );
    let linux = sandbox.asset(
        &format!("testtool-{version}-{arch}-unknown-linux-gnu.tar.gz"),
        tool_archive("linux-decoy"),
    );
    sandbox.publish("testtool", &[Release::new(version, vec![linux, native])]);
}

fn installed_bin(sandbox: &Sandbox) -> std::path::PathBuf {
    sandbox.bin().join("testtool.cmd")
}

fn run(path: &std::path::Path) -> String {
    let out = std::process::Command::new(path)
        .output()
        .expect("run installed program");
    assert!(out.status.success(), "{} did not run", path.display());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn a_tool_is_downloaded_verified_copied_and_runnable() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    let dest = installed_bin(&sandbox);
    assert!(
        dest.is_file(),
        "expected a copied file at {}",
        dest.display()
    );
    assert!(
        !dest.is_symlink(),
        "windows placement must copy, not symlink"
    );
    assert_eq!(run(&dest), "testtool 1.0.0");

    let listed = sandbox.ok(&["list", "--json"]);
    assert!(listed.contains(r#""name": "testtool""#), "{listed}");
    assert!(listed.contains(r#""checksum_verified": true"#), "{listed}");
    assert!(
        listed.contains(&format!(
            "testtool-1.0.0-{}-pc-windows-msvc.zip",
            host_arch()
        )),
        "{listed}"
    );
}

#[test]
fn an_upgrade_replaces_the_payload_and_the_copy_still_works() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);
    let dest = installed_bin(&sandbox);
    assert_eq!(run(&dest), "testtool 1.0.0");

    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["upgrade", "--yes"]);
    assert_eq!(run(&dest), "testtool 2.0.0");
}

#[test]
fn a_download_that_does_not_match_its_checksum_installs_nothing() {
    let sandbox = Sandbox::new();
    let arch = host_arch();
    let asset = sandbox
        .asset(
            &format!("testtool-1.0.0-{arch}-pc-windows-msvc.zip"),
            tool_archive("1.0.0"),
        )
        .with_wrong_digest();
    sandbox.publish("testtool", &[Release::new("1.0.0", vec![asset])]);
    sandbox.fails(&["install", "test:testtool", "--yes"]);
    assert!(!installed_bin(&sandbox).exists());
}

#[test]
fn a_binary_the_user_put_there_is_never_overwritten() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    std::fs::create_dir_all(sandbox.bin()).unwrap();
    std::fs::write(installed_bin(&sandbox), b"mine").unwrap();
    let err = sandbox.fails(&["install", "test:testtool", "--yes"]);
    assert!(err.contains("already exists"), "{err}");
    assert_eq!(std::fs::read(installed_bin(&sandbox)).unwrap(), b"mine");
}

#[test]
fn uninstall_removes_every_trace_of_a_tool() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["uninstall", "testtool", "--yes"]);
    assert!(!installed_bin(&sandbox).exists());
    assert!(sandbox.ok(&["list"]).contains("nothing installed"));
}

#[test]
fn doctor_reports_writable_dirs_without_macos_output() {
    let sandbox = Sandbox::new();
    let out = sandbox.ok(&["doctor"]);
    assert!(out.contains("writable"), "{out}");
    assert!(!out.to_ascii_lowercase().contains("macos only"), "{out}");
    assert!(!out.contains(".app"), "{out}");
}
