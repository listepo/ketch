//! One `extra_paths` entry, a bare path or a `{ path, kind, shell, section }`
//! table, through `extra::classify`; a classified path must stay inside the
//! payload.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(
    |entry: (String, Option<(bool, Option<u8>, Option<String>)>)| {
        let (path, spec) = entry;
        ketch::fuzzing::extra_path(path, spec);
    }
);
