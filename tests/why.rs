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
    let mut out = redact_root_paths(text, sandbox);
    out = out.replace(&host_target(), "{target}");

    // Asset names embed arch + OS triple; replace before `{arch}` so one
    // snapshot set works on macOS, Linux, and Windows.
    for (pkg, label) in [("whypkg", "whypkg"), ("ripgrep", "ripgrep")] {
        for version in ["2.0.0-rc.1", "1.0.0"] {
            out = out.replace(
                &native_name(pkg, version),
                &format!("{{{label}-native-{version}}}"),
            );
            out = out.replace(
                &foreign_name(pkg, version),
                &format!("{{{label}-foreign-{version}}}"),
            );
        }
    }

    out = out.replace(host_arch(), "{arch}");

    for reason in [
        "darwin / {arch} / tar.gz",
        "linux / gnu / {arch} / tar.gz",
        "windows / {arch} / zip",
    ] {
        out = out.replace(reason, "{native-reason}");
    }

    normalize_why_text(&out)
}

/// Wipe absolute sandbox roots, including Windows short-path / JSON-escaped forms.
fn redact_root_paths(text: &str, sandbox: &Sandbox) -> String {
    let root = sandbox.root();
    let mut out = text.to_string();
    let mut forms = vec![root.display().to_string()];
    if let Ok(canonical) = root.canonicalize() {
        forms.push(canonical.display().to_string());
    }
    if let Some(parent) = root.parent() {
        forms.push(parent.join("root").display().to_string());
    }
    for form in forms {
        for candidate in [
            form.clone(),
            form.replace('\\', "/"),
            form.replace('\\', "\\\\"),
        ] {
            out = out.replace(&candidate, "{root}");
        }
    }
    // When display()/canonicalize() disagree with the path ketch printed
    // (Windows 8.3 short names, mixed separators, JSON escapes), scrub by
    // known relative suffixes under the sandbox root.
    out = scrub_through_root_suffix(&out, "/manifests/", "/manifests/");
    out = scrub_through_root_suffix(&out, "\\manifests\\", "/manifests/");
    out = scrub_through_root_suffix(&out, "\\\\manifests\\\\", "/manifests/");
    out = scrub_through_root_suffix(&out, "/registry/", "/registry/");
    out = scrub_through_root_suffix(&out, "\\registry\\", "/registry/");
    out = scrub_through_root_suffix(&out, "\\\\registry\\\\", "/registry/");
    out = out.replace("{root}\\", "{root}/");
    out = out.replace("{root}\\\\", "{root}/");
    out
}

/// Replace `…<anything>root<needle>rest` with `{root}<canonical_needle>rest`.
fn scrub_through_root_suffix(text: &str, needle: &str, canonical_needle: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(idx) = rest.find(needle) {
        let before = &rest[..idx];
        // Walk left to the start of the absolute path token.
        let start = before
            .char_indices()
            .rev()
            .find(|(_, c)| *c == '"' || c.is_whitespace() || *c == '=')
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        out.push_str(&before[..start]);
        out.push_str("{root}");
        out.push_str(canonical_needle);
        rest = &rest[idx + needle.len()..];
    }
    out.push_str(rest);
    out
}

/// Make text snapshots OS-agnostic: scores (except pin), and table padding that
/// still reflects the pre-redaction asset-name widths, are normalized here.
fn normalize_why_text(text: &str) -> String {
    let mut in_table = false;
    let mut out_lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed == "assets" || trimmed == "rejected" {
            in_table = true;
            out_lines.push(trimmed.to_string());
            continue;
        }
        if trimmed.is_empty() {
            in_table = false;
            out_lines.push(String::new());
            continue;
        }
        if trimmed.starts_with("checksum")
            || trimmed.starts_with("trust")
            || trimmed.starts_with("candidate")
        {
            in_table = false;
        }

        let mut row = trimmed.to_string();
        if in_table {
            let mut cells = split_table_cols(&row);
            if cells.first().is_some_and(|c| {
                c.chars().all(|ch| ch.is_ascii_digit()) && c.as_str() != "2147483647"
            }) && cells.len() >= 2
            {
                cells[0] = "{score}".into();
            }
            row = cells.join("  ");
        }
        out_lines.push(row);
    }
    let mut body = out_lines.join("\n");
    if text.ends_with('\n') {
        body.push('\n');
    }
    body
}

/// Split a `ui::table` row on the two-space column gap without breaking
/// single spaces inside a cell (for example `incompatible with {target}`).
fn split_table_cols(row: &str) -> Vec<String> {
    let bytes = row.as_bytes();
    let mut cols = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b' ' && bytes[i + 1] == b' ' {
            let col = row[start..i].trim_end();
            if !col.is_empty() || !cols.is_empty() {
                cols.push(col.to_string());
            }
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            start = i;
            continue;
        }
        i += 1;
    }
    let col = row[start..].trim_end();
    if !col.is_empty() {
        cols.push(col.to_string());
    }
    cols
}

fn parse_json(stdout: &str) -> Value {
    serde_json::from_str(stdout.trim()).expect("why --json is JSON")
}

fn normalize_why_json(value: &mut Value) {
    if let Some(origin) = value.pointer_mut("/manifest/origin") {
        if let Some(raw) = origin.as_str() {
            *origin = Value::String(canonical_manifest_origin(raw));
        }
    }
    let pin = 2147483647_i64;
    if let Some(arr) = value
        .pointer_mut("/assets/scored")
        .and_then(|v| v.as_array_mut())
    {
        for item in arr {
            if let Some(score) = item.get_mut("score") {
                if score.as_i64() != Some(pin) {
                    *score = Value::String("{score}".into());
                }
            }
        }
    }
    if let Some(score) = value.pointer_mut("/candidate/score") {
        if score.as_i64() != Some(pin) {
            *score = Value::String("{score}".into());
        }
    }
}

fn canonical_manifest_origin(origin: &str) -> String {
    for (needle, canon) in [
        ("/manifests/", "/manifests/"),
        ("\\manifests\\", "/manifests/"),
        ("/registry/", "/registry/"),
        ("\\registry\\", "/registry/"),
    ] {
        if let Some(idx) = origin.find(needle) {
            let rest = origin[idx + needle.len()..].replace('\\', "/");
            return format!("{{root}}{canon}{rest}");
        }
    }
    if origin.starts_with("{root}") {
        return origin.replace('\\', "/");
    }
    origin.to_string()
}

fn snapshot_json(name: &str, json: &str, sandbox: &Sandbox) {
    let mut value = parse_json(&redact(json, sandbox));
    normalize_why_json(&mut value);
    let pretty = serde_json::to_string_pretty(&value).expect("pretty json") + "\n";
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
