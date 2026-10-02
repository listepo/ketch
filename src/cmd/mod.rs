// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Command implementations.
//!
//! These are thin: argument handling, output, and confirmations. Anything that
//! touches the install tree belongs in `install.rs`, `state.rs` or a trait
//! implementation, so the same logic serves every command.

pub mod config;
pub mod import;
pub mod lock;
pub mod pkg;
pub mod query;
pub mod registry;
pub mod system;
