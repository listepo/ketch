// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! Every JSON reply a `ketch-source-*` plugin can send.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|body: &str| ketch::fuzzing::plugin_protocol(body));
