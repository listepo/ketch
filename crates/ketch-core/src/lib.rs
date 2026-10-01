//! ketch-core — everything ketch does besides parsing its command line.
//!
//! Resolving a name to a manifest, fetching and verifying a release, unpacking
//! it into the store, linking it onto `PATH` and recording what happened all
//! live here, so the `ketch` binary is only the clap surface and the thin
//! command bodies that call into this crate. It is a separate crate so a
//! second front end can link the same pipeline without the CLI.
//!
//! `ui` is still here and still prints: moving output behind a reporter the
//! front end supplies is a later step, and this crate is a move only.

pub(crate) mod bin_choice;
pub mod cancel;
pub mod changelog;
pub mod config;
pub mod diff;
pub mod error;
pub mod extra;
pub mod extract;
pub mod hooks;
pub(crate) mod http;
pub mod install;
pub mod listing;
pub mod lockfile;
pub mod log;
pub mod manifest;
pub mod model;
pub mod platform;
pub mod process;
pub mod push;
pub mod registry;
pub mod resolve;
pub mod self_update;
pub mod shell;
pub mod source;
pub mod state;
pub mod stats;
pub(crate) mod trust;
#[cfg(feature = "tui")]
pub mod tui;
pub mod ui;
pub mod wizard;
