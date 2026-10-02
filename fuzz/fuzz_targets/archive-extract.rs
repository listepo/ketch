// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Arbitrary bytes through the tar.gz, tar.xz, tar.bz2, tar and zip extractors
//! into a scratch directory; nothing may land outside it, and no symlink the
//! archive made may point out of it.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| ketch::fuzzing::archive_extract(bytes));
