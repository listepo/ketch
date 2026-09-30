//! The workspace split: the `ketch` binary over the `ketch-core` library.
//!
//! What the split promises and nothing in the code enforces on its own: the
//! core reports the binary's version (user agent, `--version`, statistics), it
//! is never packaged or published by itself, and both crates keep the same
//! lint floor. A drift in any of these would build and pass every other test.

use std::path::Path;

fn manifest(rel: &str) -> toml::Table {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let text = std::fs::read_to_string(&path).expect("manifest is readable");
    toml::from_str(&text).expect("manifest parses")
}

fn get<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (last, parents) = path.split_last()?;
    let mut at = table;
    for key in parents {
        at = at.get(*key)?.as_table()?;
    }
    at.get(*last)
}

#[test]
fn the_core_reports_the_version_the_binary_was_built_as() {
    assert_eq!(
        ketch_core::self_update::current_version().to_string(),
        env!("CARGO_PKG_VERSION")
    );
}

#[test]
fn both_crates_inherit_one_workspace_version() {
    let root = manifest("Cargo.toml");
    let core = manifest("crates/ketch-core/Cargo.toml");
    for (name, table) in [("ketch", &root), ("ketch-core", &core)] {
        assert_eq!(
            get(table, &["package", "version", "workspace"]).and_then(|v| v.as_bool()),
            Some(true),
            "{name} must take its version from [workspace.package]"
        );
    }
    assert!(get(&root, &["workspace", "package", "version"]).is_some());
}

#[test]
fn the_core_is_never_published_or_packaged_by_dist() {
    let core = manifest("crates/ketch-core/Cargo.toml");
    assert_eq!(
        get(&core, &["package", "publish"]).and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        get(&core, &["package", "metadata", "dist", "dist"]).and_then(|v| v.as_bool()),
        Some(false)
    );
}

#[test]
fn both_crates_forbid_unsafe_code_through_the_workspace_lints() {
    let root = manifest("Cargo.toml");
    assert_eq!(
        get(&root, &["workspace", "lints", "rust", "unsafe_code"]).and_then(|v| v.as_str()),
        Some("forbid")
    );
    let core = manifest("crates/ketch-core/Cargo.toml");
    for (name, table) in [("ketch", &root), ("ketch-core", &core)] {
        assert_eq!(
            get(table, &["lints", "workspace"]).and_then(|v| v.as_bool()),
            Some(true),
            "{name} must inherit [workspace.lints]"
        );
    }
}
