//! A ketch that `mise use -g github:listepo/ketch` installed: the real binary,
//! copied into a sandboxed mise tree and run from there.
//!
//! Every OS, because the part most likely to break is Windows: mise has to
//! delete the directory holding the very image that asked it to.

mod support;

use support::Sandbox;

#[test]
fn a_mise_owned_ketch_will_not_rewrite_itself_in_place() {
    let sandbox = Sandbox::new();
    let exe = sandbox.install_with_mise();
    let (mise_bin, mise_log) = sandbox.fake_mise();

    let out = sandbox.ketch_from(&exe, &["self", "upgrade", "--yes"], &mise_bin);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(!out.status.success(), "the upgrade went ahead:\n{stderr}");
    assert!(stderr.contains("managed by mise"), "{stderr}");
    assert!(stderr.contains("mise upgrade"), "{stderr}");
    assert!(exe.exists(), "the mise-owned binary was touched");
    assert!(!mise_log.exists(), "mise was run for an upgrade");
}

#[test]
fn self_uninstall_hands_a_mise_install_back_to_mise() {
    let sandbox = Sandbox::new();
    let exe = sandbox.install_with_mise();
    let (mise_bin, mise_log) = sandbox.fake_mise();

    let out = sandbox.ketch_from(&exe, &["self", "uninstall", "--yes"], &mise_bin);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "uninstall failed:\n{stderr}");
    // `--yes` in front: the user has already answered, and mise would
    // otherwise ask again for every version it prunes.
    assert_eq!(
        std::fs::read_to_string(&mise_log).expect("mise was never run"),
        "--yes unuse -g github:listepo/ketch"
    );
    assert!(
        !stderr.contains("mise:"),
        "mise could not remove the install:\n{stderr}"
    );
    assert!(
        !sandbox.mise_tool_dir().exists(),
        "the mise install survived:\n{stderr}"
    );
    assert!(!sandbox.root().exists(), "the root survived:\n{stderr}");
}

#[test]
fn a_dry_run_names_the_mise_command_and_runs_nothing() {
    let sandbox = Sandbox::new();
    let exe = sandbox.install_with_mise();
    let (mise_bin, mise_log) = sandbox.fake_mise();

    let out = sandbox.ketch_from(&exe, &["self", "uninstall", "--dry-run"], &mise_bin);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("mise unuse -g github:listepo/ketch"),
        "{stderr}"
    );
    assert!(!mise_log.exists(), "mise was run on a dry run");
    assert!(exe.exists());
    assert!(sandbox.root().exists());
}
