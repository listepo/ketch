//! `ketch.lock` text through the parse and every check `ketch sync` relies on.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| ketch::fuzzing::lockfile(text));
