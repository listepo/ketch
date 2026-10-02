// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! A release's `SHA256SUMS` body and a GitHub `sha256:` asset digest.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| ketch::fuzzing::checksum_file(text));
