//! Command implementations.
//!
//! These are thin: argument handling, output, and confirmations. Anything that
//! touches the install tree belongs in `install.rs`, `state.rs` or a trait
//! implementation, so the same logic serves every command.

pub mod lock;
pub mod pkg;
pub mod query;
pub mod system;
