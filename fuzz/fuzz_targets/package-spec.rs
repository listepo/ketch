// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! What a user types as a package: `PackageSpec::parse` and its label.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: &str| ketch::fuzzing::package_spec(input));
