//! What a user types as a package: `PackageSpec::parse` and its label.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| ketch::fuzzing::package_spec(input));
