// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! `state.json` bytes through `State::load_path`, the loader every command runs.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| ketch::fuzzing::state(bytes));
