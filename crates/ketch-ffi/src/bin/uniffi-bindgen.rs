// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! `uniffi-bindgen`, built from this workspace so the generator always matches
//! the `uniffi` version the library was built with; a separately installed
//! one that drifts by a release writes bindings the library rejects at load.

fn main() {
    uniffi::uniffi_bindgen_main()
}
