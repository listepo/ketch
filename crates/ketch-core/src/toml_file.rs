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
}
