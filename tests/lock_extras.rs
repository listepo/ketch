//! The install lock held across two commands, and `extra_paths` travelling
//! with the package from install to uninstall.
//!
//! The lock is what keeps two `ketch upgrade` runs from each saving a
//! `state.json` that omits the other's package; the extras are what put a
//! payload's man page and completion where the user finds them. Offline,
//! like the rest of the suite.
#![cfg(unix)]

mod support;

use support::{host_arch, Archive, Entry, Release, Sandbox};

fn publish_tool(sandbox: &Sandbox, name: &str, version: &str) {
    let arch = host_arch();
    let native = sandbox.asset(
        &format!("{name}-{version}-{arch}-apple-darwin.tar.gz"),
        Archive::TarGz(vec![Entry::program(
            &format!("{name}-{version}/bin/{name}"),
            &format!("{name} {version}"),
        )]),
    );
    sandbox.publish(name, &[Release::new(version, vec![native])]);
}

/// Two `upgrade` runs cannot both hold the install-tree lock: the second
/// fails with the pid-holding message (exit 8) rather than losing an install.
#[test]
fn a_second_upgrade_while_the_lock_is_held_reports_the_holder() {
    let sandbox = Sandbox::new();
    publish_tool(&sandbox, "testtool", "1.0.0");
    sandbox.ok(&["install", "test:testtool", "--yes"]);

    let lock = sandbox.root().join(".lock");
    // A live foreign pid: our own pid would be adopted as a re-entrant lock
    // instead of failing, which is exactly the wrong thing to prove here.
    // The `sleep` is killed and reaped below: clippy's zombie-process lint
    // fires if the `Child` is never `wait()`ed on.
    let mut holder = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn lock holder");
    std::fs::write(&lock, format!("{}", holder.id())).expect("hold the lock");

    let out = sandbox.ketch(&["upgrade", "--yes"]);
    let _ = holder.kill();
    let _ = holder.wait();
    assert!(!out.status.success(), "upgrade succeeded under a held lock");
    assert_eq!(out.status.code(), Some(8));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("another ketch process holds the lock"),
        "wrong lock message:\n{stderr}"
    );
}

/// `extra_paths` travel with the package: install links the man page and the
/// completion into the user directories, and uninstall takes them back.
#[test]
fn extras_are_linked_on_install_and_removed_on_uninstall() {
    let sandbox = Sandbox::new();
    let arch = host_arch();
    let native = sandbox.asset(
        &format!("extratool-1.0.0-{arch}-apple-darwin.tar.gz"),
        Archive::TarGz(vec![
            Entry::program("extratool-1.0.0/bin/extratool", "extratool 1.0.0"),
            Entry::file("extratool-1.0.0/doc/extratool.1", ".TH EXTRATOOL 1\n"),
            Entry::file("extratool-1.0.0/complete/extratool.bash", "# completion\n"),
        ]),
    );
    sandbox.publish("extratool", &[Release::new("1.0.0", vec![native])]);
    let dir = sandbox.root().join("manifests");
    std::fs::create_dir_all(&dir).expect("manifests dir");
    std::fs::write(
        dir.join("extratool.toml"),
        "name = \"extratool\"\nsource = \"test:extratool\"\n",
    )
    .expect("write manifest");
    let mut manifest = String::from("name = \"extratool\"\nsource = \"test:extratool\"\n");
    manifest.push_str("extra_paths = [\"doc/extratool.1\", \"complete/extratool.bash\"]\n");
    std::fs::write(dir.join("extratool.toml"), manifest).expect("write manifest");

    // Man pages and completions live under the data home, so point it at the
    // sandbox: without this the test writes into the runner's own home.
    let fake_home = sandbox.home().join("data");
    std::fs::create_dir_all(&fake_home).expect("data home");
    sandbox.ok_env(
        &["install", "extratool", "--yes"],
        &[("XDG_DATA_HOME", fake_home.to_str().unwrap())],
    );

    let man = fake_home.join("man/man1/extratool.1");
    let completion = fake_home.join("bash-completion/completions/extratool");
    assert!(man.exists(), "man page not linked at {}", man.display());
    assert!(
        completion.exists(),
        "completion not linked at {}",
        completion.display()
    );

    sandbox.ok_env(
        &["uninstall", "extratool", "--yes"],
        &[("XDG_DATA_HOME", fake_home.to_str().unwrap())],
    );
    assert!(!man.exists(), "man page left behind at {}", man.display());
    assert!(
        !completion.exists(),
        "completion left behind at {}",
        completion.display()
    );
}
