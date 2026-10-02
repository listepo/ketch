// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! What the core says while it works: typed events a front end renders.
//!
//! The pipeline does not print. It describes what is happening — a package
//! reaching a stage, a status line, a warning, a download advancing — as an
//! [`Event`] and hands it to the [`Reporter`] its caller supplied. The `ketch`
//! binary renders those events as today's terminal lines and bars; a full-screen
//! or graphical front end maps them onto its own widgets; a test records them.
//! None of them needs the core to know which one is listening.
//!
//! Events carry structured fields, not rendered lines: colour, alignment,
//! verbosity and whether a bar is drawn at all are the renderer's decisions.
//! A `detail` is still prose, and much of it names a client app's asset or
//! package — a renderer that shows it on a terminal filters it first
//! ([`crate::changelog::sanitize`]).
//!
//! [`Report`] is the handle the core passes around: a cheap clone of one
//! shared reporter, with the helpers that build each event. Entry points take
//! a [`Ctx`] instead of a bare [`Config`], so the next thing every operation
//! needs (a cancellation token, say) is one more field, not one more argument
//! on every signature.

use crate::config::Config;
use crate::decide::{Decider, NoDecider};
use crate::log;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A stage of the install pipeline, for a renderer that shows one row per
/// package. Hooks run inside [`Stage::Installing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Looking up the manifest and release.
    Resolving,
    /// Copying the release asset locally.
    Downloading,
    /// Checking the asset digest.
    Verifying,
    /// Expanding the release archive.
    Extracting,
    /// Checking platform-specific trust requirements.
    Trusting,
    /// Putting files and links in their final locations.
    Installing,
}

/// Names one piece of long-running work from [`Event::Began`] to its end.
///
/// Unique for the life of the process, so events from concurrent work — a
/// batch runs several downloads at once — never mix up whose bar is whose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(u64);

impl TaskId {
    /// The number behind the id, for a front end that keys its own rows by
    /// it or hands it across a language boundary.
    pub fn get(self) -> u64 {
        self.0
    }

    fn next() -> TaskId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        TaskId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// What kind of long-running work a [`TaskId`] names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    /// Several downloads running side by side. Downloads that name it share
    /// the terminal; the batch ends once every one of them has.
    Batch,
    /// Bytes arriving. `label` is what the caller calls the work (a package,
    /// `registry`, `download`); the file name arrives with [`Event::Sized`].
    Download {
        label: String,
        batch: Option<TaskId>,
    },
    /// Work of unknown length, such as resolving or unpacking.
    Activity { message: String },
    /// Work measured in things rather than bytes: `checking 3/12 packages`.
    Counter {
        verb: String,
        unit: String,
        total: u64,
    },
}

/// One thing the core reports. Renderers match on it; nothing here is a line
/// of output yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A package reached a stage of the install pipeline.
    Step { package: String, stage: Stage },
    /// Work happening now: `resolving ripgrep (github:BurntSushi/ripgrep)`.
    Status { verb: String, detail: String },
    /// Work that finished well.
    Success { verb: String, detail: String },
    /// Something the user should know that does not stop the run.
    Warn { detail: String },
    /// An aside: true, worth saying once, and not a problem.
    Note { detail: String },
    /// Detail for `--verbose` and the log.
    Debug { detail: String },
    /// Long-running work started.
    Began { id: TaskId, task: Task },
    /// A download learned its file name and, when the server said, its size.
    Sized {
        id: TaskId,
        name: String,
        total: Option<u64>,
    },
    /// Work advanced to `done` units (bytes for a download, items for a
    /// counter) of `total` when that is known.
    Progress {
        id: TaskId,
        done: u64,
        total: Option<u64>,
    },
    /// Work finished. A download's `message` names what arrived
    /// (`rg.tar.gz (1.2 MiB)`); `None` when there is nothing to say.
    Ended { id: TaskId, message: Option<String> },
    /// Work stopped without finishing — an error unwound past it. A renderer
    /// takes down whatever it drew for it without announcing anything.
    Abandoned { id: TaskId },
}

/// Receives the core's events. Implementors must be cheap to call and safe to
/// call from any thread: a batch reports from several at once.
pub trait Reporter: Send + Sync {
    /// Handle one event.
    fn event(&self, event: Event);
}

/// Discards every event.
pub struct Silent;

impl Reporter for Silent {
    fn event(&self, _event: Event) {}
}

/// Writes events to the log file, then passes them on.
///
/// The `ketch` binary does not need it: its renderer logs every line it
/// prints. A front end that renders events itself wraps its reporter in this
/// so its operations land in the same log, worded the same way.
pub struct LogReporter {
    inner: Option<Report>,
}

impl LogReporter {
    /// Log, then hand each event to `inner` when there is one.
    pub fn new(inner: Option<Report>) -> Self {
        LogReporter { inner }
    }
}

impl Reporter for LogReporter {
    fn event(&self, event: Event) {
        match &event {
            Event::Status { verb, detail } | Event::Success { verb, detail } => {
                log::record(log::Level::Info, &format!("{verb} {detail}"))
            }
            Event::Warn { detail } => log::record(log::Level::Warn, detail),
            Event::Note { detail } => log::record(log::Level::Info, detail),
            Event::Debug { detail } => log::record(log::Level::Debug, detail),
            Event::Ended {
                message: Some(message),
                ..
            } => log::record(log::Level::Info, &format!("fetched {message}")),
            _ => {}
        }
        if let Some(inner) = &self.inner {
            inner.event(event);
        }
    }
}

/// Keeps every event, for a test or a host that inspects them afterwards.
#[derive(Default)]
pub struct Recorder {
    events: Mutex<Vec<Event>>,
}

impl Recorder {
    /// Everything received so far, in order.
    pub fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl Reporter for Recorder {
    fn event(&self, event: Event) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event);
    }
}

/// The reporter the core passes around: one shared [`Reporter`] and the
/// helpers that build its events.
#[derive(Clone)]
pub struct Report(Arc<dyn Reporter>);

impl Report {
    /// Report to `reporter`.
    pub fn new(reporter: impl Reporter + 'static) -> Self {
        Report(Arc::new(reporter))
    }

    /// Report to a reporter the caller keeps a handle to as well.
    pub fn shared(reporter: Arc<dyn Reporter>) -> Self {
        Report(reporter)
    }

    /// Report to nobody.
    pub fn silent() -> Self {
        Report::new(Silent)
    }

    /// Hand one event to the reporter.
    pub fn event(&self, event: Event) {
        self.0.event(event);
    }

    /// `package` reached `stage`.
    pub fn stage(&self, package: &str, stage: Stage) {
        self.event(Event::Step {
            package: package.to_string(),
            stage,
        });
    }

    /// See [`Event::Status`].
    pub fn step(&self, verb: &str, detail: &str) {
        self.event(Event::Status {
            verb: verb.to_string(),
            detail: detail.to_string(),
        });
    }

    /// See [`Event::Success`].
    pub fn success(&self, verb: &str, detail: &str) {
        self.event(Event::Success {
            verb: verb.to_string(),
            detail: detail.to_string(),
        });
    }

    /// See [`Event::Warn`].
    pub fn warn(&self, detail: &str) {
        self.event(Event::Warn {
            detail: detail.to_string(),
        });
    }

    /// See [`Event::Note`].
    pub fn note(&self, detail: &str) {
        self.event(Event::Note {
            detail: detail.to_string(),
        });
    }

    /// See [`Event::Debug`].
    pub fn debug(&self, detail: &str) {
        self.event(Event::Debug {
            detail: detail.to_string(),
        });
    }

    /// Start work of unknown length; it ends when the value is dropped.
    #[must_use = "the activity ends when dropped; hold it or call run"]
    pub fn activity(&self, message: &str) -> Activity {
        Activity {
            done: Handle::begin(
                self,
                Task::Activity {
                    message: message.to_string(),
                },
            ),
        }
    }

    /// Start a batch of concurrent downloads; it ends when the value is
    /// dropped.
    #[must_use = "the batch ends when dropped"]
    pub fn batch(&self) -> Batch {
        Batch {
            done: Handle::begin(self, Task::Batch),
        }
    }

    /// A download sink for work the caller calls `label`.
    pub fn download(&self, label: &str) -> Download {
        Download::begin(self, label, None)
    }

    /// Count `total` things done one at a time.
    #[must_use = "the counter ends when dropped"]
    pub fn counter(&self, verb: &str, total: u64, unit: &str) -> Counter {
        Counter {
            done: Handle::begin(
                self,
                Task::Counter {
                    verb: verb.to_string(),
                    unit: unit.to_string(),
                    total,
                },
            ),
            count: AtomicU64::new(0),
            total,
        }
    }
}

/// What a core operation runs with: the configuration, where to report and
/// who answers its questions.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub report: &'a Report,
    /// Asked what inference cannot decide. [`NoDecider`] unless a front end
    /// with a person in front of it says otherwise.
    pub decider: &'a dyn Decider,
}

impl<'a> Ctx<'a> {
    /// Pair a configuration with a reporter; nobody answers questions.
    pub fn new(cfg: &'a Config, report: &'a Report) -> Self {
        Ctx {
            cfg,
            report,
            decider: &NoDecider,
        }
    }

    /// The same context, with `decider` answering the pipeline's questions.
    pub fn with_decider(self, decider: &'a dyn Decider) -> Self {
        Ctx { decider, ..self }
    }
}

/// A begun task that sends [`Event::Ended`] once, when dropped.
struct Handle {
    report: Report,
    id: TaskId,
}

impl Handle {
    fn begin(report: &Report, task: Task) -> Handle {
        let id = TaskId::next();
        report.event(Event::Began { id, task });
        Handle {
            report: report.clone(),
            id,
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.report.event(Event::Ended {
            id: self.id,
            message: None,
        });
    }
}

/// Work of unknown length, from [`Report::activity`].
pub struct Activity {
    done: Handle,
}

impl Activity {
    /// Run `work`, then end the activity before returning its value.
    pub fn run<T>(self, work: impl FnOnce() -> T) -> T {
        let out = work();
        drop(self);
        out
    }

    /// This activity's id, as its events carry it.
    pub fn id(&self) -> TaskId {
        self.done.id
    }
}

/// Concurrent downloads, from [`Report::batch`].
pub struct Batch {
    done: Handle,
}

impl Batch {
    /// A download sink that belongs to this batch.
    pub fn download(&self, label: &str) -> Download {
        Download::begin(&self.done.report, label, Some(self.done.id))
    }
}

/// Things counted one at a time, from [`Report::counter`].
pub struct Counter {
    done: Handle,
    count: AtomicU64,
    total: u64,
}

impl Counter {
    /// One more item done.
    pub fn inc(&self) {
        let done = self.count.fetch_add(1, Ordering::Relaxed) + 1;
        self.done.report.event(Event::Progress {
            id: self.done.id,
            done,
            total: Some(self.total),
        });
    }
}

/// How a download reports back. Implementors must be cheap to call and safe
/// to call from any thread.
pub trait ProgressSink: Send + Sync {
    /// The transfer began: `label` is the file, `total` its size when known.
    fn start(&self, total: Option<u64>, label: &str);
    /// `delta` more bytes arrived.
    fn advance(&self, delta: u64);
    /// The transfer finished; `message` names what arrived, or is empty.
    fn finish(&self, message: &str);
}

/// Discards everything. For downloads nobody watches, such as a signature
/// sidecar fetched on the way to verifying an asset.
pub struct SilentProgress;

impl ProgressSink for SilentProgress {
    fn start(&self, _total: Option<u64>, _label: &str) {}
    fn advance(&self, _delta: u64) {}
    fn finish(&self, _message: &str) {}
}

/// A download reported as events, from [`Report::download`] or
/// [`Batch::download`].
pub struct Download {
    report: Report,
    id: TaskId,
    done: AtomicU64,
    total: Mutex<Option<u64>>,
    finished: AtomicBool,
}

impl Download {
    fn begin(report: &Report, label: &str, batch: Option<TaskId>) -> Download {
        let id = TaskId::next();
        report.event(Event::Began {
            id,
            task: Task::Download {
                label: label.to_string(),
                batch,
            },
        });
        Download {
            report: report.clone(),
            id,
            done: AtomicU64::new(0),
            total: Mutex::new(None),
            finished: AtomicBool::new(false),
        }
    }

    fn total(&self) -> Option<u64> {
        *self.total.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ProgressSink for Download {
    fn start(&self, total: Option<u64>, label: &str) {
        *self.total.lock().unwrap_or_else(|e| e.into_inner()) = total;
        self.done.store(0, Ordering::Relaxed);
        self.report.event(Event::Sized {
            id: self.id,
            name: label.to_string(),
            total,
        });
    }

    fn advance(&self, delta: u64) {
        let done = self.done.fetch_add(delta, Ordering::Relaxed) + delta;
        self.report.event(Event::Progress {
            id: self.id,
            done,
            total: self.total(),
        });
    }

    fn finish(&self, message: &str) {
        self.finished.store(true, Ordering::Relaxed);
        self.report.event(Event::Ended {
            id: self.id,
            message: (!message.is_empty()).then(|| message.to_string()),
        });
    }
}

impl Drop for Download {
    fn drop(&mut self) {
        if !self.finished.load(Ordering::Relaxed) {
            self.report.event(Event::Abandoned { id: self.id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn recording() -> (Arc<Recorder>, Report) {
        let recorder = Arc::new(Recorder::default());
        let report = Report::shared(recorder.clone());
        (recorder, report)
    }

    #[test]
    fn an_activity_ends_when_its_work_returns() {
        let (recorder, report) = recording();
        let value = report.activity("resolving rg").run(|| 7);
        assert_eq!(value, 7);
        let events = recorder.events();
        let Event::Began { id, .. } = &events[0] else {
            panic!("{events:?}");
        };
        assert_eq!(
            events,
            vec![
                Event::Began {
                    id: *id,
                    task: Task::Activity {
                        message: "resolving rg".into()
                    }
                },
                Event::Ended {
                    id: *id,
                    message: None
                },
            ]
        );
    }

    #[test]
    fn a_download_reports_its_running_total_and_what_arrived() {
        let (recorder, report) = recording();
        let sink = report.download("rg");
        sink.start(Some(10), "rg.tar.gz");
        sink.advance(4);
        sink.advance(6);
        sink.finish("rg.tar.gz (10 B)");
        drop(sink);
        let events = recorder.events();
        let Event::Began { id, .. } = &events[0] else {
            panic!("{events:?}");
        };
        let id = *id;
        assert_eq!(
            events,
            vec![
                Event::Began {
                    id,
                    task: Task::Download {
                        label: "rg".into(),
                        batch: None
                    }
                },
                Event::Sized {
                    id,
                    name: "rg.tar.gz".into(),
                    total: Some(10)
                },
                Event::Progress {
                    id,
                    done: 4,
                    total: Some(10)
                },
                Event::Progress {
                    id,
                    done: 10,
                    total: Some(10)
                },
                Event::Ended {
                    id,
                    message: Some("rg.tar.gz (10 B)".into())
                },
            ]
        );
    }

    #[test]
    fn a_download_dropped_unfinished_is_abandoned_not_ended() {
        let (recorder, report) = recording();
        drop(report.download("rg"));
        let events = recorder.events();
        assert!(matches!(events.last(), Some(Event::Abandoned { .. })));
        assert!(!events.iter().any(|e| matches!(e, Event::Ended { .. })));
    }

    #[test]
    fn a_batch_download_names_its_batch() {
        let (recorder, report) = recording();
        let batch = report.batch();
        let sink = batch.download("fd");
        sink.finish("");
        drop(sink);
        drop(batch);
        let events = recorder.events();
        let Event::Began { id: batch_id, .. } = &events[0] else {
            panic!("{events:?}");
        };
        assert!(matches!(
            &events[1],
            Event::Began { task: Task::Download { batch: Some(b), .. }, .. } if b == batch_id
        ));
        assert!(matches!(&events[2], Event::Ended { message: None, .. }));
        assert_eq!(
            events[3],
            Event::Ended {
                id: *batch_id,
                message: None
            }
        );
    }

    #[test]
    fn a_counter_counts_up_to_its_total() {
        let (recorder, report) = recording();
        let counter = report.counter("checking", 2, "packages");
        counter.inc();
        counter.inc();
        drop(counter);
        let done: Vec<u64> = recorder
            .events()
            .iter()
            .filter_map(|e| match e {
                Event::Progress {
                    done,
                    total: Some(2),
                    ..
                } => Some(*done),
                _ => None,
            })
            .collect();
        assert_eq!(done, vec![1, 2]);
    }

    #[test]
    fn a_log_reporter_passes_every_event_on() {
        let (recorder, inner) = recording();
        let report = Report::new(LogReporter::new(Some(inner)));
        report.warn("careful");
        report.step("resolving", "rg");
        assert_eq!(
            recorder.events(),
            vec![
                Event::Warn {
                    detail: "careful".into()
                },
                Event::Status {
                    verb: "resolving".into(),
                    detail: "rg".into()
                },
            ]
        );
    }
}
