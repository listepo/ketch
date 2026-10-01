//! Somebody else's text through `ui::printable`, the filter in front of the terminal.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| ketch::fuzzing::printable(text));
