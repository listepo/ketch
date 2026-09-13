//! Binary-level `ketch why` tests.
//!
//! These drive the real binary against the offline test plugin so the
//! explanation is the production resolver, not a reconstructed one.

mod support;

use serde_json::Value;
use std::path::Path;
use support::{host_arch, Archive, Entry, Release, Sandbox};

fn tool_archive(version: &str) -> Archive {
    Archive::TarGz(vec![Entry::program(
        &format!("whypkg-{version}/bin/whypkg"),
        &format!("whypkg {version}"),
    )])
}

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

fn foreign_name(pkg: &str, version: &str) -> String {
    let arch = host_arch();
    #[cfg(target_os = "macos")]
    {
        format!("{pkg}-{version}-{arch}-unknown-linux-gnu.tar.gz")
    }
    #[cfg(not(target_os = "macos"))]
    {
        format!("{pkg}-{version}-{arch}-apple-darwin.tar.gz")
    }
}

fn host_target() -> String {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "windows"
    };
    format!("{os}-{}", host_arch())
}

fn publish_pair(sandbox: &Sandbox, id: &str, version: &str) {
    let native = sandbox.asset(&native_name(id, version), tool_archive(version));
    let foreign = sandbox.asset(&foreign_name(id, version), tool_archive("foreign"));
    sandbox.publish(id, &[Release::new(version, vec![foreign, native])]);
}

fn write_user_manifest(sandbox: &Sandbox, name: &str, body: &str) {
    let dir = sandbox.root().join("manifests");
    std::fs::create_dir_all(&dir).expect("manifests dir");
    std::fs::write(dir.join(format!("{name}.toml")), body).expect("write user manifest");
}

fn write_registry_package(sandbox: &Sandbox, name: &str, body: &str) {
    let dir = sandbox.root().join("registry").join(name);
    std::fs::create_dir_all(&dir).expect("registry package dir");
    std::fs::write(dir.join("ketch.toml"), body).expect("write registry manifest");
}

fn redact(text: &str, sandbox: &Sandbox) -> String {
    text.replace(&sandbox.root().display().to_string(), "{root}")
        .replace(&host_target(), "{target}")
        .replace(host_arch(), "{arch}")
}

fn parse_json(stdout: &str) -> Value {
    serde_json::from_str(stdout.trim()).expect("why --json is JSON")
}

fn snapshot_json(name: &str, json: &str, sandbox: &Sandbox) {
    let pretty = serde_json::to_string_pretty(&parse_json(&redact(json, sandbox)))
        .expect("pretty json")
        + "\n";
    assert_snapshot(&format!("{name}.json"), &pretty);
}

fn snapshot_text(name: &str, text: &str, sandbox: &Sandbox) {
    assert_snapshot(&format!("{name}.txt"), &redact(text, sandbox));
}

fn assert_snapshot(name: &str, body: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots/why")
        .join(name);
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some();
    match std::fs::read_to_string(&path) {
        Ok(expected) if expected == body => {}
        Ok(_) if update => {
            std::fs::write(&path, body).expect("update snapshot");
        }
        Ok(expected) => {
            panic!("snapshot {name} drifted\n--- expected ---\n{expected}\n--- actual ---\n{body}")
        }
        Err(_) if update => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("snapshot dir");
            }
            std::fs::write(&path, body).expect("write snapshot");
        }
        Err(_) => panic!(
            "missing snapshot {}; run with UPDATE_SNAPSHOTS=1",
            path.display()
        ),
    }
}

#[test]
fn why_json_and_text_for_a_user_manifest_alias() {
    let sandbox = Sandbox::new();
    publish_pair(&sandbox, "whypkg", "1.0.0");
    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\nprovides = [\"whyalias\"]\n",
    );

    let json = sandbox.ok(&["why", "whyalias", "--json"]);
    let parsed = parse_json(&json);
    assert_eq!(parsed["manifest"]["tier"], "user");
    assert_eq!(parsed["manifest"]["name"], "whypkg");
    assert_eq!(parsed["manifest"]["matched"], "whyalias");
    assert_eq!(parsed["candidate"]["name"], native_name("whypkg", "1.0.0"));
    assert!(
        parsed["assets"]["rejected"]
            .as_array()
            .is_some_and(|r| !r.is_empty()),
        "foreign asset should be rejected: {json}"
    );
    snapshot_json("alias", &json, &sandbox);

    let text = sandbox.ok(&["why", "whyalias"]);
    assert!(text.contains("user"), "{text}");
    assert!(text.contains("whyalias"), "{text}");
    snapshot_text("alias", &text, &sandbox);
}

#[test]
fn why_prefers_a_user_manifest_over_the_registry() {
    let sandbox = Sandbox::new();
    publish_pair(&sandbox, "whypkg", "1.0.0");
    write_registry_package(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\n",
    );
    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\n",
    );
    let json = sandbox.ok(&["why", "whypkg", "--json"]);
    assert_eq!(parse_json(&json)["manifest"]["tier"], "user");
    snapshot_json("user-over-registry", &json, &sandbox);
    snapshot_text(
        "user-over-registry",
        &sandbox.ok(&["why", "whypkg"]),
        &sandbox,
    );
}

#[test]
fn why_prefers_the_registry_over_builtin() {
    let sandbox = Sandbox::new();
    publish_pair(&sandbox, "ripgrep", "1.0.0");
    write_registry_package(
        &sandbox,
        "ripgrep",
        "name = \"ripgrep\"\nsource = \"test:ripgrep\"\nprovides = [\"rg\"]\n",
    );
    let json = sandbox.ok(&["why", "rg", "--json"]);
    let parsed = parse_json(&json);
    assert_eq!(parsed["manifest"]["tier"], "registry");
    assert_eq!(parsed["source"]["scheme"], "test");
    snapshot_json("registry-over-builtin", &json, &sandbox);
    snapshot_text(
        "registry-over-builtin",
        &sandbox.ok(&["why", "rg"]),
        &sandbox,
    );
}

#[test]
fn why_selects_stable_over_a_prerelease_unless_the_manifest_asks() {
    let sandbox = Sandbox::new();
    let stable = sandbox.asset(&native_name("whypkg", "1.0.0"), tool_archive("1.0.0"));
    let pre = sandbox.asset(
        &native_name("whypkg", "2.0.0-rc.1"),
        tool_archive("2.0.0-rc.1"),
    );
    sandbox.publish(
        "whypkg",
        &[
            Release::new("2.0.0-rc.1", vec![pre]).into_prerelease(),
            Release::new("1.0.0", vec![stable]),
        ],
    );
    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\n",
    );

    let json = sandbox.ok(&["why", "whypkg", "--json"]);
    let parsed = parse_json(&json);
    assert_eq!(parsed["version"]["selected"]["version"], "1.0.0");
    assert_eq!(parsed["version"]["include_prerelease"], false);
    snapshot_json("prerelease-excluded", &json, &sandbox);
    snapshot_text(
        "prerelease-excluded",
        &sandbox.ok(&["why", "whypkg"]),
        &sandbox,
    );

    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\nprerelease = true\n",
    );
    let pre_json = sandbox.ok(&["why", "whypkg", "--json"]);
    assert_eq!(
        parse_json(&pre_json)["version"]["selected"]["version"],
        "2.0.0-rc.1"
    );
    snapshot_json("prerelease-included", &pre_json, &sandbox);
    snapshot_text(
        "prerelease-included",
        &sandbox.ok(&["why", "whypkg"]),
        &sandbox,
    );
}

#[test]
fn why_explains_a_pinned_asset() {
    let sandbox = Sandbox::new();
    let pinned = sandbox.asset("whypkg-pinned.tar.gz", tool_archive("1.0.0"));
    let other = sandbox.asset(&native_name("whypkg", "1.0.0"), tool_archive("1.0.0"));
    sandbox.publish("whypkg", &[Release::new("1.0.0", vec![other, pinned])]);
    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\n\n[asset.target]\n\
             \"macos-aarch64\" = \"*-pinned.tar.gz\"\n\
             \"macos-x86_64\" = \"*-pinned.tar.gz\"\n\
             \"linux-x86_64\" = \"*-pinned.tar.gz\"\n\
             \"linux-aarch64\" = \"*-pinned.tar.gz\"\n\
             \"windows-x86_64\" = \"*-pinned.tar.gz\"\n\
             \"windows-aarch64\" = \"*-pinned.tar.gz\"\n",
    );
    let json = sandbox.ok(&["why", "whypkg", "--json"]);
    let parsed = parse_json(&json);
    assert_eq!(parsed["candidate"]["name"], "whypkg-pinned.tar.gz");
    assert!(
        parsed["candidate"]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("pins")),
        "{json}"
    );
    snapshot_json("pinned-asset", &json, &sandbox);
    snapshot_text("pinned-asset", &sandbox.ok(&["why", "whypkg"]), &sandbox);
}

#[test]
fn why_reports_no_compatible_asset() {
    let sandbox = Sandbox::new();
    let foreign = sandbox.asset(&foreign_name("whypkg", "1.0.0"), tool_archive("1.0.0"));
    sandbox.publish("whypkg", &[Release::new("1.0.0", vec![foreign])]);
    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\n",
    );
    let out = sandbox.ketch(&["why", "whypkg", "--json"]);
    assert!(!out.status.success(), "no compatible asset must fail");
    let json = String::from_utf8_lossy(&out.stdout).to_string();
    let parsed = parse_json(&json);
    assert!(parsed["candidate"].is_null(), "{json}");
    assert!(
        parsed["assets"]["rejected"]
            .as_array()
            .is_some_and(|r| !r.is_empty()),
        "{json}"
    );
    snapshot_json("no-compatible-asset", &json, &sandbox);
    let text_out = sandbox.ketch(&["why", "whypkg"]);
    assert!(!text_out.status.success());
    snapshot_text(
        "no-compatible-asset",
        &String::from_utf8_lossy(&text_out.stdout),
        &sandbox,
    );
}

#[test]
fn why_json_keeps_secrets_and_bidi_out() {
    let sandbox = Sandbox::new();
    publish_pair(&sandbox, "whypkg", "1.0.0");
    write_user_manifest(
        &sandbox,
        "whypkg",
        "name = \"whypkg\"\nsource = \"test:whypkg\"\ndescription = \"safe\u{202e}evil\"\n",
    );
    let json = sandbox.ok(&["why", "whypkg", "--json"]);
    assert!(!json.contains('\u{202e}'), "{json:?}");
    assert!(!json.contains("SECRET"), "{json}");
    assert!(!json.contains("authorization"), "{json}");
    snapshot_json("no-secrets", &json, &sandbox);
}
