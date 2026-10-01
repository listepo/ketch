//! `state.json` bytes through `State::load_path`, the loader every command runs.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| ketch::fuzzing::state(bytes));
