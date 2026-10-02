// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

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

    let listed = sandbox.state();
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
    assert!(
        sandbox.store().join("testtool").join("1.0.0").is_dir(),
        "upgrade must keep the previous prefix"
    );
}

#[test]
fn rollback_restores_the_previous_prefix_without_redownloading() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);
    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["upgrade", "--yes"]);
    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["rollback", "testtool"]);
    assert_eq!(run(&installed_bin(&sandbox)), "testtool 1.0.0");
}

#[test]
fn rollback_without_a_retained_version_leaves_the_install_alone() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);
    let stderr = sandbox.fails(&["rollback", "testtool"]);
    assert!(stderr.contains("no retained version"), "{stderr}");
}

#[test]
fn rollback_refuses_a_pinned_package() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);
    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["upgrade", "--yes"]);
    sandbox.ok(&["pin", "testtool"]);
    let stderr = sandbox.fails(&["rollback", "testtool"]);
    assert!(stderr.to_lowercase().contains("pinned"), "{stderr}");
}

#[test]
fn uninstall_after_rollback_removes_every_retained_prefix() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);
    publish_tool(&sandbox, "2.0.0");
    sandbox.ok(&["upgrade", "--yes"]);
    sandbox.ok(&["rollback", "testtool"]);
    sandbox.ok(&["uninstall", "testtool", "--yes"]);
    assert!(!sandbox.store().join("testtool").exists());
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
    assert!(sandbox.ok(&["list", "local"]).contains("nothing installed"));
}

#[test]
fn doctor_reports_writable_dirs_without_macos_output() {
    let sandbox = Sandbox::new();
    let out = sandbox.ok(&["doctor"]);
    assert!(out.contains("writable"), "{out}");
    assert!(!out.to_ascii_lowercase().contains("macos only"), "{out}");
    assert!(!out.contains(".app"), "{out}");
}

/// `ketch path install` writes HKCU\\Environment\\Path, not a shell file.
/// The sandbox bin dir is unique, so a leftover is a dead temp path; Drop
/// still takes it back out if the assertions fail.
#[test]
fn path_install_puts_the_bin_dir_on_the_user_path() {
    let sandbox = Sandbox::new();
    struct Restore<'a>(&'a Sandbox);
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            let _ = self.0.ketch(&["path", "uninstall"]);
        }
    }
    let _restore = Restore(&sandbox);

    sandbox.ok(&["path", "install"]);
    let status = sandbox.ok(&["path", "status"]);
    assert!(
        status
            .lines()
            .any(|l| l.contains("user PATH") && l.contains("configured")),
        "user PATH should be configured:\n{status}"
    );

    sandbox.ok(&["path", "uninstall"]);
    let status = sandbox.ok(&["path", "status"]);
    assert!(
        status
            .lines()
            .any(|l| l.contains("user PATH") && l.contains("not set up")),
        "user PATH should be empty of this bin dir:\n{status}"
    );
}

fn user_path() -> String {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','User')",
        ])
        .output()
        .expect("read the user PATH");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn set_user_path(value: &str) {
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::SetEnvironmentVariable('Path', $env:TEST_USER_PATH, 'User')",
        ])
        .env("TEST_USER_PATH", value)
        .status()
        .expect("write the user PATH");
    assert!(status.success(), "could not write the user PATH");
}

/// Entries of the user PATH that name anything under `root`, however spelled.
fn entries_under(root: &std::path::Path) -> Vec<String> {
    let root = root.display().to_string().to_ascii_lowercase();
    user_path()
        .split(';')
        .filter(|e| {
            e.trim_matches('"')
                .replace('/', "\\")
                .to_ascii_lowercase()
                .starts_with(&root)
        })
        .map(str::to_string)
        .collect()
}

/// B66: `self uninstall` takes the bin dir back out of HKCU\Environment\Path
/// even when the entry is not spelled the way ketch spells the bin dir —
/// `install.ps1` writes whatever `Resolve-Path` gave it.
#[test]
fn self_uninstall_removes_the_user_path_entry_however_it_is_spelled() {
    let sandbox = Sandbox::new();
    let root = sandbox.root();
    struct Restore(std::path::PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let left = entries_under(&self.0);
            if !left.is_empty() {
                let kept: Vec<String> = user_path()
                    .split(';')
                    .filter(|e| !left.iter().any(|l| l == e))
                    .map(str::to_string)
                    .collect();
                set_user_path(&kept.join(";"));
            }
        }
    }
    let _restore = Restore(root.clone());

    let spelled = format!(
        "\"{}\\\"",
        sandbox.bin().display().to_string().to_ascii_uppercase()
    );
    let before = user_path();
    set_user_path(&if before.is_empty() {
        spelled.clone()
    } else {
        format!("{spelled};{before}")
    });
    assert_eq!(entries_under(&root), vec![spelled]);

    sandbox.ok(&["self", "uninstall", "--yes"]);
    assert!(
        entries_under(&root).is_empty(),
        "the user PATH still names the bin dir: {}",
        user_path()
    );
}
