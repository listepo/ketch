//! Regression tests for read-only query commands.
#![cfg(target_os = "macos")]

mod support;

use support::{host_arch, Archive, Entry, Release, Sandbox};

const CHANGELOG: &str = "\
# Changelog

## [2.0.0] - 2024-06-01

- the second one

## [1.0.0] - 2024-05-01

- the first one
";

fn tool_archive(version: &str) -> Archive {
    Archive::TarGz(vec![
        Entry::program(
            &format!("testtool-{version}/bin/testtool"),
            &format!("testtool {version}"),
        ),
        Entry::file(&format!("testtool-{version}/CHANGELOG.md"), CHANGELOG),
    ])
}

fn publish_tool(sandbox: &Sandbox, version: &str) {
    let arch = host_arch();
    let native = sandbox.asset(
        &format!("testtool-{version}-{arch}-apple-darwin.tar.gz"),
        tool_archive(version),
    );
    sandbox.publish(
        "testtool",
        &[Release::new(version, vec![native])
            .with_notes(&format!("published notes for {version}"))],
    );
}

fn publish_named(sandbox: &Sandbox, name: &str, version: &str) {
    let asset = sandbox.asset(
        &format!("{name}-{version}-{}-apple-darwin.tar.gz", host_arch()),
        Archive::TarGz(vec![Entry::program(
            &format!("{name}-{version}/bin/{name}"),
            &format!("{name} {version}"),
        )]),
    );
    sandbox.publish(name, &[Release::new(version, vec![asset])]);
}

/// `@version` picks the changelog section; it does not mean "not installed".
#[test]
fn changelog_file_with_an_explicit_version_reads_the_installed_payload() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    let out = sandbox.ok(&["changelog", "testtool@1.0.0", "--file"]);
    assert!(out.contains("the first one"), "no 1.0.0 section:\n{out}");
    assert!(
        !out.contains("the second one"),
        "ran past 1.0.0 into 2.0.0:\n{out}"
    );
}

/// Installed state is the answer even when the source plugin is gone.
#[test]
fn info_still_reports_an_installed_package_when_the_source_is_unavailable() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    let plugin = sandbox.root().join("plugins").join("ketch-source-test");
    std::fs::remove_file(&plugin).expect("remove plugin");

    let info = sandbox.ok(&["info", "testtool"]);
    assert!(info.contains("installed"), "{info}");
    assert!(info.contains("1.0.0"), "{info}");
    assert!(
        info.contains(&sandbox.bin().join("testtool").display().to_string()),
        "link missing:\n{info}"
    );

    let json = sandbox.ok(&["info", "testtool", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed["installed"], "1.0.0");
    assert_eq!(parsed["name"], "testtool");
}

/// Scripts need to tell a partial check from "everything current".
#[test]
fn outdated_json_reports_partial_failures_and_still_exits_non_zero() {
    let sandbox = Sandbox::new();
    publish_named(&sandbox, "alpha", "1.0.0");
    publish_named(&sandbox, "bravo", "1.0.0");
    sandbox.ok(&["install", "test:alpha", "test:bravo", "--yes"]);

    publish_named(&sandbox, "alpha", "2.0.0");
    let releases = sandbox
        .root()
        .parent()
        .expect("sandbox parent")
        .join("assets")
        .join("bravo.releases.json");
    std::fs::remove_file(releases).expect("remove bravo releases");

    let out = sandbox.ketch(&["outdated", "--json"]);
    assert!(
        !out.status.success(),
        "outdated --json passed when a check failed"
    );
    let json = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON on failure");
    assert_eq!(parsed["status"], "partial", "{json}");
    assert!(
        parsed["outdated"]
            .as_array()
            .expect("outdated")
            .iter()
            .any(|row| row["name"] == "alpha"),
        "{json}"
    );
    assert!(
        parsed["failed"]
            .as_array()
            .expect("failed")
            .iter()
            .any(|row| row["name"] == "bravo"),
        "{json}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("bravo"), "{stderr}");
}
