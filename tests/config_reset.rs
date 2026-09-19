//! End-to-end tests for `ketch config reset`, which writes `config.toml` with
//! the compiled defaults after backing up the existing file.

mod support;

use support::Sandbox;

#[test]
fn reset_writes_defaults_and_backs_up_the_old_file() {
    let sandbox = Sandbox::new();
    sandbox.configure("prerelease = true\n");

    let before = sandbox.root().join("config.toml");
    assert!(before.is_file());

    let out = sandbox.ketch(&["config", "reset", "--yes"]);
    assert!(
        out.status.success(),
        "reset failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let body = std::fs::read_to_string(&before).unwrap();
    assert!(body.contains("Written by `ketch config reset`"), "{body}");
    assert!(body.contains("auto_update = true"), "{body}");

    let backups: Vec<_> = std::fs::read_dir(sandbox.root())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("config.toml.bak-")
        })
        .collect();
    assert_eq!(backups.len(), 1, "one backup beside the reset file");
    assert_eq!(
        std::fs::read_to_string(backups[0].path()).unwrap(),
        "prerelease = true\n"
    );

    // A second reset with changed bytes (the defaults now, vs the old backup)
    // backs up again; a third finds the defaults already backed up and stops.
    let out = sandbox.ketch(&["config", "reset", "--yes"]);
    assert!(out.status.success());
    let count = || {
        std::fs::read_dir(sandbox.root())
            .unwrap()
            .filter(|e| {
                e.as_ref().is_ok_and(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("config.toml.bak-")
                })
            })
            .count()
    };
    assert_eq!(count(), 2, "changed bytes are copied again");
    let out = sandbox.ketch(&["config", "reset", "--yes"]);
    assert!(out.status.success());
    assert_eq!(count(), 2, "identical bytes are not copied again");
}

#[test]
fn reset_without_a_file_writes_defaults_and_no_backup() {
    let sandbox = Sandbox::new();
    let path = sandbox.root().join("config.toml");
    assert!(!path.exists());

    sandbox.ok(&["config", "reset", "--yes"]);

    let body = std::fs::read_to_string(&path).unwrap();
    assert!(body.contains("jobs = 4"), "{body}");
    let backups: Vec<_> = std::fs::read_dir(sandbox.root())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("config.toml.bak-")
        })
        .collect();
    assert!(backups.is_empty(), "a missing file is not copied");
}

#[test]
fn reset_asks_first_without_yes() {
    let sandbox = Sandbox::new();
    sandbox.configure("prerelease = true\n");

    // Non-interactive stdin answers with the default (no), so the file stays.
    let out = sandbox.ketch(&["config", "reset"]);
    assert!(out.status.success());
    assert_eq!(
        std::fs::read_to_string(sandbox.root().join("config.toml")).unwrap(),
        "prerelease = true\n"
    );
}
