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
    /// Converts the action to its stored string representation.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(Action::Install.as_str(), "install");
    /// assert_eq!(Action::Upgrade.as_str(), "upgrade");
    /// assert_eq!(Action::Uninstall.as_str(), "uninstall");
    /// ```
    ///
    /// Returns the string used to store the action in the database.
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
    /// Computes the average duration in milliseconds across timed events.
    ///
    /// The result uses integer division and is `None` when no events have a duration.
    ///
    /// # Examples
    ///
    /// ```
    /// let summary = Summary {
    ///     events: 2,
    ///     installs: 2,
    ///     upgrades: 0,
    ///     uninstalls: 0,
    ///     packages: 2,
    ///     total_duration_ms: 250,
    ///     timed: 2,
    ///     first_at: None,
    ///     last_at: None,
    /// };
    ///
    /// assert_eq!(summary.mean_duration_ms(), Some(125));
    /// ```
    pub fn mean_duration_ms(&self) -> Option<i64> {
        (self.timed > 0).then(|| self.total_duration_ms / self.timed)
    }
}

/// Opens the SQLite statistics database, creating its parent directories and applying pending migrations.
///
/// # Examples
///
/// ```no_run
/// let connection = open(std::path::Path::new("stats.sqlite"))?;
/// # Ok::<(), Error>(())
/// ```
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

/// Records an event without allowing statistics failures to affect the caller.
///
/// Database write failures are reported as warnings.
///
/// # Examples
///
/// ```no_run
/// # let cfg: Config = todo!();
/// # let event: NewEvent<'_> = todo!();
/// record(&cfg, &event);
/// ```
pub fn record(cfg: &Config, event: &NewEvent<'_>) {
    if let Err(e) = record_at(&cfg.stats_db, event) {
        crate::ui::warn(&format!("could not record statistics: {e}"));
    }
}

/// Records an event in the statistics database at the specified path.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or the event cannot be inserted.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// # fn example(event: &NewEvent<'_>) -> Result<()> {
/// record_at(Path::new("stats.sqlite"), event)?;
/// # Ok(())
/// # }
/// ```
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

/// Builds an event describing a package installation or upgrade. A replacement is classified as an upgrade regardless of version ordering.
///
/// # Examples
///
/// ```no_run
/// # let package: &crate::model::InstalledPackage = todo!();
/// let event = install_event(
///     package,
///     None,
///     Some(250),
///     "1.2.3",
///     "https://example.com/package",
///     "x86_64-unknown-linux-gnu",
/// );
///
/// assert_eq!(event.action, "install");
/// ```
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

/// Builds an uninstall event for an installed package without recording a duration.
///
/// # Examples
///
/// ```ignore
/// let event = uninstall_event(&package, "1.2.3", "source", "target");
/// assert_eq!(event.action, "uninstall");
/// assert_eq!(event.duration_ms, None);
/// ```
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

/// Reads package installation history, ordered from newest to oldest.
///
/// # Parameters
///
/// * `package` filters results to one package when provided.
/// * `limit` restricts the maximum number of returned events.
///
/// # Examples
///
/// ```ignore
/// let events = history(&cfg, Some("example"), 10)?;
/// ```
///
/// # Returns
///
/// The matching history events, or an error if the history cannot be read.
pub fn history(cfg: &Config, package: Option<&str>, limit: i64) -> Result<Vec<Event>> {
    history_at(&cfg.stats_db, package, limit)
}

/// Reads package history from an explicit database path.
///
/// A missing database is treated as an empty history. Results are ordered from
/// newest to oldest and may be filtered by package and limited in count.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let events = history_at(Path::new("history.sqlite"), None, 20)?;
/// assert!(events.len() <= 20);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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

/// Aggregates statistics for all events recorded in the configured statistics database.
///
/// # Examples
///
/// ```
/// # let cfg = Config::load(None)?;
/// let summary = summary(&cfg)?;
/// println!("{} events recorded", summary.events);
/// # Ok::<(), _>(())
/// ```
pub fn summary(cfg: &Config) -> Result<Summary> {
    summary_at(&cfg.stats_db)
}

/// Computes aggregate statistics for the events stored at a database path.
///
/// Returns an empty summary when the database does not exist.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let summary = summary_at(Path::new("missing-stats.sqlite"))?;
/// assert_eq!(summary.events, 0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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

/// Converts a Diesel read error into a descriptive statistics error.
///
/// # Examples
///
/// ```
/// let error = read_err(diesel::result::Error::NotFound);
/// assert!(error.to_string().starts_with("could not read statistics:"));
/// ```
fn read_err(e: diesel::result::Error) -> Error {
    Error::msg(format!("could not read statistics: {e}"))
}

/// Counts events with the specified action.
///
/// # Examples
///
/// ```
/// use diesel::{Connection, connection::SimpleConnection};
///
/// let mut conn = diesel::sqlite::SqliteConnection::establish(":memory:").unwrap();
/// conn.batch_execute(
///     "CREATE TABLE events (action TEXT NOT NULL);
///      INSERT INTO events (action) VALUES ('install');",
/// ).unwrap();
///
/// assert_eq!(count_of(&mut conn, Action::Install).unwrap(), 1);
/// ```
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
    use crate::model::{InstalledPackage, ManifestOrigin, PackageRef, TargetSpec, Version};
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

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

    fn installed_package() -> InstalledPackage {
        InstalledPackage {
            name: "ripgrep".to_string(),
            version: Version::parse("14.1.0"),
            source: PackageRef::github("BurntSushi/ripgrep"),
            tag: "14.1.0".to_string(),
            target: TargetSpec::host(),
            asset_name: "ripgrep-14.1.0.tar.gz".to_string(),
            sha256: "a".repeat(64),
            checksum_verified: false,
            installed_at: 0,
            prefix: PathBuf::from("/store/ripgrep/14.1.0"),
            links: Vec::new(),
            pinned: false,
            origin: ManifestOrigin::Inferred,
            manifest: None,
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
        assert!(
            history_at(&path, Some("ripgrep"), 0).unwrap().is_empty(),
            "a zero limit must return no rows"
        );
    }

    #[test]
    fn every_event_field_round_trips_without_loss() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        let mut written = event("unicode-☃", "2.0.0-rc.1+build.7", Action::Upgrade);
        written.previous_version = Some("1.9.0");
        written.tag = "release/2.0.0-rc.1";
        written.source = "plugin:forge/package";
        written.target = "macos-universal";
        written.asset_name = "package 'quoted' universal.tar.gz";
        written.sha256 = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
        written.checksum_verified = false;
        written.duration_ms = None;
        written.at = -1;
        written.ketch_version = "9.8.7-test";

        record_at(&path, &written).unwrap();
        let rows = history_at(&path, None, 1).unwrap();
        let read = &rows[0];

        assert_eq!(read.package, written.package);
        assert_eq!(read.action, written.action);
        assert_eq!(read.version, written.version);
        assert_eq!(read.previous_version.as_deref(), written.previous_version);
        assert_eq!(read.tag, written.tag);
        assert_eq!(read.source, written.source);
        assert_eq!(read.target, written.target);
        assert_eq!(read.asset_name, written.asset_name);
        assert_eq!(read.sha256, written.sha256);
        assert_eq!(read.checksum_verified, written.checksum_verified);
        assert_eq!(read.duration_ms, written.duration_ms);
        assert_eq!(read.at, written.at);
        assert_eq!(read.ketch_version, written.ketch_version);
    }

    #[test]
    fn event_builders_distinguish_first_install_replacement_and_uninstall() {
        let package = installed_package();
        let source = package.source.to_string();
        let target = package.target.to_string();

        let first = install_event(&package, None, Some(250), "14.1.0", &source, &target);
        assert_eq!(first.action, Action::Install.as_str());
        assert_eq!(first.previous_version, None);
        assert_eq!(first.duration_ms, Some(250));
        assert_eq!(first.package, package.name);
        assert_eq!(first.asset_name, package.asset_name);
        assert_eq!(first.checksum_verified, package.checksum_verified);

        // Replacing a package is an upgrade event even if the requested
        // version moves backwards; the operation, not semver ordering, is the
        // history fact being recorded.
        let replacement = install_event(
            &package,
            Some("15.0.0"),
            Some(1),
            "14.1.0",
            &source,
            &target,
        );
        assert_eq!(replacement.action, Action::Upgrade.as_str());
        assert_eq!(replacement.previous_version, Some("15.0.0"));

        let removal = uninstall_event(&package, "14.1.0", &source, &target);
        assert_eq!(removal.action, Action::Uninstall.as_str());
        assert_eq!(removal.previous_version, None);
        assert_eq!(removal.duration_ms, None, "removals do not affect the mean");
        assert_eq!(removal.version, "14.1.0");
        assert_eq!(removal.source, source);
        assert_eq!(removal.target, target);
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

    #[test]
    fn a_corrupt_database_is_reported_and_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stats.db");
        let corrupt = b"this is not sqlite";
        std::fs::write(&path, corrupt).unwrap();

        let history_error = history_at(&path, None, 10).unwrap_err().to_string();
        let summary_error = summary_at(&path).unwrap_err().to_string();

        assert!(history_error.contains("could not"), "{history_error}");
        assert!(summary_error.contains("could not"), "{summary_error}");
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
    }
}
