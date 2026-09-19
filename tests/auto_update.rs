//! CLI behaviour for `auto_update` and for stopping a process that holds a
//! file `ketch upgrade` is about to replace.

mod support;

use std::process::{Command, Stdio};
use std::time::Duration;

use support::{host_arch, Archive, Entry, Release, Sandbox};

fn host_triple() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "apple-darwin"
    }
    #[cfg(target_os = "linux")]
    {
        "unknown-linux-gnu"
    }
    #[cfg(target_os = "windows")]
    {
        "pc-windows-msvc"
    }
}

fn asset_name(package: &str, version: &str) -> String {
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    format!(
        "{package}-{version}-{arch}-{triple}.{ext}",
        arch = host_arch(),
        triple = host_triple()
    )
}

fn archive(entries: Vec<Entry>) -> Archive {
    if cfg!(windows) {
        Archive::Zip(entries)
    } else {
        Archive::TarGz(entries)
    }
}

fn publish_tool(sandbox: &Sandbox, version: &str) {
    let native = sandbox.asset(
        &asset_name("testtool", version),
        archive(vec![Entry::program(
            &format!("testtool-{version}/bin/testtool"),
            &format!("testtool {version}"),
        )]),
    );
    sandbox.publish("testtool", &[Release::new(version, vec![native])]);
}

fn publish_sleeper(sandbox: &Sandbox, version: &str) {
    let native = sandbox.asset(
        &asset_name("testtool", version),
        archive(vec![Entry::sleeper(&format!(
            "testtool-{version}/bin/testtool"
        ))]),
    );
    sandbox.publish("testtool", &[Release::new(version, vec![native])]);
}

fn bin_tool(sandbox: &Sandbox) -> std::path::PathBuf {
    let name = if cfg!(windows) {
        "testtool.cmd"
    } else {
        "testtool"
    };
    sandbox.bin().join(name)
}

#[test]
fn install_does_not_mention_auto_update_when_it_is_off() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    let out = sandbox.ketch(&["install", "test:testtool@1.0.0", "--yes"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("auto-update"),
        "disabled auto-update must stay silent, got {err}"
    );
}

#[test]
fn install_prints_that_auto_update_is_enabled() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    let out = sandbox.ketch_overrides(
        &["install", "test:testtool@1.0.0", "--yes"],
        &[
            ("KETCH_AUTO_UPDATE", "true"),
            ("KETCH_GITHUB_API", "http://127.0.0.1:1"),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("auto-update"),
        "enabled auto-update must name itself, got {err}"
    );
    assert!(
        err.contains("enabled"),
        "enabled auto-update must say so, got {err}"
    );
}

#[test]
fn upgrade_prints_that_auto_update_is_enabled() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);
    publish_tool(&sandbox, "2.0.0");
    let out = sandbox.ketch_overrides(
        &["upgrade", "--yes"],
        &[
            ("KETCH_AUTO_UPDATE", "true"),
            ("KETCH_GITHUB_API", "http://127.0.0.1:1"),
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("auto-update"),
        "upgrade with auto-update on must name it, got {err}"
    );
}

#[test]
fn upgrade_stops_a_process_holding_the_binary_when_yes() {
    let sandbox = Sandbox::new();
    publish_sleeper(&sandbox, "1.0.0");
    sandbox.ok(&["install", "test:testtool@1.0.0", "--yes"]);

    let link = bin_tool(&sandbox);
    let mut child = Command::new(&link)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn installed tool");
    std::thread::sleep(Duration::from_millis(400));

    publish_sleeper(&sandbox, "2.0.0");
    let out = sandbox.ketch(&["upgrade", "--yes"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("in use") || err.contains("stopping"),
        "upgrade --yes must stop the occupant, got {err}"
    );

    let _ = child.kill();
    let _ = child.wait();
}
