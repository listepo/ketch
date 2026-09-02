//! The log file.
//!
//! Every status line ketch writes also goes here, minus the colour and plus a
//! timestamp — including the lines `--quiet` swallowed and the debug lines
//! `--verbose` would have shown. A terminal scrolls away and a failed install
//! is usually reported hours later; the log is what is left to read.
//!
//! Not the data on stdout. `ketch list` answers a question the caller already
//! has; recording the answer would only make the log harder to search for the
//! run that went wrong.
//!
//! It is wired in exactly one place, `ui.rs`, for the same reason every other
//! byte of output is: one choke point means a new command cannot forget.
//!
//! Nothing here is allowed to fail loudly. A package manager that refuses to
//! install because it could not open its log is broken, so an unwritable log
//! is reported once and then ignored for the rest of the run.

use crate::config::Config;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Mutex;

/// Rotate at this size. One old file is kept, so the logs cost at most twice
/// this much and never need pruning by hand.
const MAX_BYTES: u64 = 5 * 1024 * 1024;

/// How much gets written.
///
/// Ordered so a record is written when its level is at or below the configured
/// one, which puts `Off` first and makes it filter everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Level {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

impl Level {
    /// Whether a record at this level is written to a log configured at
    /// `configured`. `Off` is below every real level, so it filters all of them.
    fn passes(self, configured: Level) -> bool {
        self <= configured && configured != Level::Off
    }

    fn label(self) -> &'static str {
        match self {
            Level::Off => "OFF",
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label().to_ascii_lowercase())
    }
}

impl FromStr for Level {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "false" => Ok(Level::Off),
            "error" => Ok(Level::Error),
            "warn" | "warning" => Ok(Level::Warn),
            "info" => Ok(Level::Info),
            "debug" | "trace" => Ok(Level::Debug),
            other => Err(format!(
                "unknown log level `{other}`; use off, error, warn, info or debug"
            )),
        }
    }
}

/// How a record is written.
///
/// Two, because a log file has two audiences. The default is the line format
/// every CLI writes and every person can read; `json` is JSON Lines, which is
/// what a log shipper wants and what `jq` reads without a parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Text,
    Json,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Text => "text",
            Format::Json => "json",
        })
    }
}

impl FromStr for Format {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "text" | "plain" | "logfmt" => Ok(Format::Text),
            "json" | "jsonl" | "ndjson" => Ok(Format::Json),
            other => Err(format!("unknown log format `{other}`; use text or json")),
        }
    }
}

struct Sink {
    file: File,
    path: PathBuf,
    level: Level,
    format: Format,
    pid: u32,
}

static SINK: Mutex<Option<Sink>> = Mutex::new(None);

/// Open the log for this run. Called once, as soon as the config exists.
///
/// The failure is deliberately soft: an unwritable log is a warning on stderr,
/// not a reason for `ketch install` to stop working.
pub fn init(cfg: &Config) {
    if cfg.log_level == Level::Off {
        return;
    }
    match open(&cfg.log_file, cfg.log_level, cfg.log_format) {
        Ok(sink) => {
            set(Some(sink));
            record(
                Level::Info,
                &format!(
                    "ketch {} · {}",
                    env!("CARGO_PKG_VERSION"),
                    std::env::args().skip(1).collect::<Vec<_>>().join(" ")
                ),
            );
        }
        // Not through `ui::warn`: that would try to log the failure to log.
        Err(e) => eprintln!("   warning could not open {}: {e}", cfg.log_file.display()),
    }
}

/// Where this run is logging, once `init` has succeeded.
pub fn path() -> Option<PathBuf> {
    guard().as_ref().map(|sink| sink.path.clone())
}

/// Write one record. Never fails, never blocks on anything but the file.
pub fn record(level: Level, message: &str) {
    let mut sink = guard();
    let Some(sink) = sink.as_mut() else {
        return;
    };
    if !level.passes(sink.level) {
        return;
    }
    let line = match sink.format {
        Format::Text => format!(
            "{} [{}] {:<5} {}\n",
            timestamp(now()),
            sink.pid,
            level.label(),
            escape(message)
        ),
        Format::Json => format!(
            "{}\n",
            serde_json::json!({
                "time": timestamp(now()),
                "level": level.to_string(),
                "pid": sink.pid,
                "msg": message,
            })
        ),
    };
    // A log that cannot be written is not a reason to fail the command it was
    // recording, and reporting it here would recurse straight back in.
    let _ = sink.file.write_all(line.as_bytes());
}

fn guard() -> std::sync::MutexGuard<'static, Option<Sink>> {
    // A panicking thread must not silence the log for the rest of the run.
    SINK.lock().unwrap_or_else(|e| e.into_inner())
}

fn set(sink: Option<Sink>) {
    *guard() = sink;
}

fn open(path: &Path, level: Level, format: Format) -> std::io::Result<Sink> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    rotate(path);
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    Ok(Sink {
        file,
        path: path.to_path_buf(),
        level,
        format,
        pid: std::process::id(),
    })
}

/// Move a full log aside, keeping one generation.
///
/// Best-effort on purpose: if the rename fails the log simply keeps growing,
/// which is better than refusing to log at all.
fn rotate(path: &Path) {
    let full = std::fs::metadata(path).is_ok_and(|m| m.len() >= MAX_BYTES);
    if full {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
}

/// Flatten a record onto one line.
///
/// Every record is one line, so a log stays greppable and every reader from
/// `tail` to a shipper agrees where a record ends. Control characters go for
/// the same reason they go from a changelog: this file gets `cat`ed.
fn escape(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for c in message.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Seconds since the epoch as RFC 3339 UTC, which is what every log format in
/// common use writes and every parser in common use reads.
fn timestamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Days since the epoch to a calendar date, by Howard Hinnant's `civil_from_days`.
///
/// Written out rather than pulled in: a date crate is a dependency, a build,
/// and a supply chain, and this is the only date ketch formats.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_match_dates_everybody_knows() {
        assert_eq!(timestamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(timestamp(86_399), "1970-01-01T23:59:59Z");
        assert_eq!(timestamp(86_400), "1970-01-02T00:00:00Z");
        assert_eq!(timestamp(1_000_000_000), "2001-09-09T01:46:40Z");
        assert_eq!(timestamp(1_700_000_000), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn leap_days_land_on_the_29th() {
        // 2024-02-29T00:00:00Z and 2000-02-29T00:00:00Z, the century that is
        // a leap year and the one every naive rule gets wrong.
        assert_eq!(timestamp(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(timestamp(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(timestamp(4_107_542_400), "2100-03-01T00:00:00Z");
    }

    #[test]
    fn a_record_is_always_one_line() {
        let flattened = escape("first\nsecond\ttabbed\u{1b}[2Kcleared");
        assert_eq!(flattened, "first\\nsecond\\ttabbed[2Kcleared");
        assert!(!flattened.contains('\n'));
    }

    #[test]
    fn a_level_admits_itself_and_everything_more_serious() {
        assert!(Level::Warn.passes(Level::Info));
        assert!(Level::Error.passes(Level::Info));
        assert!(Level::Info.passes(Level::Info));
        assert!(!Level::Debug.passes(Level::Info));
        assert!(!Level::Warn.passes(Level::Error));
        assert!(Level::Debug.passes(Level::Debug));
    }

    #[test]
    fn off_writes_nothing_at_all() {
        for level in [Level::Error, Level::Warn, Level::Info, Level::Debug] {
            assert!(!level.passes(Level::Off), "{level} escaped an off log");
        }
    }

    #[test]
    fn levels_and_formats_parse_the_names_people_type() {
        assert_eq!("WARNING".parse(), Ok(Level::Warn));
        assert_eq!("off".parse(), Ok(Level::Off));
        assert_eq!(" Debug ".parse(), Ok(Level::Debug));
        assert!("chatty".parse::<Level>().is_err());
        assert_eq!("ndjson".parse(), Ok(Format::Json));
        assert_eq!("text".parse(), Ok(Format::Text));
        assert!("xml".parse::<Format>().is_err());
    }
}
