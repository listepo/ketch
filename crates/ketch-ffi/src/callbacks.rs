// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! What the foreign side implements and holds: the reporter that receives the
//! core's events, the decider that answers its questions, and the token that
//! cancels an operation.
//!
//! Each is a thin adapter onto the core trait or type R6, R7 and R8 introduced
//! (`report::Reporter`, `decide::Decider`, `cancel::Cancel`), so the core
//! knows nothing about bindings and the CLI's implementations stay as they are.

use ketch_core::cancel::Cancel;
use ketch_core::decide;
use ketch_core::process::Occupant;
use ketch_core::report::{self, Stage as CoreStage, Task as CoreTask};
use std::sync::Arc;

/// A stage of the install pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Stage {
    Resolving,
    Downloading,
    Verifying,
    Extracting,
    Trusting,
    Installing,
}

impl From<CoreStage> for Stage {
    fn from(stage: CoreStage) -> Self {
        match stage {
            CoreStage::Resolving => Stage::Resolving,
            CoreStage::Downloading => Stage::Downloading,
            CoreStage::Verifying => Stage::Verifying,
            CoreStage::Extracting => Stage::Extracting,
            CoreStage::Trusting => Stage::Trusting,
            CoreStage::Installing => Stage::Installing,
        }
    }
}

/// What kind of long-running work a task id names. Not `Task`: that is a type
/// every Swift file already has in scope.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TaskKind {
    /// Several downloads running side by side.
    Batch,
    /// Bytes arriving for `label` (a package name, `registry`, `download`).
    Download { label: String, batch: Option<u64> },
    /// Work of unknown length.
    Activity { message: String },
    /// Things counted one at a time: `checking 3/12 packages`.
    Counter {
        verb: String,
        unit: String,
        total: u64,
    },
}

impl From<CoreTask> for TaskKind {
    fn from(task: CoreTask) -> Self {
        match task {
            CoreTask::Batch => TaskKind::Batch,
            CoreTask::Download { label, batch } => TaskKind::Download {
                label,
                batch: batch.map(|id| id.get()),
            },
            CoreTask::Activity { message } => TaskKind::Activity { message },
            CoreTask::Counter { verb, unit, total } => TaskKind::Counter { verb, unit, total },
        }
    }
}

/// One thing the core reports, field for field what `report::Event` carries.
/// Task ids are unique for the life of the process.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum Event {
    /// A package reached a stage of the install pipeline.
    Step { package: String, stage: Stage },
    /// Work happening now.
    Status { verb: String, detail: String },
    /// Work that finished well.
    Success { verb: String, detail: String },
    /// Something worth knowing that does not stop the run.
    Warn { detail: String },
    /// An aside.
    Note { detail: String },
    /// Detail for a verbose view and the log.
    Debug { detail: String },
    /// Long-running work started.
    Began { id: u64, task: TaskKind },
    /// A download learned its file name and, when known, its size.
    Sized {
        id: u64,
        name: String,
        total: Option<u64>,
    },
    /// Work advanced to `done` of `total`.
    Progress {
        id: u64,
        done: u64,
        total: Option<u64>,
    },
    /// Work finished.
    Ended { id: u64, message: Option<String> },
    /// Work stopped without finishing; take down whatever shows it.
    Abandoned { id: u64 },
}

impl From<report::Event> for Event {
    fn from(event: report::Event) -> Self {
        use report::Event as E;
        match event {
            E::Step { package, stage } => Event::Step {
                package,
                stage: stage.into(),
            },
            E::Status { verb, detail } => Event::Status { verb, detail },
            E::Success { verb, detail } => Event::Success { verb, detail },
            E::Warn { detail } => Event::Warn { detail },
            E::Note { detail } => Event::Note { detail },
            E::Debug { detail } => Event::Debug { detail },
            E::Began { id, task } => Event::Began {
                id: id.get(),
                task: task.into(),
            },
            E::Sized { id, name, total } => Event::Sized {
                id: id.get(),
                name,
                total,
            },
            E::Progress { id, done, total } => Event::Progress {
                id: id.get(),
                done,
                total,
            },
            E::Ended { id, message } => Event::Ended {
                id: id.get(),
                message,
            },
            E::Abandoned { id } => Event::Abandoned { id: id.get() },
        }
    }
}

/// Receives the core's events. Called from whichever thread the work runs on,
/// several at once during a batch: an implementation hops to its UI thread
/// itself and must not block.
#[uniffi::export(callback_interface)]
pub trait Reporter: Send + Sync {
    fn event(&self, event: Event);
}

/// A process holding a file an upgrade is about to replace.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Holder {
    pub pid: u32,
    /// The file it holds.
    pub path: String,
}

/// Answers the questions the pipeline cannot infer. Called on the operation's
/// worker thread, which waits for the answer: an implementation may block on a
/// dialog.
#[uniffi::export(callback_interface)]
pub trait Decider: Send + Sync {
    /// Pick one of `candidates`, the files of `package` sharing its name: the
    /// index of the pick, or `None` to leave it to the fixed rules (which fail
    /// with an ambiguity error).
    fn choose_binary(&self, package: String, candidates: Vec<String>) -> Option<u32>;

    /// Whether to stop `holders` so their files can be replaced. A decline
    /// leaves them running.
    fn stop_processes(&self, holders: Vec<Holder>) -> bool;
}

/// The foreign reporter, as the core's.
pub(crate) struct ForeignReporter(pub(crate) Arc<dyn Reporter>);

impl report::Reporter for ForeignReporter {
    fn event(&self, event: report::Event) {
        self.0.event(event.into());
    }
}

/// The foreign decider, as the core's.
pub(crate) struct ForeignDecider(pub(crate) Box<dyn Decider>);

impl decide::Decider for ForeignDecider {
    fn choose_binary(&self, package: &str, candidates: &[String]) -> Option<usize> {
        let pick = self
            .0
            .choose_binary(package.to_string(), candidates.to_vec())?;
        // An index the foreign side made up is no answer at all, rather than a
        // panic or a different binary than the one it meant.
        usize::try_from(pick).ok().filter(|i| *i < candidates.len())
    }

    fn stop_processes(&self, occupants: &[Occupant]) -> bool {
        let holders = occupants
            .iter()
            .map(|o| Holder {
                pid: o.pid,
                path: o.path.display().to_string(),
            })
            .collect();
        self.0.stop_processes(holders)
    }
}

/// Cancels the operation it is passed to. Keep it, pass it, call `cancel`
/// from any thread; the operation returns `KetchError::Cancelled` at its next
/// check and leaves no partial install behind. One token per operation: once
/// cancelled it stays cancelled.
#[derive(Debug, Default, uniffi::Object)]
pub struct CancelToken(pub(crate) Cancel);

#[uniffi::export]
impl CancelToken {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(CancelToken::default())
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ketch_core::decide::Decider as _;
    use ketch_core::report::Reporter as _;
    use pretty_assertions::assert_eq;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorded(Mutex<Vec<Event>>);

    impl Reporter for Recorded {
        fn event(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn core_events_reach_the_foreign_reporter_with_their_ids() {
        let recorded = Arc::new(Recorded::default());
        let core = report::Report::shared(Arc::new(ForeignReporter(recorded.clone())));
        core.warn("careful");
        let activity = core.activity("unpacking");
        let id = activity.id().get();
        drop(activity);

        assert_eq!(
            *recorded.0.lock().unwrap(),
            vec![
                Event::Warn {
                    detail: "careful".into()
                },
                Event::Began {
                    id,
                    task: TaskKind::Activity {
                        message: "unpacking".into()
                    }
                },
                Event::Ended { id, message: None },
            ]
        );
    }

    #[test]
    fn steps_and_progress_cross_with_their_fields() {
        let recorded = Arc::new(Recorded::default());
        let core = report::Report::shared(Arc::new(ForeignReporter(recorded.clone())));
        core.event(report::Event::Step {
            package: "rg".into(),
            stage: CoreStage::Verifying,
        });
        ForeignReporter(recorded.clone()).event(report::Event::Progress {
            id: core.activity("x").id(),
            done: 3,
            total: Some(9),
        });
        let events = recorded.0.lock().unwrap().clone();
        assert_eq!(
            events[0],
            Event::Step {
                package: "rg".into(),
                stage: Stage::Verifying
            }
        );
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Progress {
                done: 3,
                total: Some(9),
                ..
            }
        )));
    }

    struct Scripted(Option<u32>, bool);

    impl Decider for Scripted {
        fn choose_binary(&self, _package: String, _candidates: Vec<String>) -> Option<u32> {
            self.0
        }
        fn stop_processes(&self, holders: Vec<Holder>) -> bool {
            self.1 && holders.iter().all(|h| h.path == "/k/bin/rg")
        }
    }

    #[test]
    fn the_decider_picks_by_index_and_rejects_one_out_of_range() {
        let candidates = ["rg-a".to_string(), "rg-b".to_string()];
        let second = ForeignDecider(Box::new(Scripted(Some(1), false)));
        assert_eq!(second.choose_binary("rg", &candidates), Some(1));
        let beyond = ForeignDecider(Box::new(Scripted(Some(2), false)));
        assert_eq!(beyond.choose_binary("rg", &candidates), None);
        let none = ForeignDecider(Box::new(Scripted(None, false)));
        assert_eq!(none.choose_binary("rg", &candidates), None);
    }

    #[test]
    fn the_decider_sees_the_holders_it_is_asked_about() {
        let occupants = [Occupant {
            pid: 7,
            path: "/k/bin/rg".into(),
        }];
        assert!(ForeignDecider(Box::new(Scripted(None, true))).stop_processes(&occupants));
        assert!(!ForeignDecider(Box::new(Scripted(None, false))).stop_processes(&occupants));
    }

    #[test]
    fn a_cancel_token_is_seen_by_the_core_clone() {
        let token = CancelToken::new();
        let core = token.0.clone();
        assert!(core.check().is_ok());
        token.cancel();
        assert!(token.is_cancelled());
        assert!(core.check().is_err());
    }
}
