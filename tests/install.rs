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

/// A shell startup file is the one thing ketch writes outside its own root, so
/// the guarantee worth proving end to end is that the user's own file survives
/// being written, rewritten and taken back out.
#[test]
fn path_install_edits_a_shell_config_and_can_undo_itself() {
    let sandbox = Sandbox::new();
    let zshrc = sandbox.home().join(".zshrc");
    let original = "# mine\nexport EDITOR=vi\n";
    std::fs::write(&zshrc, original).expect("write zshrc");

    sandbox.ok(&["path", "install", "--shell", "zsh"]);
    let after = std::fs::read_to_string(&zshrc).expect("read zshrc");
    assert!(
        after.starts_with(original),
        "the user's own lines moved:\n{after}"
    );
    assert!(
        after.contains(&sandbox.bin().display().to_string()),
        "bin dir missing from:\n{after}"
    );

    // Twice must not mean two blocks.
    sandbox.ok(&["path", "install", "--shell", "zsh"]);
    let twice = std::fs::read_to_string(&zshrc).expect("read zshrc");
    assert_eq!(twice, after, "a second install changed the file");

    sandbox.ok(&["path", "uninstall", "--shell", "zsh"]);
    let restored = std::fs::read_to_string(&zshrc).expect("read zshrc");
    assert_eq!(restored, original, "uninstall did not restore the file");
}

#[test]
fn a_dry_run_says_what_it_would_do_and_writes_nothing() {
    let sandbox = Sandbox::new();
    // Progress goes to stderr, so output stays pipeable — see `ui`.
    let out = sandbox.ketch(&["path", "install", "--shell", "fish", "--dry-run"]);
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("would add"), "{said}");
    assert!(
        !sandbox.home().join(".config/fish/config.fish").exists(),
        "a dry run created the file"
    );
}

/// The whole point of `--fix`: a PATH that no shell knows about is the one
/// doctor failure the user should not have to act on themselves.
#[test]
fn doctor_fixes_a_path_no_shell_knows_about() {
    let sandbox = Sandbox::new();

    let failed = sandbox.ketch_off_path(&["doctor"]);
    assert!(
        !failed.status.success(),
        "doctor passed without PATH set up"
    );
    let text = String::from_utf8_lossy(&failed.stdout);
    assert!(text.contains("is not on PATH"), "{text}");

    let fixed = sandbox.ketch_off_path(&["doctor", "--fix"]);
    let text = String::from_utf8_lossy(&fixed.stdout);
    assert!(
        fixed.status.success(),
        "doctor --fix still failed:\n{text}\n{}",
        String::from_utf8_lossy(&fixed.stderr)
    );
    // Fixed, but not in *this* process: the check has to say so rather than
    // reporting the same failure it just repaired.
    assert!(text.contains("but not in this shell"), "{text}");

    let zshrc = std::fs::read_to_string(sandbox.home().join(".zshrc")).expect("read zshrc");
    assert!(
        zshrc.contains(&sandbox.bin().display().to_string()),
        "{zshrc}"
    );
}

#[test]
fn a_path_the_user_wired_up_by_hand_is_never_duplicated() {
    let sandbox = Sandbox::new();
    let zshrc = sandbox.home().join(".zshrc");
    let mine = format!("export PATH=\"{}:$PATH\"\n", sandbox.bin().display());
    std::fs::write(&zshrc, &mine).expect("write zshrc");

    sandbox.ok(&["path", "install", "--shell", "zsh"]);
    assert_eq!(
        std::fs::read_to_string(&zshrc).expect("read zshrc"),
        mine,
        "ketch added a second copy of a line the user already had"
    );
}

/// Where the lockfile goes in a test: inside the sandbox, never the cwd the
/// suite happens to run from.
fn lock_at(sandbox: &Sandbox) -> std::path::PathBuf {
    sandbox.home().join("ketch.lock")
}

#[test]
fn a_lockfile_records_what_is_installed_and_sync_puts_it_back() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    let lock = lock_at(&sandbox);
    let lock_arg = lock.display().to_string();

    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["lock", "--file", &lock_arg]);

    let text = std::fs::read_to_string(&lock).expect("read lockfile");
    assert!(text.contains("name = \"testtool\""), "{text}");
    assert!(text.contains("tag = \"v1.0.0\""), "{text}");
    assert!(text.contains("source = \"test:testtool\""), "{text}");

    sandbox.ok(&["lock", "--check", "--file", &lock_arg]);

    // Wipe it, then let the lockfile put it back.
    sandbox.ok(&["uninstall", "testtool", "--yes"]);
    assert!(!sandbox.bin().join("testtool").exists());
    sandbox.fails(&["lock", "--check", "--file", &lock_arg]);

    sandbox.ok(&["sync", "--file", &lock_arg]);
    assert_eq!(run(&sandbox.bin().join("testtool")), "testtool 1.0.0");
    sandbox.ok(&["lock", "--check", "--file", &lock_arg]);
}

/// The reason to write versions down: a newer release exists and sync must
/// still produce the one that was locked.
#[test]
fn sync_installs_the_locked_tag_not_the_latest_one() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    let lock = lock_at(&sandbox);
    let lock_arg = lock.display().to_string();

    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["lock", "--file", &lock_arg]);

    // A newer release lands, and the machine takes it.
    let arch = host_arch();
    let newer = sandbox.asset(
        &format!("testtool-2.0.0-{arch}-apple-darwin.tar.gz"),
        tool_archive("2.0.0"),
    );
    let older = sandbox.asset(
        &format!("testtool-1.0.0-{arch}-apple-darwin.tar.gz"),
        tool_archive("1.0.0"),
    );
    sandbox.publish(
        "testtool",
        &[
            Release::new("2.0.0", vec![newer]),
            Release::new("1.0.0", vec![older]),
        ],
    );
    sandbox.ok(&["upgrade", "--yes"]);
    assert_eq!(run(&sandbox.bin().join("testtool")), "testtool 2.0.0");

    sandbox.fails(&["lock", "--check", "--file", &lock_arg]);
    sandbox.ok(&["sync", "--file", &lock_arg]);
    assert_eq!(run(&sandbox.bin().join("testtool")), "testtool 1.0.0");
}

/// A release replaced under a tag it already published is the thing a lockfile
/// exists to catch, and it must be caught before anything is unpacked.
#[test]
fn sync_refuses_a_payload_that_is_not_the_one_that_was_locked() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    let lock = lock_at(&sandbox);
    let lock_arg = lock.display().to_string();

    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["lock", "--file", &lock_arg]);
    sandbox.ok(&["uninstall", "testtool", "--yes"]);

    // Same tag, different bytes — exactly what a re-tagged release looks like.
    let text = std::fs::read_to_string(&lock).expect("read lockfile");
    let recorded = text
        .split_once("sha256 = \"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(hash, _)| hash.to_string())
        .expect("a sha256 in the lockfile");
    std::fs::write(&lock, text.replace(&recorded, &"b".repeat(64))).expect("rewrite lockfile");

    let said = sandbox.fails(&["sync", "--file", &lock_arg]);
    assert!(said.contains("does not match the lockfile"), "{said}");
    assert!(
        !sandbox.bin().join("testtool").exists(),
        "a payload that did not match the lock was installed anyway"
    );
}

#[test]
fn prune_removes_what_the_lockfile_does_not_name() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "1.0.0");
    let asset = sandbox.asset("TestApp-1.0.0-macos.zip", app_archive("1.0.0"));
    sandbox.publish("testapp", &[Release::new("1.0.0", vec![asset])]);
    let lock_arg = lock_at(&sandbox).display().to_string();

    sandbox.ok(&["install", "test:testtool", "--yes"]);
    sandbox.ok(&["lock", "--file", &lock_arg]);
    sandbox.ok(&["install", "test:testapp", "--yes"]);

    // An extra is not drift on its own — only `--prune` treats it as such.
    sandbox.ok(&["lock", "--check", "--file", &lock_arg]);
    sandbox.ok(&["sync", "--prune", "--yes", "--file", &lock_arg]);
    assert!(!sandbox.apps().join("TestApp.app").exists());
    assert_eq!(run(&sandbox.bin().join("testtool")), "testtool 1.0.0");
}

#[test]
fn a_lockfile_naming_a_path_instead_of_a_package_is_refused() {
    let sandbox = Sandbox::new();
    let lock = lock_at(&sandbox);
    std::fs::write(
        &lock,
        format!(
            "version = 1\n\n[[package]]\nname = \"../../.zshrc\"\nsource = \"test:testtool\"\n\
             version = \"1.0.0\"\ntag = \"1.0.0\"\ntarget = \"macos-aarch64\"\n\
             asset = \"t.tar.gz\"\nsha256 = \"{}\"\n",
            "a".repeat(64)
        ),
    )
    .expect("write lockfile");

    let said = sandbox.fails(&["sync", "--file", &lock.display().to_string()]);
    assert!(said.contains("not a usable package name"), "{said}");
}
