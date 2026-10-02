// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! Every TOML file ketch owns is read, written and described here.
//!
//! `config.toml`, the registry's metadata, package files and `ketch.lock`
//! each have Rust types; this module turns text into those types and back, and
//! publishes their JSON Schemas. Keeping the `toml` crate behind one module
//! means one answer to how a parse error names its file and how a file is
//! rendered, and one place to look when the format or the crate changes.

use crate::error::{Error, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
#[cfg(test)]
use std::path::Path;

/// Parse `text` into `T`. `what` names the file in the error, so a user can
/// find the line serde complains about.
pub(crate) fn parse<T: DeserializeOwned>(text: &str, what: impl Into<String>) -> Result<T> {
    toml::from_str(text).map_err(|e| Error::parse(what, e.to_string()))
}

/// Render `value` as a TOML document, tables expanded and arrays one item per
/// line where the value is long enough to need it.
pub(crate) fn render<T: Serialize>(value: &T, what: impl Into<String>) -> Result<String> {
    toml::to_string_pretty(value).map_err(|e| Error::parse(what, e.to_string()))
}

/// A quoted, escaped TOML string, for text that writes TOML line by line.
///
/// Built by rendering a `toml::Value` so escaping is never hand-rolled: one
/// writer, one answer to what quotes, backslashes and control bytes mean.
pub(crate) fn string_literal(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

/// A TOML array of strings, escaped the same way [`string_literal`] escapes.
pub(crate) fn string_list_literal(items: &[String]) -> String {
    toml::Value::Array(
        items
            .iter()
            .map(|i| toml::Value::String(i.clone()))
            .collect(),
    )
    .to_string()
}

/// A parsed TOML document whose keys a caller reads, or fills in, before it
/// becomes a typed value.
///
/// A registry folder's `ketch.toml` may leave `name` out, because the folder
/// supplies it; serde cannot see the folder, so the key is added to the
/// document first and the result deserialized as if the file had said it.
pub(crate) struct Document {
    table: toml::Table,
    what: String,
}

impl Document {
    /// Parse `text`; `what` names the file in this and every later error.
    pub(crate) fn parse(text: &str, what: impl Into<String>) -> Result<Self> {
        let what = what.into();
        // A TOML document is a table by definition, so parsing into one is
        // the whole shape check; no other top level can come back.
        let table = parse(text, what.as_str())?;
        Ok(Self { table, what })
    }

    /// The top-level `key`, when it is a string.
    pub(crate) fn str(&self, key: &str) -> Option<&str> {
        self.table.get(key).and_then(toml::Value::as_str)
    }

    /// Set the top-level `key` to the string `value`.
    pub(crate) fn set_str(&mut self, key: &str, value: &str) {
        self.table
            .insert(key.to_string(), toml::Value::String(value.to_string()));
    }

    /// The document as JSON, for code that works on `serde_json::Value`.
    pub(crate) fn into_json(self) -> Result<serde_json::Value> {
        serde_json::to_value(toml::Value::Table(self.table))
            .map_err(|e| Error::parse(self.what, e.to_string()))
    }

    /// Deserialize the document, keys added or not, into `T`.
    pub(crate) fn deserialize<T: DeserializeOwned>(self) -> Result<T> {
        T::deserialize(toml::Value::Table(self.table))
            .map_err(|e| Error::parse(self.what, e.to_string()))
    }
}

/// Fails when the JSON Schema committed at `relative` (from the repository
/// root) is not what `T` generates. `KETCH_BLESS=1` rewrites the file
/// instead: the types are the source, the file only publishes them.
///
/// The schema files stay next to the other docs, while this crate's manifest
/// is `crates/ketch-core`, so the repository root is two directories up.
#[cfg(test)]
pub(crate) fn assert_schema_current<T: schemars::JsonSchema>(relative: &str) {
    // TOML has no null: an absent key is how an `Option` says `None`, so a
    // schema allowing `null`, or offering it as a default an editor fills
    // in, would describe a file ketch cannot read.
    let drop_null = schemars::transform::RecursiveTransform(|s: &mut schemars::Schema| {
        if s.get("default").is_some_and(serde_json::Value::is_null) {
            s.remove("default");
        }
        if let Some(serde_json::Value::Array(types)) = s.get_mut("type") {
            types.retain(|t| t != "null");
            if let [only] = types.as_slice() {
                let only = only.clone();
                s.insert("type".into(), only);
            }
        }
    });
    let mut schema = schemars::generate::SchemaSettings::draft2020_12()
        .with_transform(drop_null)
        .into_generator()
        .into_root_schema_for::<T>();
    schema.insert(
        "$comment".into(),
        "Generated from the Rust types by `KETCH_BLESS=1 cargo nextest run schema`. Do not edit."
            .into(),
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    let rendered = serde_json::to_string_pretty(&schema).expect("render schema") + "\n";
    if std::env::var_os("KETCH_BLESS").is_some() {
        std::fs::write(&path, &rendered).expect("write schema");
        return;
    }
    // A Windows checkout may have turned LF into CRLF; the schema is the same.
    let committed = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    pretty_assertions::assert_eq!(
        committed,
        rendered,
        "{relative} is stale; regenerate it with KETCH_BLESS=1 cargo nextest run schema"
    );
}

/// Whether `line` reaches the `toml` or `toml_edit` crate: a path through
/// one (`toml::from_str`) or an import of one (`use toml_edit;`). File names
/// such as `"ketch.toml"` and mentions in comments do not count.
#[cfg(test)]
fn names_toml_crate(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    let imports = ["use ", "pub use ", "pub(crate) use ", "extern crate "]
        .iter()
        .any(|p| code.starts_with(p));
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    ["toml_edit", "toml"].iter().any(|name| {
        code.match_indices(name).any(|(at, _)| {
            let before = code[..at].chars().next_back();
            let after = &code[at + name.len()..];
            let whole = !before.is_some_and(|c| ident(c) || c == '.' || c == '-')
                && !after.starts_with(ident);
            whole && (after.trim_start().starts_with("::") || imports)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Sample {
        name: String,
        tags: Vec<String>,
    }

    #[test]
    fn a_parse_error_names_the_file() {
        let err = parse::<Sample>("name = ", "/root/config.toml").unwrap_err();
        assert!(err.to_string().contains("/root/config.toml"), "{err}");
    }

    #[test]
    fn rendered_text_parses_back_to_the_same_value() {
        let sample = Sample {
            name: "rg \"quoted\"".to_string(),
            tags: vec!["a".to_string(), "b".to_string()],
        };
        let text = render(&sample, "sample").unwrap();
        assert_eq!(parse::<Sample>(&text, "sample").unwrap(), sample);
    }

    #[test]
    fn a_string_literal_escapes_quotes_backslashes_and_control_bytes() {
        let text = "say \"hi\" \\ tab\there\u{1b}";
        let literal = string_literal(text);
        assert!(!literal.contains('\u{1b}'), "{literal}");
        let parsed: toml::Table = parse(&format!("v = {literal}"), "literal").unwrap();
        assert_eq!(parsed["v"].as_str(), Some(text));
    }

    #[test]
    fn a_string_list_literal_parses_back_to_the_same_items() {
        let items = vec!["a\"b".to_string(), "c\\d".to_string(), String::new()];
        let literal = string_list_literal(&items);
        let parsed: Sample = parse(&format!("name = \"x\"\ntags = {literal}"), "list").unwrap();
        assert_eq!(parsed.tags, items);
    }

    #[test]
    fn a_key_set_on_a_document_is_deserialized_as_if_the_file_had_it() {
        let mut doc = Document::parse("tags = [\"a\"]\n", "sample").unwrap();
        assert_eq!(doc.str("name"), None);
        doc.set_str("name", "rg");
        assert_eq!(doc.str("name"), Some("rg"));
        let sample: Sample = doc.deserialize().unwrap();
        assert_eq!(sample.name, "rg");
        assert_eq!(sample.tags, vec!["a".to_string()]);
    }

    #[test]
    fn a_document_as_json_keeps_its_nesting_and_types() {
        let doc = Document::parse(
            "name = \"rg\"\njobs = 4\n[asset]\ninclude = [\"*.tar.gz\"]\n",
            "sample",
        )
        .unwrap();
        assert_eq!(
            doc.into_json().unwrap(),
            serde_json::json!({"name": "rg", "jobs": 4, "asset": {"include": ["*.tar.gz"]}})
        );
    }

    #[test]
    fn a_key_that_is_not_a_string_reads_as_absent() {
        let doc = Document::parse("name = 1\n", "sample").unwrap();
        assert_eq!(doc.str("name"), None);
    }

    #[test]
    fn a_document_that_does_not_fit_the_type_names_the_file() {
        let doc = Document::parse("name = \"rg\"\n", "/r/rg/ketch.toml").unwrap();
        let err = doc.deserialize::<Sample>().unwrap_err();
        assert!(err.to_string().contains("/r/rg/ketch.toml"), "{err}");
    }

    #[test]
    fn the_scan_finds_paths_and_imports_but_not_file_names_or_comments() {
        for line in [
            "    let v: toml::Value = toml::from_str(t)?;",
            "use toml_edit::DocumentMut;",
            "use toml;",
            "    let doc = ::toml_edit::DocumentMut::new();",
        ] {
            assert!(names_toml_crate(line), "{line}");
        }
        for line in [
            "    let path = root.join(\"ketch.toml\");",
            "// toml::from_str would drop the comments",
            "    /// Built by rendering a `toml::Value`.",
            "    let tomlish = 1;",
            "    crate::toml_file::parse(text, what)",
        ] {
            assert!(!names_toml_crate(line), "{line}");
        }
    }

    /// Modules that still call `toml` themselves, each until its M16 subtask
    /// moves the calls here. Waiting for the creator's choice of how much of
    /// each moves; whoever does it removes the entry, which this test then
    /// insists on.
    const NOT_YET_MOVED: [&str; 2] = [
        // M16.6: user manifests, `builtin.toml` and the `toml_edit` edit.
        "crates/ketch-core/src/manifest.rs",
        // M16.7: `ketch.lock`.
        "crates/ketch-core/src/lockfile.rs",
    ];

    #[test]
    fn no_module_but_this_one_names_the_toml_crates() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut dirs = vec![root.join("src")];
        for entry in std::fs::read_dir(root.join("crates")).expect("read crates/") {
            dirs.push(entry.expect("crate dir").path().join("src"));
        }
        let mut offenders = Vec::new();
        let mut still_using = Vec::new();
        for dir in dirs {
            for entry in walkdir::WalkDir::new(&dir) {
                let entry = entry.expect("walk sources");
                if entry.path().extension().is_none_or(|e| e != "rs") {
                    continue;
                }
                // Forward slashes, so the names match on Windows too.
                let relative = entry
                    .path()
                    .strip_prefix(&root)
                    .expect("under the repository")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if relative == "crates/ketch-core/src/toml_file.rs" {
                    continue;
                }
                let text = std::fs::read_to_string(entry.path()).expect("read source");
                if !text.lines().any(names_toml_crate) {
                    continue;
                }
                if NOT_YET_MOVED.contains(&relative.as_str()) {
                    still_using.push(relative);
                } else {
                    offenders.push(relative);
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "{offenders:?} name `toml` or `toml_edit`; go through crate::toml_file instead"
        );
        still_using.sort();
        let mut expected = NOT_YET_MOVED.map(str::to_string).to_vec();
        expected.sort();
        assert_eq!(
            still_using, expected,
            "a file in NOT_YET_MOVED no longer names `toml`; take it off the list"
        );
    }
}
