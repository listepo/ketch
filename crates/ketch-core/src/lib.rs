// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! ketch-core — everything ketch does besides parsing its command line.
//!
//! Resolving a name to a manifest, fetching and verifying a release, unpacking
//! it into the store, linking it onto `PATH` and recording what happened all
//! live here, so the `ketch` binary is only the clap surface and the thin
//! command bodies that call into this crate. It is a separate crate so a
//! second front end can link the same pipeline without the CLI.
//!
//! It prints nothing and reads nothing. What it has to say — stages, progress,
//! warnings — goes out as `report::Event`s to the `report::Reporter` the front
//! end hands in through `report::Ctx`, and the front end decides how it looks.
//! What it has to ask goes to the `decide::Decider` in the same `Ctx`.

pub(crate) mod bin_choice;
pub mod cancel;
pub mod changelog;
pub mod config;
pub mod decide;
pub mod diff;
pub mod doctor;
pub mod error;
pub mod extra;
pub mod extract;
pub mod hooks;
pub(crate) mod http;
pub mod import;
pub mod install;
pub mod link;
pub mod listing;
pub mod lockfile;
pub mod log;
pub mod manifest;
pub mod model;
pub mod platform;
pub mod process;
pub mod push;
pub mod registry;
pub mod report;
pub mod resolve;
pub mod self_update;
pub mod shell;
pub mod source;
pub mod state;
pub mod stats;
pub mod text;
pub(crate) mod trust;
pub mod wizard;
