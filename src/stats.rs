//! The statistics database: what ketch did, and when.
//!
//! `state.json` records what is installed *now*, and rewrites itself whole on
//! every change. That is the right shape for current state and the wrong one
//! for history: it cannot say which version was installed in March, or how long
//! installs take, because it never kept either. This module owns the other
//! half — an append-only record of every install, upgrade and uninstall, in
//! SQLite so it can be queried instead of parsed.
//!
//! Nothing here is authoritative. Losing `stats.db` costs history, never
//! packages, and that asymmetry is what makes the write path best effort: a
//! package manager that refused to install because it could not record a
//! statistic would be trading the user's actual goal for bookkeeping. Every
//! failure while recording is a warning and nothing more. Reading is not best
//! effort — a query the user asked for reports why it could not answer.
//!
//! SQLite is compiled in (`libsqlite3-sys/bundled`) and the schema travels
//! inside the binary (`embed_migrations!`), so the single-binary promise
//! survives: no system SQLite to find, no migration files to ship, and a fresh
//! machine opens a working database on first write.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::now_unix;
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use std::path::Path;

diesel::table! {
    events (id) {
        id -> BigInt,
        package -> Text,
        action -> Text,
        version -> Text,
        previous_version -> Nullable<Text>,
        tag -> Text,
        source -> Text,
        target -> Text,
        asset_name -> Text,
        sha256 -> Text,
        checksum_verified -> Bool,
        duration_ms -> Nullable<Integer>,
        at -> BigInt,
        ketch_version -> Text,
    }
}

/// The schema, carried inside the binary rather than shipped beside it.
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Another ketch may be installing at the same moment. SQLite serialises
/// writers, and the alternative to waiting is an immediate "database is
/// locked" — a recorded install lost to a race that resolves itself in
/// milliseconds.
const BUSY_TIMEOUT_MS: i64 = 5_000;

/// What happened to a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Install,
    Upgrade,
    Uninstall,
}

impl Action {
    /// The value stored in the `action` column.
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Upgrade => "upgrade",
            Action::Uninstall => "uninstall",
        }
    }
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One row, on its way in.
#[derive(Debug, Insertable)]
#[diesel(table_name = events)]
pub struct NewEvent<'a> {
    pub package: &'a str,
    pub action: &'a str,
    pub version: &'a str,
    pub previous_version: Option<&'a str>,
    pub tag: &'a str,
    pub source: &'a str,
    pub target: &'a str,
    pub asset_name: &'a str,
    pub sha256: &'a str,
    pub checksum_verified: bool,
    pub duration_ms: Option<i32>,
    pub at: i64,
    pub ketch_version: &'a str,
}

/// One row, on its way out.
#[derive(Debug, Clone, Queryable, Selectable, serde::Serialize)]
#[diesel(table_name = events)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Event {
    pub id: i64,
    pub package: String,
    pub action: String,
    pub version: String,
    pub previous_version: Option<String>,
    pub tag: String,
    pub source: String,
    pub target: String,
    pub asset_name: String,
    pub sha256: String,
    pub checksum_verified: bool,
    pub duration_ms: Option<i32>,
    pub at: i64,
    pub ketch_version: String,
}

/// Everything `ketch stats` reports, in one pass over the table.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub events: i64,
    pub installs: i64,
    pub upgrades: i64,
    pub uninstalls: i64,
    /// Distinct packages that have ever appeared, installed or not any more.
    pub packages: i64,
    pub total_duration_ms: i64,
    /// Rows that carried a duration, which is the divisor for the mean.
    pub timed: i64,
    pub first_at: Option<i64>,
    pub last_at: Option<i64>,
}

impl Summary {
    /// Mean install time, or `None` before anything has been timed.
    ///
    /// Computed here rather than in SQL because `AVG` over a `BIGINT` comes
    /// back as a decimal, and carrying a bignum dependency to divide two
    /// integers is a poor trade.
    pub fn mean_duration_ms(&self) -> Option<i64> {
        (self.timed > 0).then(|| self.total_duration_ms / self.timed)
    }
}

/// Open the database, creating and migrating it if this is the first write.
fn open(path: &Path) -> Result<SqliteConnection> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;

    // SQLite takes the path as a string, and a lossy conversion would silently
    // open a *different* file than the one configured.
    let url = path.to_str().ok_or_else(|| {
        Error::msg(format!(
            "{} is not valid UTF-8, and SQLite cannot be pointed at it",
            path.display()
        ))
    })?;

    let mut conn = SqliteConnection::establish(url)
        .map_err(|e| Error::msg(format!("could not open {}: {e}", path.display())))?;

    diesel::sql_query(format!("PRAGMA busy_timeout = {BUSY_TIMEOUT_MS};"))
        .execute(&mut conn)
        .map_err(|e| Error::msg(format!("could not configure {}: {e}", path.display())))?;

    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| Error::msg(format!("could not migrate {}: {e}", path.display())))?;

    Ok(conn)
}

/// Record one event. Never fails an install: a database that cannot be written
/// is worth a warning, not a rolled-back package.
pub fn record(cfg: &Config, event: &NewEvent<'_>) {
    if let Err(e) = record_at(&cfg.stats_db, event) {
        crate::ui::warn(&format!("could not record statistics: {e}"));
    }
}

/// The same, against an explicit path, so tests need no `Config`.
pub fn record_at(path: &Path, event: &NewEvent<'_>) -> Result<()> {
    let mut conn = open(path)?;
    // Diesel binds every value as a parameter. Package names and asset names
    // come from manifests and releases written by other people, and this is the
    // reason none of them can end a statement early and start their own.
    diesel::insert_into(events::table)
        .values(event)
        .execute(&mut conn)
        .map_err(|e| Error::msg(format!("could not record an event: {e}")))?;
    Ok(())
}

/// Build the event describing an install or an upgrade.
///
/// A package that replaced an earlier version is an upgrade even when the new
/// version is the same or lower — what the record is for is that the installed
/// version *changed*, not which direction it moved.
pub fn install_event<'a>(
    pkg: &'a crate::model::InstalledPackage,
    replaced: Option<&'a str>,
    duration_ms: Option<i32>,
    version: &'a str,
    source: &'a str,
    target: &'a str,
) -> NewEvent<'a> {
    NewEvent {
        package: &pkg.name,
        action: if replaced.is_some() {
            Action::Upgrade.as_str()
        } else {
            Action::Install.as_str()
        },
        version,
        previous_version: replaced,
        tag: &pkg.tag,
        source,
        target,
        asset_name: &pkg.asset_name,
        sha256: &pkg.sha256,
        checksum_verified: pkg.checksum_verified,
        duration_ms,
        at: now_unix() as i64,
        ketch_version: env!("CARGO_PKG_VERSION"),
    }
}

/// Build the event describing an uninstall.
///
/// No duration: an uninstall unlinks and deletes rather than going through the
/// download pipeline, so timing it would measure a different thing than every
/// other row and quietly skew the mean.
pub fn uninstall_event<'a>(
    pkg: &'a crate::model::InstalledPackage,
    version: &'a str,
    source: &'a str,
    target: &'a str,
) -> NewEvent<'a> {
    NewEvent {
        package: &pkg.name,
        action: Action::Uninstall.as_str(),
        version,
        previous_version: None,
        tag: &pkg.tag,
        source,
        target,
        asset_name: &pkg.asset_name,
        sha256: &pkg.sha256,
        checksum_verified: pkg.checksum_verified,
        duration_ms: None,
        at: now_unix() as i64,
        ketch_version: env!("CARGO_PKG_VERSION"),
    }
}

/// A package's version history, newest first. `None` reads every package.
pub fn history(cfg: &Config, package: Option<&str>, limit: i64) -> Result<Vec<Event>> {
    history_at(&cfg.stats_db, package, limit)
}

/// The same, against an explicit path, so tests need no `Config`.
pub fn history_at(path: &Path, package: Option<&str>, limit: i64) -> Result<Vec<Event>> {
    // Reading must not create the database. `ketch history` on a machine that
    // has never installed anything should answer "nothing" and leave no file
    // behind, the same way a missing `state.json` is an empty state.
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut conn = open(path)?;
    let mut query = events::table.into_boxed();
    if let Some(name) = package {
        query = query.filter(events::package.eq(name));
    }
    query
        .order((events::at.desc(), events::id.desc()))
        .limit(limit)
        .select(Event::as_select())
        .load(&mut conn)
        .map_err(|e| Error::msg(format!("could not read history: {e}")))
}

/// Aggregate statistics over everything recorded.
pub fn summary(cfg: &Config) -> Result<Summary> {
    summary_at(&cfg.stats_db)
}

/// The same, against an explicit path, so tests need no `Config`.
pub fn summary_at(path: &Path) -> Result<Summary> {
    use diesel::dsl::{count, max, min, sum};
    use diesel::expression_methods::AggregateExpressionMethods;

    if !path.exists() {
        return Ok(Summary::default());
    }
    let mut conn = open(path)?;

    Ok(Summary {
        events: events::table
            .count()
            .get_result(&mut conn)
            .map_err(read_err)?,
        installs: count_of(&mut conn, Action::Install)?,
        upgrades: count_of(&mut conn, Action::Upgrade)?,
        uninstalls: count_of(&mut conn, Action::Uninstall)?,
        packages: events::table
            .select(count(events::package).aggregate_distinct())
            .get_result(&mut conn)
            .map_err(read_err)?,
        total_duration_ms: events::table
            .select(sum(events::duration_ms))
            .get_result::<Option<i64>>(&mut conn)
            .map_err(read_err)?
            .unwrap_or(0),
        timed: events::table
            .filter(events::duration_ms.is_not_null())
            .count()
            .get_result(&mut conn)
            .map_err(read_err)?,
        first_at: events::table
            .select(min(events::at))
            .get_result(&mut conn)
            .map_err(read_err)?,
        last_at: events::table
            .select(max(events::at))
            .get_result(&mut conn)
            .map_err(read_err)?,
    })
}

fn read_err(e: diesel::result::Error) -> Error {
    Error::msg(format!("could not read statistics: {e}"))
}

fn count_of(conn: &mut SqliteConnection, action: Action) -> Result<i64> {
    events::table
        .filter(events::action.eq(action.as_str()))
        .count()
        .get_result(conn)
        .map_err(read_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// A digest-shaped constant, so the helper need not leak a built string.
    const SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    fn event<'a>(package: &'a str, version: &'a str, action: Action) -> NewEvent<'a> {
        NewEvent {
            package,
            action: action.as_str(),
            version,
            previous_version: None,
            tag: "v1",
            source: "github:o/r",
            target: "aarch64-apple-darwin",
            asset_name: "a.tar.gz",
            sha256: SHA,
            checksum_verified: true,
            duration_ms: Some(100),
            at: 0,
            ketch_version: "0.1.0",
        }
    }

    #[test]
    fn a_missing_database_is_created_and_migrated_on_first_write() {
        let dir = tempfile::tempdir().unwrap();
        // Two levels down: the parent does not exist either.
        let path = dir.path().join("nested").join("stats.db");
        record_at(&path, &event("ripgrep", "14.1.0", Action::Install)).unwrap();

        assert!(path.exists(), "the database should have been created");
        assert_eq!(history_at(&path, None, 10).unwrap().len(), 1);
    }

    #[test]
    fn opening_an_existing_database_again_does_not_re_run_migrations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        record_at(&path, &event("ripgrep", "14.1.0", Action::Install)).unwrap();
        // A second migration run would fail on the already-created table, and
        // the row from the first write must survive.
        record_at(&path, &event("ripgrep", "14.1.1", Action::Upgrade)).unwrap();
        assert_eq!(history_at(&path, None, 10).unwrap().len(), 2);
    }

    #[test]
    fn history_is_newest_first_and_scoped_to_one_package() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");

        for (i, version) in ["1.0.0", "1.1.0", "2.0.0"].iter().enumerate() {
            let mut e = event("ripgrep", version, Action::Upgrade);
            e.at = i as i64;
            record_at(&path, &e).unwrap();
        }
        record_at(&path, &event("fd", "9.0.0", Action::Install)).unwrap();

        let rg = history_at(&path, Some("ripgrep"), 10).unwrap();
        let versions: Vec<&str> = rg.iter().map(|e| e.version.as_str()).collect();
        assert_eq!(versions, ["2.0.0", "1.1.0", "1.0.0"], "newest first");
        assert!(rg.iter().all(|e| e.package == "ripgrep"), "fd leaked in");
    }

    #[test]
    fn rows_written_in_the_same_second_still_come_back_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        // `at` has one-second resolution, so a fast upgrade ties with the
        // install it replaced; insertion order is what breaks the tie.
        for version in ["1.0.0", "2.0.0"] {
            let mut e = event("ripgrep", version, Action::Upgrade);
            e.at = 42;
            record_at(&path, &e).unwrap();
        }
        let seen = history_at(&path, Some("ripgrep"), 10).unwrap();
        assert_eq!(seen[0].version, "2.0.0", "the later write should lead");
    }

    #[test]
    fn the_limit_is_honoured() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        for i in 0..5 {
            let mut e = event("ripgrep", "1.0.0", Action::Upgrade);
            e.at = i;
            record_at(&path, &e).unwrap();
        }
        assert_eq!(history_at(&path, Some("ripgrep"), 2).unwrap().len(), 2);
    }

    #[test]
    fn a_summary_counts_each_action_and_averages_only_what_was_timed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");

        let mut first = event("ripgrep", "1.0.0", Action::Install);
        first.duration_ms = Some(100);
        first.at = 10;
        record_at(&path, &first).unwrap();

        let mut second = event("ripgrep", "2.0.0", Action::Upgrade);
        second.duration_ms = Some(300);
        second.at = 20;
        record_at(&path, &second).unwrap();

        // An uninstall has no duration, and must not drag the mean down.
        let mut third = event("fd", "9.0.0", Action::Uninstall);
        third.duration_ms = None;
        third.at = 30;
        record_at(&path, &third).unwrap();

        let s = summary_at(&path).unwrap();
        assert_eq!(s.events, 3);
        assert_eq!(s.installs, 1);
        assert_eq!(s.upgrades, 1);
        assert_eq!(s.uninstalls, 1);
        assert_eq!(s.packages, 2, "ripgrep and fd");
        assert_eq!(s.timed, 2);
        assert_eq!(s.mean_duration_ms(), Some(200));
        assert_eq!(s.first_at, Some(10));
        assert_eq!(s.last_at, Some(30));
    }

    #[test]
    fn an_empty_database_summarises_to_zero_rather_than_failing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        let s = summary_at(&path).unwrap();
        assert_eq!(s.events, 0);
        assert_eq!(s.mean_duration_ms(), None, "no divisor, no mean");
        assert_eq!(s.first_at, None);

        assert!(history_at(&path, None, 10).unwrap().is_empty());
        assert!(!path.exists(), "reading must not create the database");
    }

    #[test]
    fn a_name_that_looks_like_sql_is_stored_as_a_name() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        // Manifests are written by other people; this one is a package name,
        // and the only thing it may ever be is a package name.
        let hostile = "rg'); DROP TABLE events;--";
        record_at(&path, &event(hostile, "1.0.0", Action::Install)).unwrap();

        let seen = history_at(&path, Some(hostile), 10).unwrap();
        assert_eq!(seen.len(), 1, "the table should still be there");
        assert_eq!(seen[0].package, hostile);
    }
}
