// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! ketch-capi — `ketch-ffi` behind a C ABI, for front ends that no UniFFI
//! generator reaches: the Vala app on Linux, through `vapi/ketch.vapi`.
//!
//! The shape is what the Swift binding has, carried in C terms. One opaque
//! `KetchCore` handle per root and one function per operation; a reporter and
//! a decider passed per call, each a function pointer with its own
//! `user_data`, which is how a Vala delegate with a target crosses; an opaque
//! `KetchCancel` another thread can trip.
//!
//! Records cross as JSON rather than C structs: about two dozen functions and
//! no struct layout to keep in step on both sides. Every call answers with one
//! string, an envelope that is either `{"ok": <value>}` or
//! `{"error": {"kind": …, "message": …}}`; `schema/payloads.schema.json`
//! describes every value. The string is `malloc`ed: free it with
//! `ketch_string_free`, or with `g_free`, which is `free`.
//!
//! # Safety
//!
//! Every exported function is `unsafe` because it trusts the caller with:
//!
//! - **Strings**: each `const char *` is NULL where documented as optional,
//!   else a NUL-terminated string valid for the call. Bytes that are not UTF-8
//!   are an error, not undefined behaviour.
//! - **String arrays**: `names` points at `names_len` such strings, or is NULL
//!   with `names_len` 0.
//! - **Handles**: a `KetchCore *` or `KetchCancel *` came from its `_new`
//!   function, is not yet freed, and is not freed while a call uses it. Any
//!   number of threads may use one handle at once.
//! - **Callbacks**: a function pointer may be called from any thread, any
//!   number of times, until the call it was passed to returns, and never after.
//!   `user_data` is handed back untouched and must be usable from those
//!   threads. A callback must not unwind into Rust.
//!
//! A panic inside the core never crosses the boundary: it becomes an
//! `other` error.
//!
//! All the unsafe code lives in [`abi`]; the rest of the crate is safe Rust
//! and keeps `unsafe_code` denied.

pub mod abi;

use ketch_ffi::KetchError;
use serde::Serialize;
use std::panic::{catch_unwind, AssertUnwindSafe};

/// What every call answers: its value, or why there is none.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Envelope<T> {
    Ok(T),
    Error(ErrorBody),
}

/// An error, with the sentence a person can be shown next to its kind.
#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
struct ErrorBody {
    #[serde(flatten)]
    error: KetchError,
    message: String,
}

/// Run one call and render its answer as the JSON envelope.
///
/// The panic guard is here, not per function, so no function can forget it:
/// unwinding out of an `extern "C"` function aborts the host app.
fn respond<T: Serialize>(call: impl FnOnce() -> Result<T, KetchError>) -> String {
    let result = catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        Err(KetchError::Other {
            message: "ketch-capi: the call panicked".into(),
        })
    });
    let envelope = match result {
        Ok(value) => Envelope::Ok(value),
        Err(error) => Envelope::Error(ErrorBody {
            message: error.to_string(),
            error,
        }),
    };
    // Every payload is plain data with string keys, which serde_json always
    // renders; the fallback only keeps a broken invariant from being silent.
    serde_json::to_string(&envelope).unwrap_or_else(|e| {
        let message = serde_json::to_string(&e.to_string()).unwrap_or_else(|_| "\"\"".into());
        format!(r#"{{"error":{{"kind":"other","message":{message}}}}}"#)
    })
}

/// An argument the caller got wrong, as the error the envelope carries.
fn invalid(what: &str) -> KetchError {
    KetchError::Other {
        message: format!("ketch-capi: {what}"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::{json, Value};
    use std::path::PathBuf;

    fn crate_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Compare `rendered` with the committed `relative`, or rewrite it under
    /// `KETCH_BLESS`, as the config schemas' drift tests do.
    fn check_drift(relative: &str, rendered: &str) {
        let path = crate_dir().join(relative);
        if std::env::var_os("KETCH_BLESS").is_some() {
            std::fs::write(&path, rendered).unwrap();
            return;
        }
        // A Windows checkout may have turned LF into CRLF; the file is the same.
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            committed == rendered,
            "{relative} is stale: run `KETCH_BLESS=1 cargo nextest run -p ketch-capi`"
        );
    }

    #[test]
    fn a_value_is_wrapped_in_ok() {
        let rendered = respond(|| Ok(vec!["rg"]));
        assert_eq!(
            serde_json::from_str::<Value>(&rendered).unwrap(),
            json!({"ok": ["rg"]})
        );
    }

    #[test]
    fn an_error_carries_its_kind_its_fields_and_a_sentence() {
        let rendered = respond::<()>(|| Err(KetchError::NotFound { name: "jq".into() }));
        assert_eq!(
            serde_json::from_str::<Value>(&rendered).unwrap(),
            json!({"error": {"kind": "not_found", "name": "jq", "message": "`jq` not found"}})
        );
    }

    #[test]
    fn a_panic_becomes_an_other_error() {
        let rendered = respond::<()>(|| panic!("boom"));
        let value: Value = serde_json::from_str(&rendered).unwrap();
        assert_eq!(value["error"]["kind"], "other");
    }

    #[test]
    fn the_committed_header_is_what_cbindgen_generates() {
        let config = cbindgen::Config::from_file(crate_dir().join("cbindgen.toml")).unwrap();
        let mut rendered = Vec::new();
        cbindgen::Builder::new()
            .with_config(config)
            .with_src(crate_dir().join("src/abi.rs"))
            .generate()
            .unwrap()
            .write(&mut rendered);
        check_drift("include/ketch.h", &String::from_utf8(rendered).unwrap());
    }

    #[test]
    fn the_committed_payload_schema_is_what_the_types_generate() {
        check_drift("schema/payloads.schema.json", &payload_schema());
    }

    /// Every value an envelope can carry, under `$defs`, with the envelope
    /// itself as the root.
    pub(crate) fn payload_schema() -> String {
        use ketch_ffi::*;
        let mut generator = schemars::generate::SchemaSettings::draft2020_12().into_generator();
        macro_rules! define {
            ($($t:ty),* $(,)?) => { $( generator.subschema_for::<$t>(); )* };
        }
        define!(
            ErrorBody,
            Event,
            Holder,
            Package,
            Installed,
            InstallOptions,
            SearchResults,
            Upgrade,
            Changelog,
            Check,
            HistoryEvent,
            PackageInfo,
            Pruned,
            PathStatus,
            PathChange,
            Settings,
        );
        let defs = generator.take_definitions(true);
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$comment": "Generated from the Rust types by `KETCH_BLESS=1 cargo nextest run -p ketch-capi`. Do not edit.",
            "title": "ketch-capi payloads",
            "description": "The envelope every ketch-capi call returns. `ok` holds the value named in that function's comment in include/ketch.h; event, choose and stop callbacks receive an `Event`, a JSON array of strings and an array of `Holder`.",
            "oneOf": [
                {"type": "object", "required": ["ok"], "properties": {"ok": true}, "additionalProperties": false},
                {"type": "object", "required": ["error"], "properties": {"error": {"$ref": "#/$defs/ErrorBody"}}, "additionalProperties": false},
            ],
            "$defs": defs,
        });
        serde_json::to_string_pretty(&schema).unwrap() + "\n"
    }
}
