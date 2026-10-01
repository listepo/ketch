//! Every JSON reply a `ketch-source-*` plugin can send.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|body: &str| ketch::fuzzing::plugin_protocol(body));
