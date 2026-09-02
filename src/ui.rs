//! Terminal output.
//!
//! Kept dependency-light on purpose: sources and platforms report progress
//! through the `ProgressSink` trait, so nothing below this module needs to know
//! whether a human, a pipe, or a test is watching.

use crate::log;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;

static COLOR: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU8 = AtomicU8::new(1); // 0 quiet, 1 normal, 2 verbose

/// The bars currently sharing the terminal, while a batch is running.
static BARS: Mutex<Option<MultiProgress>> = Mutex::new(None);

/// Every status line leaves through here.
///
/// While progress bars are on screen they own the bottom of the terminal, and
/// a bare `eprintln!` lands in the middle of one. `indicatif` knows how to
/// print above them, so when a batch is running it does the writing.
fn emit(line: &str) {
    match held().as_ref() {
        // `suspend` takes the bars off the screen, lets the line be written
        // normally, and redraws them underneath it. `println` queues the line
        // for the next redraw, and the last line of a batch never gets one.
        Some(bars) => bars.suspend(|| eprintln!("{line}")),
        None => eprintln!("{line}"),
    }
}

fn held() -> std::sync::MutexGuard<'static, Option<MultiProgress>> {
    BARS.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn init(color: Option<bool>, quiet: bool, verbose: bool) {
    let enabled = color.unwrap_or_else(|| {
        std::io::stderr().is_terminal()
            && std::env::var_os("NO_COLOR").is_none()
            && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true)
    });
    COLOR.store(enabled, Ordering::Relaxed);
    LEVEL.store(
        if quiet {
            0
        } else if verbose {
            2
        } else {
            1
        },
        Ordering::Relaxed,
    );
}

pub fn color_enabled() -> bool {
    COLOR.load(Ordering::Relaxed)
}

pub fn is_quiet() -> bool {
    LEVEL.load(Ordering::Relaxed) == 0
}

pub fn is_verbose() -> bool {
    LEVEL.load(Ordering::Relaxed) >= 2
}

fn paint(code: &str, text: &str) -> String {
    if color_enabled() {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn bold(t: &str) -> String {
    paint("1", t)
}
pub fn dim(t: &str) -> String {
    paint("2", t)
}
pub fn green(t: &str) -> String {
    paint("32", t)
}
pub fn yellow(t: &str) -> String {
    paint("33", t)
}
pub fn blue(t: &str) -> String {
    paint("34", t)
}
pub fn red(t: &str) -> String {
    paint("31", t)
}
pub fn cyan(t: &str) -> String {
    paint("36", t)
}

/// Status line for a step that is happening now.
pub fn step(verb: &str, detail: &str) {
    // Logged before the level check: `--quiet` is about this terminal, and the
    // whole point of the log is to still have the run afterwards.
    log::record(log::Level::Info, &format!("{verb} {detail}"));
    if is_quiet() {
        return;
    }
    emit(&format!("{} {}", blue(&format!("{verb:>10}")), detail));
}

/// Something finished well.
pub fn success(verb: &str, detail: &str) {
    log::record(log::Level::Info, &format!("{verb} {detail}"));
    if is_quiet() {
        return;
    }
    emit(&format!("{} {}", green(&format!("{verb:>10}")), detail));
}

/// Something the user should know but that does not stop the run.
pub fn warn(detail: &str) {
    log::record(log::Level::Warn, detail);
    emit(&format!(
        "{} {}",
        yellow(&format!("{:>10}", "warning")),
        detail
    ));
}

/// An aside: true, worth saying once, and not a problem.
pub fn note(detail: &str) {
    log::record(log::Level::Info, detail);
    if is_quiet() {
        return;
    }
    emit(&format!(
        "{} {}",
        dim(&format!("{:>10}", "note")),
        dim(detail)
    ));
}

/// Only shown with `--verbose`, but always written to the log.
pub fn debug(detail: &str) {
    log::record(log::Level::Debug, detail);
    if is_verbose() {
        emit(&format!(
            "{} {}",
            dim(&format!("{:>10}", "debug")),
            dim(detail)
        ));
    }
}

/// Fatal error rendering, including details and a hint when we have one.
pub fn error(err: &crate::error::Error) {
    let details = err.details();
    let hint = err.hint();
    // One record, so a failure is one entry in the log rather than three lines
    // a reader has to piece back together.
    let mut logged = err.to_string();
    for line in &details {
        logged.push('\n');
        logged.push_str(line);
    }
    if let Some(hint) = &hint {
        logged.push_str("\nhint: ");
        logged.push_str(hint);
    }
    log::record(log::Level::Error, &logged);

    emit(&format!("{} {err}", red(&format!("{:>10}", "error"))));
    for line in &details {
        emit(&format!("{} {}", " ".repeat(10), dim(line)));
    }
    if let Some(hint) = &hint {
        emit(&format!("{} {hint}", cyan(&format!("{:>10}", "hint"))));
    }
}

/// Data output. Unlike the status helpers this goes to stdout, so `ketch list`
/// can be piped while progress still shows on the terminal.
pub fn out(line: &str) {
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let _ = writeln!(lock, "{line}");
}

/// Ask a yes/no question. Returns `default` when stdin is not a terminal, so
/// scripts never hang waiting for input that will not come.
pub fn confirm(question: &str, default: bool) -> bool {
    let answered = ask(question, default);
    if !answered {
        // Callers return Ok(()) on a decline, which is indistinguishable from
        // success. Say why nothing happened — especially when the decline came
        // from a non-interactive stdin rather than from a person. Printed even
        // under `--quiet`: "it did nothing and said nothing" is not quiet, it
        // is a bug report waiting to happen.
        eprintln!("{} {question}", blue(&format!("{:>10}", "cancelled")));
    }
    answered
}

/// `--quiet` deliberately does not reach here. It asks for less output, not for
/// consent: silently taking the default answer to "remove this?" is not a
/// quieter version of asking, it is a different program.
fn ask(question: &str, default: bool) -> bool {
    if !std::io::stdin().is_terminal() {
        return default;
    }
    let suffix = if default { "[Y/n]" } else { "[y/N]" };
    eprint!(
        "{} {question} {suffix} ",
        yellow(&format!("{:>10}", "confirm"))
    );
    let _ = std::io::stderr().flush();
    let mut answer = String::new();
    if std::io::stdin().read_line(&mut answer).is_err() {
        return default;
    }
    match answer.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        _ => default,
    }
}

/// Human-readable byte count.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

/// How long-running work reports back. Implementors must be cheap to call and
/// safe to call from any thread.
pub trait ProgressSink: Send + Sync {
    fn start(&self, total: Option<u64>, label: &str);
    fn advance(&self, delta: u64);
    fn finish(&self, message: &str);
}

/// Discards everything. Used by tests, `--quiet`, and non-terminal output.
pub struct SilentProgress;

impl ProgressSink for SilentProgress {
    fn start(&self, _total: Option<u64>, _label: &str) {}
    fn advance(&self, _delta: u64) {}
    fn finish(&self, _message: &str) {}
}

/// A real terminal progress bar.
pub struct BarProgress {
    bar: ProgressBar,
    /// The group this bar belongs to, when it is one of several on screen.
    group: Option<MultiProgress>,
    /// What to call the work, when the caller knows better than the download
    /// does. Four bars all labelled by asset file name say very little.
    label: Option<String>,
}

impl Default for BarProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl BarProgress {
    pub fn new() -> Self {
        BarProgress {
            bar: ProgressBar::hidden(),
            group: None,
            label: None,
        }
    }

    fn in_group(group: MultiProgress, label: &str) -> Self {
        BarProgress {
            bar: ProgressBar::hidden(),
            group: Some(group),
            label: Some(label.to_string()),
        }
    }
}

impl ProgressSink for BarProgress {
    fn start(&self, total: Option<u64>, label: &str) {
        let style = match total {
            Some(_) => ProgressStyle::with_template(
                "  {msg:<28} [{bar:24.cyan/blue}] {bytes:>10}/{total_bytes} {bytes_per_sec:>11}",
            )
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=> "),
            None => ProgressStyle::with_template("  {msg:<28} {spinner} {bytes:>10}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        };
        match total {
            Some(t) => self.bar.set_length(t),
            None => self.bar.set_length(0),
        }
        self.bar.set_style(style);
        self.bar
            .set_message(truncate(self.label.as_deref().unwrap_or(label), 28));
        match &self.group {
            // `add` gives the bar its draw target, and the group keeps the
            // bars from overwriting one another.
            Some(group) => {
                group.add(self.bar.clone());
            }
            None => self
                .bar
                .set_draw_target(indicatif::ProgressDrawTarget::stderr()),
        }
        self.bar.set_position(0);
    }

    fn advance(&self, delta: u64) {
        self.bar.inc(delta);
    }

    fn finish(&self, message: &str) {
        self.bar.finish_and_clear();
        if let Some(group) = &self.group {
            group.remove(&self.bar);
        }
        if message.is_empty() {
            return;
        }
        log::record(log::Level::Info, &format!("fetched {message}"));
        // In a batch the download is one step of several and `installed X`
        // follows it directly; a line per asset just pushes that off screen.
        if !is_quiet() && self.group.is_none() {
            emit(&format!(
                "{} {}",
                green(&format!("{:>10}", "fetched")),
                message
            ));
        }
    }
}

/// A terminal shared by several progress bars at once.
///
/// Held for the length of a batch. While it lives every status line is printed
/// through `indicatif` rather than straight to stderr, so a bar being redrawn
/// never lands in the middle of a warning.
pub struct Bars {
    group: Option<MultiProgress>,
}

/// Start a group of bars for concurrent work.
pub fn bars() -> Bars {
    if is_quiet() || !std::io::stderr().is_terminal() {
        return Bars { group: None };
    }
    let group = MultiProgress::new();
    *held() = Some(group.clone());
    Bars { group: Some(group) }
}

impl Bars {
    /// One bar in this group, named for the work it is doing.
    pub fn sink(&self, label: &str) -> Box<dyn ProgressSink> {
        match &self.group {
            Some(group) => Box::new(BarProgress::in_group(group.clone(), label)),
            None => Box::new(SilentProgress),
        }
    }
}

impl Drop for Bars {
    fn drop(&mut self) {
        // The global goes first, so the two locks are only ever taken in this
        // order: `emit` holds this one while it suspends the bars, and nothing
        // is left that could want them the other way round.
        let group = held().take();
        if let Some(group) = group.or_else(|| self.group.take()) {
            group.clear().ok();
        }
    }
}

/// Pick the right sink for the current run.
pub fn progress() -> Box<dyn ProgressSink> {
    if is_quiet() || !std::io::stderr().is_terminal() {
        Box::new(SilentProgress)
    } else {
        Box::new(BarProgress::new())
    }
}

/// Shorten to `width`, ending with `…` when it does not fit.
pub fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut s: String = text.chars().take(keep).collect();
    s.push('…');
    s
}

/// Render rows as an aligned table. Empty input produces no output.
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    let cols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().take(cols).enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let header: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:<width$}", h, width = widths[i]))
        .collect();
    out(&bold(header.join("  ").trim_end()));
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .take(cols)
            .enumerate()
            .map(|(i, c)| format!("{:<width$}", c, width = widths[i]))
            .collect();
        out(line.join("  ").trim_end());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_byte_counts() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("abcdef", 10), "abcdef");
        assert_eq!(truncate("abcdef", 4), "abc…");
        // Multi-byte input must not panic or split a character.
        assert_eq!(truncate("ünïcödé-package", 6), "ünïcö…");
    }
}
