// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! `ketch.lock` text through the parse and every check `ketch sync` relies on.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| ketch::fuzzing::lockfile(text));
