//! Read-only commands.
//!
//! These never take the lock and never write. Their data output goes to stdout
//! through `ui::out`/`ui::table` so it can be piped while progress and warnings
//! stay on stderr.

use crate::changelog::{self, Entry, Origin};
use crate::cli::{
    ChangelogArgs, HistoryArgs, InfoArgs, ListArgs, OutdatedArgs, SearchArgs, StatsArgs, WhyArgs,
};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::install;
use crate::manifest::Resolver;
use crate::model::{InstalledPackage, Manifest, ManifestOrigin, PackageSpec, Release, VersionSpec};
use crate::source::{ListOpts, SourceRegistry};
use crate::state::State;
use crate::stats;
use crate::ui;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Lists installed packages in JSON, name-only, or tabular format.
///
/// # Examples
///
/// ```no_run
/// let cfg = Config::load(None)?;
/// let args = ListArgs {
///     json: false,
///     names_only: false,
/// };
///
/// list(&cfg, args)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
///
/// Returns an error if the installed state cannot be loaded or JSON output
/// cannot be serialized.
pub fn list(cfg: &Config, args: ListArgs) -> Result<()> {
    let state = State::load(cfg)?;
    let packages: Vec<&InstalledPackage> = state.iter().collect();

    if args.json {
        return print_json(&packages);
    }
    if args.names_only {
        for pkg in &packages {
            ui::out(&crate::changelog::sanitize(&pkg.name));
        }
        return Ok(());
    }
    if packages.is_empty() {
        ui::out("nothing installed");
        return Ok(());
    }

    let rows: Vec<Vec<String>> = packages
        .iter()
        .map(|pkg| {
            let retained = if pkg.retained.is_empty() {
                String::new()
            } else {
                format!(" (+{} retained)", pkg.retained.len())
            };
            vec![
                pkg.name.clone(),
                format!(
                    "{}{}{retained}",
                    pkg.version,
                    if pkg.pinned { " (pinned)" } else { "" }
                ),
                pkg.source.to_string(),
            ]
        })
        .collect();
    ui::table(&["package", "version", "source"], &rows);
    Ok(())
}

/// Reports installed packages that have a newer upstream release.
///
/// `--json` writes an object, not a bare array: `status`, `outdated`
/// (packages with a newer release), `failed` (each source that could not
/// be checked), and `unreachable` (that count). A failed check still
/// exits non-zero, matching the text "N could not be checked".
pub fn outdated(cfg: &Config, args: OutdatedArgs) -> Result<()> {
    let state = State::load(cfg)?;
    let sources = SourceRegistry::load(cfg);
    let prerelease = args.prerelease || cfg.prerelease;
    // Local packages have no upstream release stream; skipping them keeps
    // `outdated` from treating the synthetic tag as something to refresh.
    let pkgs: Vec<&InstalledPackage> = state
        .iter()
        .filter(|pkg| pkg.source.scheme != "local")
        .collect();

    let jobs = super::pkg::jobs(cfg, args.jobs).min(pkgs.len());
    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(pkg) = pkgs.get(i) else { return };
                ui::step("checking", &pkg.name);
                let result = install::latest_release(&sources, pkg, prerelease);
                done.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push((i, result));
            });
        }
    });
    let mut results = done.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by_key(|(i, _)| *i);

    let mut rows = Vec::new();
    let mut outdated = Vec::new();
    let mut failed = Vec::new();
    let (mut checked, mut unreachable) = (0usize, 0usize);
    for (i, result) in results {
        let pkg = pkgs[i];
        let release = match result {
            Ok(r) => r,
            // Reporting is best-effort: one unreachable source must not hide
            // the rest of the answer.
            Err(e) => {
                ui::warn(&format!("{}: {e}", pkg.name));
                failed.push(serde_json::json!({
                    "name": pkg.name,
                    "error": e.to_string(),
                }));
                unreachable += 1;
                continue;
            }
        };
        checked += 1;
        // Match `upgrade`: a retagged release is not newer, and a source that
        // still reports the installed tag is current even if its version
        // string somehow parses differently.
        if release.tag == pkg.tag || release.version <= pkg.version {
            continue;
        }
        rows.push(vec![
            pkg.name.clone(),
            pkg.version.to_string(),
            release.version.to_string(),
            if pkg.pinned {
                "pinned".into()
            } else {
                String::new()
            },
        ]);
        outdated.push(serde_json::json!({
            "name": pkg.name,
            "installed": pkg.version.to_string(),
            "latest": release.version.to_string(),
            "tag": release.tag,
            "pinned": pkg.pinned,
        }));
    }

    if args.json {
        print_json(&outdated_report(checked, &outdated, &failed))?;
    }
    if unreachable > 0 {
        return Err(Error::msg(if checked == 0 {
            format!("could not check any of the {unreachable} packages; see the warnings above")
        } else {
            format!(
                "{unreachable} of {} packages could not be checked; see the warnings above",
                pkgs.len()
            )
        }));
    }
    if args.json {
        return Ok(());
    }
    if rows.is_empty() {
        ui::out(&if unreachable > 0 {
            format!("everything checked is up to date ({unreachable} could not be checked)")
        } else {
            "everything is up to date".to_string()
        });
        return Ok(());
    }
    ui::table(&["package", "installed", "latest", ""], &rows);
    Ok(())
}

pub fn info(cfg: &Config, args: InfoArgs) -> Result<()> {
    let state = State::load(cfg)?;
    let installed = state.find(&args.package).cloned();
    let spec = PackageSpec::parse(&args.package);

    // An installed package always has an answer, even when the registry has
    // forgotten the name it was installed under.
    let manifest = match Resolver::new(cfg)?.resolve(&spec) {
        Ok((m, origin)) => {
            ui::debug(&format!("manifest from {}", describe_origin(&origin)));
            m
        }
        Err(e) => match &installed {
            Some(pkg) => pkg
                .manifest
                .clone()
                .unwrap_or_else(|| Manifest::inferred(pkg.source.clone())),
            None => return Err(e),
        },
    };

    let sources = SourceRegistry::load(cfg);
    let source = match sources.for_ref(&manifest.source) {
        Ok(s) => Some(s),
        Err(e) => {
            ui::warn(&format!("{}: {e}", manifest.name));
            None
        }
    };
    let described = source.as_ref().and_then(|s| {
        s.describe(&manifest.source.id).unwrap_or_else(|e| {
            ui::debug(&format!("describe failed: {e}"));
            None
        })
    });
    let opts = ListOpts {
        include_prerelease: cfg.prerelease || manifest.prerelease,
        ..Default::default()
    };
    let release: Option<Release> = match source.as_ref() {
        Some(s) => match s.resolve(&manifest.source.id, &VersionSpec::Latest, &opts) {
            Ok(r) => Some(r),
            Err(e) => {
                ui::warn(&format!("{}: {e}", manifest.name));
                None
            }
        },
        None => None,
    };

    let scored = match &release {
        Some(r) if args.assets => {
            let platform = crate::platform::host()?;
            install::score_assets(cfg, platform.as_ref(), r, &manifest.asset)
        }
        _ => Vec::new(),
    };

    if args.json {
        return print_json(&serde_json::json!({
            "name": ui::printable(&manifest.name),
            "source": manifest.source.to_string(),
            "url": source.as_ref().and_then(|s| s.web_url(&manifest.source.id)),
            "description": json_prose(manifest.description.clone().or_else(|| described.as_ref().and_then(|d| d.description.clone()))),
            "homepage": json_prose(manifest.homepage.clone().or_else(|| described.as_ref().and_then(|d| d.homepage.clone()))),
            "stars": described.as_ref().and_then(|d| d.stars),
            "license": json_prose(described.as_ref().and_then(|d| d.license.clone())),
            "archived": described.as_ref().map(|d| d.archived).unwrap_or(false),
            "latest": release.as_ref().map(|r| r.version.to_string()),
            "latest_tag": release.as_ref().map(|r| ui::printable(&r.tag)),
            "installed": installed.as_ref().map(|p| p.version.to_string()),
            "pinned": installed.as_ref().map(|p| p.pinned).unwrap_or(false),
            "publisher_trust": installed.as_ref().map(|p| p.publisher_trust()),
            "provenance": installed.as_ref().and_then(|p| p.provenance.as_ref()),
            "retained": installed.as_ref().map(|p| {
                p.retained.iter().map(|r| serde_json::json!({
                    "version": r.version.to_string(),
                    "prefix": r.prefix.display().to_string(),
                    "sha256": r.sha256,
                    "trust": r.trust,
                })).collect::<Vec<_>>()
            }).unwrap_or_default(),
            "retention": { "keep": state.retention.keep },
            "local_kind": installed.as_ref().and_then(|p| p.local_kind).map(|k| k.to_string())
                .or_else(|| (manifest.source.scheme == "local").then(|| "local".to_string())),
            "local_path": installed.as_ref().and_then(|p| p.local_path.as_ref()).map(|p| p.display().to_string())
                .or_else(|| (manifest.source.scheme == "local").then(|| manifest.source.id.clone())),
            "assets": scored.iter().map(|s| serde_json::json!({
                "name": ui::printable(&s.asset.name),
                "size": s.asset.size,
                "score": s.score.score,
                "reason": ui::printable(&s.score.reason),
                "emulated": s.score.emulated,
            })).collect::<Vec<_>>(),
        }));
    }

    ui::out(&ui::bold(&manifest.name));
    let description = manifest
        .description
        .as_deref()
        .or_else(|| described.as_ref().and_then(|d| d.description.as_deref()));
    if let Some(text) = description {
        // A registry or user manifest's prose: somebody else's text on its way
        // to a terminal.
        ui::out(&crate::changelog::sanitize(text));
    }
    ui::out("");

    let field = |label: &str, value: String| {
        ui::out(&format!(
            "{:<12} {}",
            ui::dim(label),
            crate::changelog::sanitize(&value)
        ))
    };
    field("source", manifest.source.to_string());
    if let Some(url) = source.as_ref().and_then(|s| s.web_url(&manifest.source.id)) {
        field("url", url);
    }
    if let Some(home) = manifest
        .homepage
        .as_deref()
        .or_else(|| described.as_ref().and_then(|d| d.homepage.as_deref()))
    {
        field("homepage", home.to_string());
    }
    if let Some(d) = &described {
        if let Some(stars) = d.stars {
            field("stars", stars.to_string());
        }
        if let Some(license) = &d.license {
            field("license", license.clone());
        }
        if d.archived {
            field(
                "archived",
                "yes — this repository is no longer maintained".into(),
            );
        }
    }
    if let Some(r) = &release {
        field(
            "latest",
            format!("{} ({} assets)", r.version, r.assets.len()),
        );
    }
    match &installed {
        Some(pkg) => {
            field(
                "installed",
                format!(
                    "{}{}",
                    pkg.version,
                    if pkg.pinned { " (pinned)" } else { "" }
                ),
            );
            field("prefix", pkg.prefix.display().to_string());
            field("verified", pkg.publisher_trust().into());
            if let Some(p) = &pkg.provenance {
                let log = p
                    .log_index
                    .map(|i| format!(", Rekor log index {i}"))
                    .unwrap_or_default();
                field(
                    "signature",
                    format!("{} {} by {}{log}", p.signature, p.verifier, p.identity),
                );
            }
            if let Some(kind) = pkg.local_kind {
                field("local kind", kind.to_string());
            }
            if let Some(path) = &pkg.local_path {
                field("local path", path.display().to_string());
            }
            for link in pkg.binaries() {
                field("binary", link.link.display().to_string());
            }
            if pkg.retained.is_empty() {
                field("retained", "none".into());
            } else {
                let versions = pkg
                    .retained
                    .iter()
                    .map(|r| r.version.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                field("retained", versions);
            }
            field(
                "retention",
                format!(
                    "keep {} previous version{}",
                    state.retention.keep,
                    if state.retention.keep == 1 { "" } else { "s" }
                ),
            );
        }
        None => {
            field("installed", "no".into());
            if manifest.source.scheme == "local" {
                field("local path", manifest.source.id.clone());
            }
        }
    }

    if args.assets {
        ui::out("");
        if scored.is_empty() {
            ui::out("no asset in this release can run on this machine");
        } else {
            let rows: Vec<Vec<String>> = scored
                .iter()
                .map(|s| {
                    vec![
                        s.asset.name.clone(),
                        ui::bytes(s.asset.size),
                        s.score.score.to_string(),
                        s.score.reason.clone(),
                    ]
                })
                .collect();
            ui::table(&["asset", "size", "score", "why"], &rows);
        }
    }
    Ok(())
}

/// Print what changed in a release.
///
/// The file the package ships is preferred over the notes the release
/// published: it needs no network, and it is the history of the version
/// actually on disk. A file with no section for that version is not an answer
/// about it, though — plenty of projects cut a release before the heading is
/// written — so that falls through to the notes, and back to the whole file
/// only when there are none to fall through to.
pub fn changelog(cfg: &Config, args: ChangelogArgs) -> Result<()> {
    let state = State::load(cfg)?;
    let spec = PackageSpec::parse(&args.package);
    let installed = installed_for_spec(&state, &spec, &args.package);
    // Only the installed version has a file; any other release is the source's
    // to answer for. `--file` always reads the payload on disk when installed.
    let elsewhere = !args.file && (args.latest || matches!(spec.version, VersionSpec::Exact(_)));

    let mut whole_file = None;
    if !args.release {
        if let Some(pkg) = installed.as_ref().filter(|_| !elsewhere) {
            let version = match &spec.version {
                VersionSpec::Exact(v) => v.clone(),
                VersionSpec::Latest => pkg.version.to_string(),
            };
            match changelog::find_file(&pkg.prefix) {
                Some(path) => {
                    let entry = changelog::from_file(&path, Some(&version))?;
                    if entry.heading.is_some() || args.file {
                        return show(&entry, &pkg.name, &version);
                    }
                    ui::debug(&format!("{} has no entry for {version}", path.display()));
                    whole_file = Some((entry, pkg.name.clone(), version));
                }
                None => ui::debug(&format!("no changelog under {}", pkg.prefix.display())),
            }
        }
    }
    if args.file {
        return Err(Error::msg(match &installed {
            Some(pkg) => format!(
                "{} ships no changelog file; `ketch changelog {} --release` reads the published notes",
                pkg.name, pkg.name
            ),
            None => format!("{} is not installed, so there is no file to read", args.package),
        }));
    }

    match published_notes(cfg, &spec, installed, args.latest) {
        Ok((name, version, entry)) => show(&entry, &name, &version),
        Err(e) => match whole_file {
            Some((entry, name, version)) => {
                ui::warn(&e.to_string());
                show(&entry, &name, &version)
            }
            None => Err(e),
        },
    }
}

/// The notes the source published for the release being asked about.
fn published_notes(
    cfg: &Config,
    spec: &PackageSpec,
    installed: Option<InstalledPackage>,
    latest: bool,
) -> Result<(String, String, Entry)> {
    let manifest = match Resolver::new(cfg)?.resolve(spec) {
        Ok((m, _)) => m,
        Err(e) => match &installed {
            Some(pkg) => pkg
                .manifest
                .clone()
                .unwrap_or_else(|| Manifest::inferred(pkg.source.clone())),
            None => return Err(e),
        },
    };
    // Without `--latest` or an explicit version, the notes wanted are the ones
    // for the release that is installed, not whatever is newest.
    let want = match &spec.version {
        VersionSpec::Exact(v) => VersionSpec::Exact(v.clone()),
        VersionSpec::Latest if latest => VersionSpec::Latest,
        VersionSpec::Latest => installed
            .map(|pkg| VersionSpec::Exact(pkg.tag))
            .unwrap_or(VersionSpec::Latest),
    };

    let sources = SourceRegistry::load(cfg);
    let source = sources.for_ref(&manifest.source)?;
    let opts = ListOpts {
        include_prerelease: cfg.prerelease || manifest.prerelease,
        ..Default::default()
    };
    let release = source.resolve(&manifest.source.id, &want, &opts)?;
    let version = release.version.to_string();
    changelog::from_release(release.notes.as_deref())
        .map(|entry| (manifest.name.clone(), version.clone(), entry))
        .ok_or_else(|| {
            Error::msg(format!(
                "{} {version} published no release notes",
                manifest.name
            ))
        })
}

/// The changelog itself goes to stdout; where it came from goes to stderr, so
/// `ketch changelog rg > NOTES.md` leaves nothing but the markdown.
fn show(entry: &Entry, name: &str, version: &str) -> Result<()> {
    match &entry.origin {
        Origin::File(path) => {
            ui::step(
                "changelog",
                &format!("{name} {version} · {}", path.display()),
            );
            // Saying nothing here would pass a whole file off as one release.
            if entry.heading.is_none() {
                ui::warn(&format!(
                    "no entry for {version} in {}; showing the whole file",
                    path.display()
                ));
            }
        }
        Origin::Release => ui::step("changelog", &format!("{name} {version} · release notes")),
    }
    if let Some(heading) = &entry.heading {
        ui::out(&ui::bold(heading));
        ui::out("");
    }
    if entry.body.is_empty() {
        ui::out(&ui::dim("(nothing recorded)"));
    } else {
        ui::out(&entry.body);
    }
    Ok(())
}

pub fn search(cfg: &Config, args: SearchArgs) -> Result<()> {
    let query = args.query.join(" ");
    let query = query.trim();
    if query.is_empty() {
        return Err(Error::msg("nothing to search for"));
    }

    // Curated manifests first: they install with better names and known
    // binaries, so they are the answer whenever one matches.
    let resolver = Resolver::new(cfg)?;
    let known = resolver.search(query);
    if !known.is_empty() {
        ui::out(&ui::bold("known packages"));
        let rows: Vec<Vec<String>> = known
            .iter()
            .take(args.limit)
            .map(|m| {
                vec![
                    m.name.clone(),
                    m.source.to_string(),
                    ui::truncate(m.description.as_deref().unwrap_or(""), 60),
                ]
            })
            .collect();
        ui::table(&["package", "source", "description"], &rows);
        ui::out("");
    }

    let rest = args.limit.saturating_sub(known.len().min(args.limit));
    if rest == 0 {
        return Ok(());
    }

    let sources = SourceRegistry::load(cfg);
    let mut rows = Vec::new();
    let mut unreachable = 0usize;
    for source in sources.all() {
        let hits = match source.search(query, rest) {
            Ok(h) => h,
            Err(e) => {
                ui::warn(&format!("{}: {e}", source.scheme()));
                unreachable += 1;
                continue;
            }
        };
        for hit in hits {
            rows.push(vec![
                format!("{}:{}", source.scheme(), hit.id),
                hit.stars.map(|s| s.to_string()).unwrap_or_default(),
                ui::truncate(hit.description.as_deref().unwrap_or(""), 60),
            ]);
        }
    }
    rows.truncate(rest);

    if rows.is_empty() {
        if known.is_empty() {
            // Saying "no results" after every source failed would be a claim
            // about the world made from no evidence at all — the mistake
            // `outdated` and `upgrade` already refuse to make.
            if unreachable > 0 {
                return Err(Error::msg(format!(
                    "no source could be searched for `{query}` ({unreachable} failed); \
                     nothing was found, and nothing was ruled out either"
                )));
            }
            ui::out(&format!("no results for `{query}`"));
        }
        return Ok(());
    }
    ui::out(&ui::bold("repositories"));
    ui::table(&["package", "stars", "description"], &rows);
    Ok(())
}

/// Formats a manifest origin for display.
///
/// # Examples
///
/// ```
/// let origin = ManifestOrigin::Builtin;
/// assert_eq!(describe_origin(&origin), "the built-in registry");
/// ```
fn describe_origin(origin: &ManifestOrigin) -> String {
    match origin {
        ManifestOrigin::Builtin => "the built-in registry".to_string(),
        ManifestOrigin::Registry(path) => path.display().to_string(),
        ManifestOrigin::User(path) => path.display().to_string(),
        ManifestOrigin::Inferred => "inference".to_string(),
    }
}

/// Displays recorded package history, either for one package or for the entire tree.
///
/// Results are ordered from newest to oldest and can be rendered as JSON or a
/// human-readable table. Reports when no history exists for the requested scope.
///
/// # Examples
///
/// ```text
/// ketch history ripgrep --limit 10
/// ```
pub fn history(cfg: &Config, args: HistoryArgs) -> Result<()> {
    let events = stats::history(cfg, args.package.as_deref(), i64::from(args.limit))?;

    if args.json {
        return print_json(&events);
    }
    if events.is_empty() {
        // LIMIT 0 is a deliberate "show me nothing". An empty result there does
        // not mean the database is empty.
        if args.limit == 0 {
            return Ok(());
        }
        // Distinguish "this package has no history" from "nothing does": the
        // first is a typo often enough to be worth saying out loud.
        match &args.package {
            Some(name) => ui::out(&format!("no history recorded for {name}")),
            None => ui::out("no history recorded yet"),
        }
        return Ok(());
    }

    let rows: Vec<Vec<String>> = events
        .iter()
        .map(|e| {
            vec![
                crate::log::timestamp(e.at),
                e.package.clone(),
                e.action.clone(),
                match &e.previous_version {
                    Some(from) => format!("{from} → {}", e.version),
                    None => e.version.clone(),
                },
            ]
        })
        .collect();
    ui::table(&["when", "package", "action", "version"], &rows);
    Ok(())
}

/// Displays aggregate statistics for recorded package events.
///
/// # Examples
///
/// ```no_run
/// # use crate::{Config, StatsArgs};
/// # let cfg = todo!();
/// let args = StatsArgs {
///     json: true,
///     ..Default::default()
/// };
/// crate::cmd::query::stats(&cfg, args).unwrap();
/// ```
pub fn stats(cfg: &Config, args: StatsArgs) -> Result<()> {
    let s = stats::summary(cfg)?;

    if args.json {
        return print_json(&serde_json::json!({
            "events": s.events,
            "installs": s.installs,
            "upgrades": s.upgrades,
            "uninstalls": s.uninstalls,
            "packages": s.packages,
            "mean_duration_ms": s.mean_duration_ms(),
            "first_at": s.first_at,
            "last_at": s.last_at,
        }));
    }

    if s.events == 0 {
        ui::out("no statistics recorded yet");
        return Ok(());
    }

    let mut rows = vec![
        vec!["events".to_string(), s.events.to_string()],
        vec!["packages".to_string(), s.packages.to_string()],
        vec!["installs".to_string(), s.installs.to_string()],
        vec!["upgrades".to_string(), s.upgrades.to_string()],
        vec!["uninstalls".to_string(), s.uninstalls.to_string()],
    ];
    if let Some(mean) = s.mean_duration_ms() {
        rows.push(vec![
            "mean install".to_string(),
            format!("{:.1}s", mean as f64 / 1000.0),
        ]);
    }
    if let Some(first) = s.first_at {
        rows.push(vec!["first".to_string(), crate::log::timestamp(first)]);
    }
    if let Some(last) = s.last_at {
        rows.push(vec!["latest".to_string(), crate::log::timestamp(last)]);
    }
    ui::table(&["statistic", "value"], &rows);
    Ok(())
}

/// `pkg@version` is not a state key. Look up by alias or source ref so
/// `--file` still finds the payload on disk.
fn installed_for_spec(state: &State, spec: &PackageSpec, raw: &str) -> Option<InstalledPackage> {
    if let Some(pkg) = state.find(raw) {
        return Some(pkg.clone());
    }
    if let Some(alias) = &spec.alias {
        if let Some(pkg) = state.find(alias) {
            return Some(pkg.clone());
        }
    }
    if let Some(reference) = &spec.reference {
        if let Some(pkg) = state.find(&reference.to_string()) {
            return Some(pkg.clone());
        }
        if let Some(pkg) = state.find(&reference.id) {
            return Some(pkg.clone());
        }
    }
    None
}

/// Explain how a package would be resolved, without installing it.
///
/// Uses the same resolver install does. Prints the trace even when no
/// candidate exists, then fails so scripts can tell a missing asset from a
/// successful explanation.
pub fn why(cfg: &Config, args: WhyArgs) -> Result<()> {
    let spec = PackageSpec::parse(&args.package);
    let sources = SourceRegistry::load(cfg);
    let trace = crate::resolve::explain(cfg, &sources, &spec)?;
    if args.json {
        print_json(&trace)?;
    } else {
        print_why(&trace);
    }
    match (&trace.version.selected, &trace.candidate) {
        (_, Some(_)) => Ok(()),
        (None, None) => Err(Error::NoRelease(spec.label())),
        (Some(release), None) => Err(Error::NoCompatibleAsset {
            id: trace.manifest.source.clone(),
            tag: release.tag.clone(),
            target: trace.target.clone(),
        }),
    }
}

fn print_why(trace: &crate::resolve::ResolutionTrace) {
    let field = |label: &str, value: &str| {
        ui::out(&format!("{:<12} {}", ui::dim(label), value));
    };
    field("package", &trace.package);
    field("target", &trace.target);
    field(
        "manifest",
        &format!("{}  {}", trace.manifest.tier, trace.manifest.origin),
    );
    if trace.manifest.matched != trace.manifest.name {
        field("matched", &trace.manifest.matched);
    }
    field(
        "source",
        &format!("{}:{}", trace.source.scheme, trace.source.id),
    );
    let version = match &trace.version.selected {
        Some(sel) => format!(
            "{}  {} ({}){}",
            trace.version.request,
            sel.tag,
            sel.version,
            if sel.prerelease { "  prerelease" } else { "" }
        ),
        None => format!("{}  (no release)", trace.version.request),
    };
    field("version", &version);
    field(
        "prerelease",
        if trace.version.include_prerelease {
            "included"
        } else {
            "excluded"
        },
    );

    if !trace.assets.scored.is_empty() {
        ui::out("");
        ui::out(&ui::bold("assets"));
        let rows: Vec<Vec<String>> = trace
            .assets
            .scored
            .iter()
            .map(|a| {
                vec![
                    a.score.to_string(),
                    a.name.clone(),
                    a.reason.clone(),
                    if a.emulated {
                        "emulated".into()
                    } else {
                        String::new()
                    },
                ]
            })
            .collect();
        ui::table(&["score", "name", "reason", ""], &rows);
    }
    if !trace.assets.rejected.is_empty() {
        ui::out("");
        ui::out(&ui::bold("rejected"));
        let rows: Vec<Vec<String>> = trace
            .assets
            .rejected
            .iter()
            .map(|a| vec![a.name.clone(), a.reason.clone()])
            .collect();
        ui::table(&["name", "reason"], &rows);
    }

    ui::out("");
    field(
        "checksum",
        &format!(
            "{}{}",
            trace.checksum.policy,
            if trace.checksum.require {
                "  require"
            } else {
                ""
            }
        ),
    );
    field(
        "trust",
        &format!(
            "advisory  strip_quarantine={}  allow_emulation={}",
            trace.trust.strip_quarantine, trace.trust.allow_emulation
        ),
    );
    match &trace.candidate {
        Some(c) => field("candidate", &format!("{}  {}", c.name, c.reason)),
        None => field("candidate", "(none)"),
    }
}

fn json_prose(value: Option<String>) -> Option<String> {
    value.map(|text| ui::printable(&text))
}

/// Serializes a value as pretty-printed JSON and writes it to standard output.
fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| Error::parse("json output".to_string(), e.to_string()))?;
    ui::out(&text);
    Ok(())
}

fn outdated_status(checked: usize, failed: usize) -> &'static str {
    if failed == 0 {
        "ok"
    } else if checked == 0 {
        "fail"
    } else {
        "partial"
    }
}

/// JSON object for `outdated --json`.
///
/// `outdated` is the packages with a newer release. `failed` lists each
/// source that could not be checked. `unreachable` is that count, so a
/// machine reader does not treat an empty package list as "everything current".
fn outdated_report(
    checked: usize,
    outdated: &[serde_json::Value],
    failed: &[serde_json::Value],
) -> serde_json::Value {
    serde_json::json!({
        "status": outdated_status(checked, failed.len()),
        "outdated": outdated,
        "failed": failed,
        "unreachable": failed.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outdated_json_names_the_worst_status_and_each_failure() {
        let outdated = vec![serde_json::json!({
            "name": "ripgrep",
            "installed": "14.0.0",
            "latest": "14.1.0",
            "tag": "v14.1.0",
            "pinned": false,
        })];
        let failed = vec![serde_json::json!({
            "name": "ghost",
            "error": "no releases",
        })];
        let report = outdated_report(1, &outdated, &failed);
        assert_eq!(report["status"], "partial");
        assert_eq!(report["unreachable"], 1);
        assert_eq!(report["outdated"][0]["name"], "ripgrep");
        assert_eq!(report["failed"][0]["name"], "ghost");
        assert_eq!(report["failed"][0]["error"], "no releases");
    }

    #[test]
    fn outdated_json_marks_a_total_failure() {
        let report = outdated_report(
            0,
            &[],
            &[serde_json::json!({
                "name": "ghost",
                "error": "offline",
            })],
        );
        assert_ne!(report, serde_json::json!([]));
        assert!(report.is_object());
        assert_eq!(report["status"], "fail");
        assert_eq!(report["unreachable"], 1);
        assert!(report["outdated"].as_array().expect("outdated").is_empty());
    }
    #[test]
    fn json_prose_strips_bidi_overrides() {
        assert_eq!(
            json_prose(Some("safe\u{202e}evil".to_string())),
            Some("safeevil".to_string())
        );
    }
}
