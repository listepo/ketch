// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! `ketch self uninstall` run by the ketch that lives inside the root it is
//! removing — which is where `ketch self install` puts it.
//!
//! Every OS, because the part most likely to break is Windows: the running
//! `ketch.exe` cannot be deleted until it exits, so the root is finished off
//! by a process that outlives it.

mod support;

use std::time::{Duration, Instant};
use support::Sandbox;

#[test]
fn self_uninstall_leaves_no_root_once_the_running_ketch_has_exited() {
    let sandbox = Sandbox::new();
    let dir = sandbox.store().join("ketch").join("0.8.1");
    std::fs::create_dir_all(&dir).expect("create store prefix");
    let exe = dir.join(if cfg!(windows) { "ketch.exe" } else { "ketch" });
    std::fs::copy(env!("CARGO_BIN_EXE_ketch"), &exe).expect("copy ketch into the store");
    // A stale swap sibling, as a failed update leaves it.
    std::fs::create_dir_all(sandbox.store().join("ketch").join("0.8.0.old")).expect("plant .old");
    let nowhere = sandbox.home().join("no-extra-path");

    let out = sandbox.ketch_from(&exe, &["self", "uninstall", "--yes"], &nowhere);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "uninstall failed:\n{stderr}");

    // Windows hands the rest to a process waiting for this one to exit.
    let deadline = Instant::now() + Duration::from_secs(60);
    while sandbox.root().exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(!sandbox.root().exists(), "the root survived:\n{stderr}");
}
