//! A release's `SHA256SUMS` body and a GitHub `sha256:` asset digest.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| ketch::fuzzing::checksum_file(text));
