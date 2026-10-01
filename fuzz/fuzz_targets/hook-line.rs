//! A manifest hook script on its way to the shell.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|script: &str| ketch::fuzzing::hook_line(script));
