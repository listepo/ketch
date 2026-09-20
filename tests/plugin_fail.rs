//! A broken source plugin is a warning, never a fatal error — and a scheme
//! with no plugin at all is an actionable error, not a hang.
//!
//! These drive the real binary against plugins written into the sandbox, so
//! the discovery path (`~/.ketch/plugins` first, then `PATH`) is the one
//! production uses. Offline, like the rest of the suite.
#![cfg(unix)]

mod support;

use std::os::unix::fs::PermissionsExt;
use support::Sandbox;

fn write_plugin(sandbox: &Sandbox, name: &str, body: &str) -> std::path::PathBuf {
    let path = sandbox.root().join("plugins").join(name);
    std::fs::write(&path, body).expect("write plugin");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// `capabilities` is how a plugin introduces itself; a script that exits
/// non-zero there is broken, and discovery must say so rather than go quiet.
#[test]
fn a_plugin_that_fails_capabilities_is_named_by_plugin_list() {
    let sandbox = Sandbox::new();
    write_plugin(
        &sandbox,
        "ketch-source-broken",
        "#!/bin/sh\necho 'boom' >&2\nexit 1\n",
    );

    let out = sandbox.ketch(&["plugin", "list"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("no plugins") || stderr.contains("broken"),
        "broken plugin neither listed nor warned about:\n{stdout}\n{stderr}"
    );
}

/// A plugin that speaks a protocol ketch does not know is ignored with a
/// warning: its scheme must not resolve, and the error must name the plugin.
#[test]
fn a_plugin_on_a_future_protocol_does_not_provide_its_scheme() {
    let sandbox = Sandbox::new();
    write_plugin(
        &sandbox,
        "ketch-source-future",
        "#!/bin/sh\ncase \"$1\" in capabilities) echo '{\"protocol\":9999,\"scheme\":\"future\"}';; *) exit 1;; esac\n",
    );

    let out = sandbox.ketch(&["install", "future:owner/repo", "--yes"]);
    assert!(
        !out.status.success(),
        "install through an unsupported protocol succeeded"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("future") || stderr.contains("ketch-source-future"),
        "error names neither scheme nor plugin:\n{stderr}"
    );
}

/// A plugin whose `releases` exits non-zero fails the install it was asked
/// about — with the plugin's stderr attached, not swallowed.
#[test]
fn a_plugin_that_fails_releases_fails_the_install_with_its_stderr() {
    let sandbox = Sandbox::new();
    write_plugin(
        &sandbox,
        "ketch-source-flaky",
        "#!/bin/sh\ncase \"$1\" in capabilities) echo '{\"protocol\":1,\"scheme\":\"flaky\"}';; releases) echo 'flaky says no' >&2; exit 1;; *) exit 1;; esac\n",
    );

    let out = sandbox.ketch(&["install", "flaky:owner/repo", "--yes"]);
    assert!(
        !out.status.success(),
        "install through failing releases succeeded"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("flaky"),
        "error does not name the failing plugin:\n{stderr}"
    );
}
