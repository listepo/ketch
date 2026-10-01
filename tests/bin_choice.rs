//! End-to-end: which binary a package links when its release ships several
//! sharing the package's name and no manifest names one (B64).
//!
//! Runs on every OS, because the bug it guards against was an OS difference:
//! sorted by file name, `rtok-hook.exe` comes before `rtok.exe` on Windows
//! while `rtok` comes before `rtok-hook` everywhere else. Stdin is never a
//! terminal here, so these are the paths without a prompt; the order the
//! prompt sits in is proved by the unit tests in `crates/ketch-core/src/bin_choice.rs`.

mod support;

use std::path::{Path, PathBuf};
use support::{host_arch, Archive, Entry, Release, Sandbox};

/// The asset name this host's platform scores as native.
fn native_asset(version: &str) -> String {
    let arch = host_arch();
    if cfg!(target_os = "windows") {
        format!("rtok-{version}-{arch}-pc-windows-msvc.zip")
    } else if cfg!(target_os = "macos") {
        format!("rtok-{version}-{arch}-apple-darwin.tar.gz")
    } else {
        format!("rtok-{version}-{arch}-unknown-linux-gnu.tar.gz")
    }
}

/// Publish one `rtok` release whose payload holds `programs`, each printing
/// its own name and the version.
fn publish(sandbox: &Sandbox, version: &str, programs: &[&str]) {
    let mut entries: Vec<Entry> = programs
        .iter()
        .map(|p| Entry::program(&format!("rtok-{version}/{p}"), &format!("{p} {version}")))
        .collect();
    entries.push(Entry::file(
        &format!("rtok-{version}/README.md"),
        "# rtok\n",
    ));
    let archive = if cfg!(windows) {
        Archive::Zip(entries)
    } else {
        Archive::TarGz(entries)
    };
    let asset = sandbox.asset(&native_asset(version), archive);
    sandbox.publish("rtok", &[Release::new(version, vec![asset])]);
}

/// Where the program called `name` lands in the bin dir.
fn linked(sandbox: &Sandbox, name: &str) -> PathBuf {
    if cfg!(windows) {
        sandbox.bin().join(format!("{name}.cmd"))
    } else {
        sandbox.bin().join(name)
    }
}

fn run(path: &Path) -> String {
    let out = std::process::Command::new(path)
        .output()
        .expect("run installed program");
    assert!(out.status.success(), "{} did not run", path.display());
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn set_bin_choice(sandbox: &Sandbox, choice: &str) {
    let path = sandbox.root().join("state.json");
    let mut state: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read state"))
            .expect("parse state");
    state["packages"]["rtok"]["bin_choice"] = serde_json::json!(choice);
    std::fs::write(&path, serde_json::to_string_pretty(&state).unwrap()).expect("write state");
}

#[test]
fn the_binary_named_like_the_package_is_linked_and_its_helper_is_not() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok", "rtok-hook"]);

    sandbox.ok(&["install", "test:rtok@1.0.0", "--yes"]);

    assert_eq!(run(&linked(&sandbox, "rtok")), "rtok 1.0.0");
    assert!(
        !linked(&sandbox, "rtok-hook").exists(),
        "the helper must not be linked beside the command it serves"
    );
}

#[test]
fn without_a_terminal_an_ambiguous_release_fails_and_names_the_candidates() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok-cli", "rtok-hook"]);

    let err = sandbox.fails(&["install", "test:rtok@1.0.0"]);

    assert!(err.contains("rtok-cli"), "{err}");
    assert!(err.contains("rtok-hook"), "{err}");
    assert!(err.contains("bin = [{ name = "), "{err}");
    assert!(!linked(&sandbox, "rtok-cli").exists());
    assert!(!linked(&sandbox, "rtok-hook").exists());
}

#[test]
fn a_remembered_choice_is_reused_on_upgrade() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok-cli"]);
    sandbox.ok(&["install", "test:rtok@1.0.0", "--yes"]);
    assert_eq!(run(&linked(&sandbox, "rtok-cli")), "rtok-cli 1.0.0");

    // 2.0.0 adds a second binary sharing the name, and nothing says which.
    publish(&sandbox, "2.0.0", &["rtok-cli", "rtok-hook"]);
    let out = sandbox.ketch(&["upgrade", "--yes"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("rtok-hook"), "{err}");
    assert_eq!(run(&linked(&sandbox, "rtok-cli")), "rtok-cli 1.0.0");

    // What a pick at a terminal would have left behind.
    set_bin_choice(&sandbox, "rtok-cli");
    sandbox.ok(&["upgrade", "--yes"]);

    assert_eq!(run(&linked(&sandbox, "rtok-cli")), "rtok-cli 2.0.0");
    assert!(!linked(&sandbox, "rtok-hook").exists());
    let state = std::fs::read_to_string(sandbox.root().join("state.json")).unwrap();
    assert!(state.contains(r#""bin_choice": "rtok-cli"#), "{state}");
}

#[test]
fn a_user_manifest_gets_the_chosen_binary_written_into_it() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok", "rtok-hook", "other-tool"]);
    let manifests = sandbox.root().join("manifests");
    std::fs::create_dir_all(&manifests).unwrap();
    let file = manifests.join("rtok.toml");
    let original = "# my own rtok\nname   = \"rtok\"\nsource = \"test:rtok\"\n";
    std::fs::write(&file, original).unwrap();

    sandbox.ok(&["install", "rtok", "--yes"]);

    // Windows would link a bare name as `.exe`, so a script keeps its own.
    // A bare name equal to the package leads. A `.cmd` does not: only `.exe`
    // is folded away, so on Windows these two stay alphabetical.
    let ext = if cfg!(windows) { ".cmd" } else { "" };
    let (first, second) = if cfg!(windows) {
        (format!("other-tool{ext}"), format!("rtok{ext}"))
    } else {
        (format!("rtok{ext}"), format!("other-tool{ext}"))
    };
    let written = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        written,
        format!("{original}bin = [{{ name = \"{first}\" }}, {{ name = \"{second}\" }}]\n"),
        "everything but the new entry must be left as it was"
    );
    assert_eq!(run(&linked(&sandbox, "rtok")), "rtok 1.0.0");
    assert_eq!(run(&linked(&sandbox, "other-tool")), "other-tool 1.0.0");
    assert!(!linked(&sandbox, "rtok-hook").exists());

    // The file now names its binaries, so a reinstall leaves it alone.
    sandbox.ok(&["install", "rtok", "--force", "--yes"]);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
    assert_eq!(run(&linked(&sandbox, "rtok")), "rtok 1.0.0");
}

#[test]
fn only_the_losing_family_members_are_dropped_and_other_binaries_stay_linked() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok", "rtok-hook", "other-tool"]);

    sandbox.ok(&["install", "test:rtok@1.0.0", "--yes"]);

    assert_eq!(run(&linked(&sandbox, "rtok")), "rtok 1.0.0");
    assert_eq!(run(&linked(&sandbox, "other-tool")), "other-tool 1.0.0");
    assert!(!linked(&sandbox, "rtok-hook").exists());
}

#[test]
fn the_bin_flag_chooses_without_a_terminal_and_is_remembered() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok-cli", "rtok-hook"]);

    sandbox.ok(&["install", "test:rtok@1.0.0", "--bin", "rtok-cli"]);

    assert_eq!(run(&linked(&sandbox, "rtok-cli")), "rtok-cli 1.0.0");
    assert!(!linked(&sandbox, "rtok-hook").exists());
    let state = std::fs::read_to_string(sandbox.root().join("state.json")).unwrap();
    assert!(state.contains(r#""bin_choice": "rtok-cli"#), "{state}");

    publish(&sandbox, "2.0.0", &["rtok-cli", "rtok-hook"]);
    sandbox.ok(&["upgrade", "--yes"]);
    assert_eq!(run(&linked(&sandbox, "rtok-cli")), "rtok-cli 2.0.0");
    assert!(!linked(&sandbox, "rtok-hook").exists());
}

#[test]
fn the_bin_flag_wins_over_the_binary_named_like_the_package() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok", "rtok-hook"]);

    sandbox.ok(&["install", "test:rtok@1.0.0", "--bin", "rtok-hook"]);

    assert_eq!(run(&linked(&sandbox, "rtok-hook")), "rtok-hook 1.0.0");
    assert!(!linked(&sandbox, "rtok").exists());
}

#[test]
fn a_bin_flag_naming_no_binary_fails_and_lists_what_there_is() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok-cli", "rtok-hook"]);

    let err = sandbox.fails(&["install", "test:rtok@1.0.0", "--bin", "rtok-typo"]);
    assert!(err.contains("--bin `rtok-typo`"), "{err}");
    assert!(err.contains("rtok-cli"), "{err}");
    assert!(!linked(&sandbox, "rtok-cli").exists());

    // Nothing to choose between is no excuse for a name that matches nothing.
    publish(&sandbox, "2.0.0", &["rtok"]);
    let err = sandbox.fails(&["install", "test:rtok@2.0.0", "--bin", "rtok-typo"]);
    assert!(err.contains("--bin `rtok-typo`"), "{err}");
    assert!(!linked(&sandbox, "rtok").exists());
}

#[test]
fn sync_repeats_the_choice_the_lockfile_recorded_without_a_terminal() {
    let sandbox = Sandbox::new();
    publish(&sandbox, "1.0.0", &["rtok-cli", "rtok-hook"]);
    let lock = sandbox.home().join("ketch.lock");
    let lock_arg = lock.display().to_string();

    sandbox.ok(&["install", "test:rtok@1.0.0", "--bin", "rtok-cli"]);
    sandbox.ok(&["lock", "--file", &lock_arg]);
    sandbox.ok(&["uninstall", "rtok", "--yes"]);
    let text = std::fs::read_to_string(&lock).expect("read lockfile");
    assert!(text.contains("bin = \"rtok-cli"), "{text}");

    // Without the recorded choice a machine with no state has nothing to go by.
    let bare = sandbox.home().join("bare.lock");
    let without: String = text
        .lines()
        .filter(|l| !l.starts_with("bin = "))
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&bare, without).expect("write lockfile");
    let err = sandbox.fails(&["sync", "--file", &bare.display().to_string()]);
    assert!(err.contains("rtok-hook"), "{err}");

    sandbox.ok(&["sync", "--file", &lock_arg]);
    assert_eq!(run(&linked(&sandbox, "rtok-cli")), "rtok-cli 1.0.0");
    assert!(!linked(&sandbox, "rtok-hook").exists());
}
