// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! What the core asks a person: decisions the pipeline cannot infer.
//!
//! The pipeline never reads a terminal. Where inference runs out — which of
//! several binaries sharing a package's name to link, whether to stop a process
//! that holds a file being replaced — it asks the [`Decider`] its caller put in
//! [`crate::report::Ctx`]. The `ketch` binary answers with a prompt on the
//! terminal; a graphical front end answers with a sheet or a dialog; a test
//! answers from a script. The default, [`NoDecider`], answers nothing, which is
//! what an unattended run, `--yes` and a lockfile sync want.
//!
//! Confirmations a command makes *before* calling into the core (`remove`,
//! `config reset`) are not here: they are the front end's own dialogs, made
//! with whatever it has.

use crate::process::Occupant;

/// Answers the questions the pipeline asks mid-run. Implementors must be safe
/// to call from any thread; an answer may block on a person.
pub trait Decider: Send + Sync {
    /// Pick one of `candidates` — the files of `package` that share its name —
    /// to link: the index of the pick, or `None` when nobody can answer, which
    /// leaves the decision to the fixed rules and their ambiguity error.
    fn choose_binary(&self, package: &str, candidates: &[String]) -> Option<usize>;

    /// Whether to stop `occupants`, processes that hold files about to be
    /// replaced. A decline leaves them running.
    fn stop_processes(&self, occupants: &[Occupant]) -> bool;
}

/// Answers nothing and declines everything, as a script would.
pub struct NoDecider;

impl Decider for NoDecider {
    fn choose_binary(&self, _package: &str, _candidates: &[String]) -> Option<usize> {
        None
    }

    fn stop_processes(&self, _occupants: &[Occupant]) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_decider_answers_nothing_and_declines() {
        let candidates = ["rtok-cli".to_string(), "rtok-hook".to_string()];
        assert_eq!(NoDecider.choose_binary("rtok", &candidates), None);
        assert!(!NoDecider.stop_processes(&[]));
    }
}
