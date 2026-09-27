//! `ketch list` in its three modes: `local`, `remote`, and both at once.
//!
//! The real binary against the offline test plugin and a registry written into
//! the sandbox, so `latest` comes from the production lookup and nothing
//! touches the network. Colour is off — the sandbox sets `NO_COLOR` — except
//! in the one test that forces it on to see what a terminal gets.
//!
//! The standard fixture, used by most tests:
//!
//! | package | registry | installed      | published      |
//! |---------|----------|----------------|----------------|
//! | alpha   | yes      | 1.0.0          | 1.0.0, 2.0.0   |
//! | bravo   | no       | 1.0.0          | 1.0.0          |
//! | charlie | yes      | 1.0.0, pinned  | 1.0.0, 1.5.0   |
//! | delta   | yes      | no             | 3.1.0          |

mod support;

use serde_json::Value;
use support::{host_arch, Archive, Entry, Release, Sandbox};

fn native_name(pkg: &str, version: &str) -> String {
    let arch = host_arch();
    #[cfg(target_os = "macos")]
    {
        format!("{pkg}-{version}-{arch}-apple-darwin.tar.gz")
    }
    #[cfg(target_os = "linux")]
    {
        format!("{pkg}-{version}-{arch}-unknown-linux-gnu.tar.gz")
    }
    #[cfg(target_os = "windows")]
    {
        format!("{pkg}-{version}-{arch}-pc-windows-msvc.zip")
    }
}

/// Serve `versions` of `id`, each with an asset this machine can install.
fn publish(sandbox: &Sandbox, id: &str, versions: &[&str]) {
    let releases: Vec<Release> = versions
        .iter()
        .map(|version| {
            let asset = sandbox.asset(
                &native_name(id, version),
                Archive::TarGz(vec![Entry::program(
                    &format!("{id}-{version}/bin/{id}"),
                    &format!("{id} {version}"),
                )]),
            );
            Release::new(version, vec![asset])
        })
        .collect();
    sandbox.publish(id, &releases);
}

fn registry(sandbox: &Sandbox, name: &str, description: &str) {
    sandbox.registry_package(
        name,
        &format!("name = \"{name}\"\nsource = \"test:{name}\"\ndescription = \"{description}\"\n"),
    );
}

/// The fixture in the table at the top of this file.
fn standard() -> Sandbox {
    let sandbox = Sandbox::new();
    registry(&sandbox, "alpha", "The first tool");
    registry(&sandbox, "charlie", "A tool held at one version");
    registry(&sandbox, "delta", "Not installed yet");
    publish(&sandbox, "alpha", &["1.0.0"]);
    publish(&sandbox, "bravo", &["1.0.0"]);
    publish(&sandbox, "charlie", &["1.0.0"]);
    publish(&sandbox, "delta", &["3.1.0"]);
    sandbox.ok(&["install", "alpha", "test:bravo", "charlie", "--yes"]);
    sandbox.ok(&["pin", "charlie"]);
    publish(&sandbox, "alpha", &["1.0.0", "2.0.0"]);
    publish(&sandbox, "charlie", &["1.0.0", "1.5.0"]);
    sandbox
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("invalid JSON ({e}):\n{text}"))
}

fn keys(object: &Value) -> Vec<String> {
    let mut keys: Vec<String> = object
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn row<'a>(out: &'a str, name: &str) -> &'a str {
    out.lines()
        .find(|line| line.split_whitespace().any(|word| word == name))
        .unwrap_or_else(|| panic!("no row for {name}:\n{out}"))
}

#[test]
fn list_local_shows_installed_packages_with_their_notes() {
    let sandbox = standard();
    insta::assert_snapshot!("local", sandbox.ok(&["list", "local"]));
}

#[test]
fn list_remote_shows_the_registry_with_latest_versions() {
    let sandbox = standard();
    insta::assert_snapshot!("remote", sandbox.ok(&["list", "remote"]));
}

#[test]
fn bare_list_shows_installed_and_available_packages_in_one_table() {
    let sandbox = standard();
    insta::assert_snapshot!("all", sandbox.ok(&["list"]));
}

#[test]
fn json_of_each_mode_has_exactly_the_documented_fields() {
    let sandbox = standard();

    let local = sandbox.ok(&["list", "local", "--json"]);
    for entry in json(&local).as_array().expect("an array") {
        assert_eq!(
            keys(entry),
            ["installed", "name", "pinned", "retained", "source"]
        );
    }
    insta::assert_snapshot!("local-json", local);

    let remote = sandbox.ok(&["list", "remote", "--json"]);
    for entry in json(&remote).as_array().expect("an array") {
        assert_eq!(keys(entry), ["description", "latest", "name", "source"]);
    }
    insta::assert_snapshot!("remote-json", remote);

    let all = sandbox.ok(&["list", "--json"]);
    let parsed = json(&all);
    assert_eq!(keys(&parsed), ["packages", "unreachable"]);
    for entry in parsed["packages"].as_array().expect("packages") {
        assert_eq!(
            keys(entry),
            [
                "installed",
                "latest",
                "name",
                "pinned",
                "source",
                "update_available"
            ]
        );
    }
    insta::assert_snapshot!("all-json", all);
}

#[test]
fn bare_list_marks_installed_packages_and_shows_both_versions_only_for_them() {
    let sandbox = standard();
    let out = sandbox.ok(&["list"]);
    assert!(row(&out, "alpha").starts_with("*  alpha    1.0.0"), "{out}");
    assert!(row(&out, "bravo").starts_with('*'), "{out}");
    assert!(row(&out, "charlie").starts_with('*'), "{out}");
    let delta = row(&out, "delta");
    assert!(delta.starts_with("   delta"), "{out}");
    let words: Vec<&str> = delta.split_whitespace().collect();
    assert_eq!(words, ["delta", "3.1.0", "test:delta"], "{out}");
    assert!(!out.contains('●'), "{out}");
}

#[test]
fn colour_forced_on_marks_installed_packages_with_a_dot_and_a_bold_name() {
    let sandbox = standard();
    let out = sandbox.ok_env(&["list"], &[("CLICOLOR_FORCE", "1")]);
    assert!(out.contains("●  \u{1b}[1malpha\u{1b}[0m"), "{out:?}");
    assert!(
        out.contains("2.0.0 \u{1b}[33m(update available)\u{1b}[0m"),
        "{out:?}"
    );
    assert!(!out.contains("*  "), "{out:?}");
    let delta = row(&out, "delta");
    assert!(!delta.contains('●'), "{delta:?}");
    assert!(!delta.contains("\u{1b}[1m"), "{delta:?}");
}

#[test]
fn only_an_installed_unpinned_package_with_a_newer_release_is_an_update() {
    let sandbox = standard();
    let out = sandbox.ok(&["list"]);
    assert!(
        row(&out, "alpha").contains("2.0.0 (update available)"),
        "{out}"
    );
    for name in ["bravo", "charlie", "delta"] {
        assert!(!row(&out, name).contains("update available"), "{out}");
    }
    assert_eq!(
        out.lines().last(),
        Some("1 update available: ketch upgrade alpha"),
        "{out}"
    );

    let parsed = json(&sandbox.ok(&["list", "--json"]));
    let updates: Vec<&str> = parsed["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .filter(|p| p["update_available"] == true)
        .map(|p| p["name"].as_str().expect("name"))
        .collect();
    assert_eq!(updates, ["alpha"]);
}

#[test]
fn a_pinned_package_with_a_newer_release_says_pinned_and_is_not_offered() {
    let sandbox = standard();
    let out = sandbox.ok(&["list"]);
    let charlie = row(&out, "charlie");
    assert!(charlie.contains("1.0.0 (pinned)"), "{out}");
    assert!(charlie.contains("1.5.0"), "{out}");
    assert!(!charlie.contains("update available"), "{out}");
    assert!(!out.contains("ketch upgrade alpha charlie"), "{out}");
}

#[test]
fn a_package_installed_from_outside_the_registry_gets_latest_from_its_own_source() {
    let sandbox = standard();
    publish(&sandbox, "bravo", &["1.0.0", "1.2.0"]);
    let out = sandbox.ok(&["list"]);
    let words: Vec<&str> = row(&out, "bravo").split_whitespace().collect();
    assert_eq!(
        words,
        [
            "*",
            "bravo",
            "1.0.0",
            "1.2.0",
            "(update",
            "available)",
            "test:bravo"
        ],
        "{out}"
    );
    assert!(!sandbox.ok(&["list", "remote"]).contains("bravo"));
}

#[test]
fn offline_bare_list_prints_the_local_part_and_says_so() {
    let sandbox = standard();
    for id in ["alpha", "bravo", "charlie", "delta"] {
        sandbox.unpublish(id);
    }
    let out = sandbox.ok(&["list"]);
    assert_eq!(
        out,
        format!("{}latest: offline\n", sandbox.ok(&["list", "local"]))
    );

    let parsed = json(&sandbox.ok(&["list", "--json"]));
    let names: Vec<&str> = parsed["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| p["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, ["alpha", "bravo", "charlie"]);
    assert!(parsed["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .all(|p| p["latest"].is_null()));
}

#[test]
fn offline_list_remote_fails_with_a_clear_message() {
    let sandbox = standard();
    for id in ["alpha", "charlie", "delta"] {
        sandbox.unpublish(id);
    }
    let err = sandbox.fails(&["list", "remote"]);
    assert!(err.contains("could not reach any package source"), "{err}");
    assert!(err.contains("ketch list local"), "{err}");
}

#[test]
fn one_unreachable_package_is_a_question_mark_not_a_failure() {
    let sandbox = standard();
    sandbox.unpublish("bravo");
    let out = sandbox.ok(&["list"]);
    let words: Vec<&str> = row(&out, "bravo").split_whitespace().collect();
    assert_eq!(words, ["*", "bravo", "1.0.0", "?", "test:bravo"], "{out}");
    for name in ["alpha", "charlie", "delta"] {
        row(&out, name);
    }
    assert!(
        out.contains("? means the latest release could not be checked: bravo"),
        "{out}"
    );
    assert!(!out.contains("offline"), "{out}");

    let parsed = json(&sandbox.ok(&["list", "--json"]));
    assert_eq!(parsed["unreachable"], serde_json::json!(["bravo"]));
    assert_eq!(parsed["packages"].as_array().expect("packages").len(), 4);
}

#[test]
fn names_only_prints_the_names_of_each_mode() {
    let sandbox = standard();
    // No lookups are needed for names, so a source that cannot answer changes
    // nothing here.
    sandbox.unpublish("delta");
    assert_eq!(
        sandbox.ok(&["list", "local", "--names-only"]),
        "alpha\nbravo\ncharlie\n"
    );
    assert_eq!(
        sandbox.ok(&["list", "remote", "--names-only"]),
        "alpha\ncharlie\ndelta\n"
    );
    assert_eq!(
        sandbox.ok(&["list", "--names-only"]),
        "alpha\nbravo\ncharlie\ndelta\n"
    );
}

#[test]
fn the_hidden_installed_flag_is_list_local() {
    let sandbox = standard();
    for extra in [&[][..], &["--json"][..], &["--names-only"][..]] {
        let mut local = vec!["list", "local"];
        local.extend_from_slice(extra);
        let mut alias = vec!["list", "--installed"];
        alias.extend_from_slice(extra);
        assert_eq!(sandbox.ok(&alias), sandbox.ok(&local), "{extra:?}");
    }
    let out = sandbox.ketch(&["list", "--installed"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("ketch list local"));
}

#[test]
fn empty_modes_say_what_is_missing() {
    let sandbox = Sandbox::new();
    assert_eq!(sandbox.ok(&["list", "local"]), "nothing installed\n");
    assert_eq!(
        sandbox.ok(&["list", "remote"]),
        "registry is empty; run ketch update\n"
    );
    assert_eq!(
        sandbox.ok(&["list"]),
        "nothing installed\nregistry is empty; run ketch update\n"
    );
    assert_eq!(sandbox.ok(&["list", "local", "--json"]).trim(), "[]");
    assert_eq!(sandbox.ok(&["list", "remote", "--json"]).trim(), "[]");
}

#[test]
fn latest_versions_are_cached_for_a_short_while() {
    let sandbox = standard();
    assert!(row(&sandbox.ok(&["list"]), "delta").contains("3.1.0"));
    assert!(sandbox.root().join("cache").join("latest.json").is_file());

    // A release published a moment later is not asked about again yet, and a
    // source that stopped answering is still answered from the cache.
    publish(&sandbox, "delta", &["3.1.0", "3.2.0"]);
    sandbox.unpublish("alpha");
    let out = sandbox.ok(&["list"]);
    assert!(row(&out, "delta").contains("3.1.0"), "{out}");
    assert!(row(&out, "alpha").contains("2.0.0"), "{out}");

    // `ketch list local` never reads or needs it.
    std::fs::remove_file(sandbox.root().join("cache").join("latest.json")).expect("remove cache");
    sandbox.ok(&["list", "local"]);
    assert!(!sandbox.root().join("cache").join("latest.json").exists());
}

#[cfg(unix)]
#[test]
fn a_package_installed_from_a_local_path_is_listed_without_a_latest() {
    use std::os::unix::fs::PermissionsExt;
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("pathtool");
    std::fs::write(&fixture, "#!/bin/sh\necho path\n").expect("write program");
    std::fs::set_permissions(&fixture, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let path = fixture.display().to_string();
    sandbox.ok(&["install", "--path", &path, "--yes"]);

    // Nothing to ask is not the same as nobody answering.
    let out = sandbox.ok(&["list"]);
    assert!(row(&out, "pathtool").starts_with('*'), "{out}");
    assert!(!out.contains("offline"), "{out}");
    assert!(!out.contains('?'), "{out}");
    let parsed = json(&sandbox.ok(&["list", "--json"]));
    assert!(parsed["packages"][0]["latest"].is_null(), "{parsed}");
    assert_eq!(parsed["unreachable"], serde_json::json!([]));
}
