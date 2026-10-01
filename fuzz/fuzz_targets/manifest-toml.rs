//! A registry `ketch.toml`, single manifest or `[[package]]` array, through
//! `parse_registry` and `Manifest::validate`.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| ketch::fuzzing::manifest_toml(text));
