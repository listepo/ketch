// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! A manifest hook script on its way to the shell.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|script: &str| ketch::fuzzing::hook_line(script));
