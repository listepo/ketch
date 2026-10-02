// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Cooperative cancellation for core operations.
//!
//! A host that outlives one command (a GUI calling the core on a worker thread)
//! needs a way to stop an install it started. The token is polled, never
//! signalled: the pipeline checks it between packages and between download
//! chunks, so a cancelled run unwinds through the same `Drop` cleanup as any
//! other error and leaves no temp directory, partial store folder or state entry.

use crate::error::{Error, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shared flag. Clones observe the same cancellation, so the host keeps one
/// and hands another to the operation.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A token that is not cancelled. The CLI passes one that never is.
    pub fn new() -> Self {
        Self::default()
    }

    // No caller in the CLI, which has no signal wiring; the host app is the one
    // that cancels.
    #[allow(dead_code)]
    /// Ask the operation holding a clone of this token to stop at its next check.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// `Err(Error::Cancelled)` once cancelled, for use with `?` at a check point.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_token_is_seen_by_its_clones() {
        let token = Cancel::new();
        let other = token.clone();
        assert!(other.check().is_ok());
        token.cancel();
        assert!(other.is_cancelled());
        assert!(matches!(other.check(), Err(Error::Cancelled)));
    }
}
