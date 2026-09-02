//! Read-only commands.
//!
//! These never take the lock and never write. Their data output goes to stdout
//! through `ui::out`/`ui::table` so it can be piped while progress and warnings
//! stay on stderr.

use crate::changelog::{self, Entry, Origin};
use crate::cli::{ChangelogArgs, InfoArgs, ListArgs, OutdatedArgs, SearchArgs};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::install;
use crate::manifest::Resolver;
use crate::model::{InstalledPackage, Manifest, ManifestOrigin, PackageSpec, Release, VersionSpec};
use crate::source::{ListOpts, SourceRegistry};
use crate::state::State;
use crate::ui;

pub fn list(cfg: &Config, args: ListArgs) -> Result<()> {
    let state = State::load(cfg)?;
    let packages: Vec<&InstalledPackage> = state.iter().collect();

    if args.json {
        return print_json(&packages);
    }
    if args.names_only {
        for pkg in &packages {
            ui::out(&pkg.name);
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
            vec![
                pkg.name.clone(),
                format!(
                    "{}{}",
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

pub fn outdated(cfg: &Config, args: OutdatedArgs) -> Result<()> {
    let state = State::load(cfg)?;
    let sources = SourceRegistry::load(cfg);
    let prerelease = args.prerelease || cfg.prerelease;

    let mut rows = Vec::new();
    let mut json = Vec::new();
    let (mut checked, mut unreachable) = (0usize, 0usize);
    for pkg in state.iter() {
        ui::step("checking", &pkg.name);
        let release = match install::latest_release(&sources, pkg, prerelease) {
            Ok(r) => r,
            // Reporting is best-effort: one unreachable source must not hide
            // the rest of the answer.
            Err(e) => {
                ui::warn(&format!("{}: {e}", pkg.name));
                unreachable += 1;
                continue;
            }
        };
        checked += 1;
        if release.version <= pkg.version {
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
        json.push(serde_json::json!({
            "name": pkg.name,
            "installed": pkg.version.to_string(),
            "latest": release.version.to_string(),
            "tag": release.tag,
            "pinned": pkg.pinned,
        }));
    }

    // An empty answer because nothing could be reached is not the same answer
    // as an empty answer because nothing is out of date, and `[]` on stdout
    // with an exit status of 0 says the second either way.
    if checked == 0 && unreachable > 0 {
        return Err(Error::msg(format!(
            "could not check any of the {unreachable} packages; see the warnings above"
        )));
    }
    if args.json {
        return print_json(&json);
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
    let source = sources.for_ref(&manifest.source)?;
    let described = source.describe(&manifest.source.id).unwrap_or_else(|e| {
        ui::debug(&format!("describe failed: {e}"));
        None
    });
    let opts = ListOpts {
        include_prerelease: cfg.prerelease || manifest.prerelease,
        ..Default::default()
    };
    let release: Option<Release> =
        match source.resolve(&manifest.source.id, &VersionSpec::Latest, &opts) {
            Ok(r) => Some(r),
            Err(e) => {
                ui::warn(&format!("{}: {e}", manifest.name));
                None
            }
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
            "name": manifest.name,
            "source": manifest.source.to_string(),
            "url": source.web_url(&manifest.source.id),
            "description": manifest.description.clone().or_else(|| described.as_ref().and_then(|d| d.description.clone())),
            "homepage": manifest.homepage.clone().or_else(|| described.as_ref().and_then(|d| d.homepage.clone())),
            "stars": described.as_ref().and_then(|d| d.stars),
            "license": described.as_ref().and_then(|d| d.license.clone()),
            "archived": described.as_ref().map(|d| d.archived).unwrap_or(false),
            "latest": release.as_ref().map(|r| r.version.to_string()),
            "latest_tag": release.as_ref().map(|r| r.tag.clone()),
            "installed": installed.as_ref().map(|p| p.version.to_string()),
            "pinned": installed.as_ref().map(|p| p.pinned).unwrap_or(false),
            "assets": scored.iter().map(|s| serde_json::json!({
                "name": s.asset.name,
                "size": s.asset.size,
                "score": s.score.score,
                "reason": s.score.reason,
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
        ui::out(text);
    }
    ui::out("");

    let field = |label: &str, value: String| ui::out(&format!("{:<12} {value}", ui::dim(label)));
    field("source", manifest.source.to_string());
    if let Some(url) = source.web_url(&manifest.source.id) {
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
            for link in pkg.binaries() {
                field("binary", link.link.display().to_string());
            }
        }
        None => field("installed", "no".into()),
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
    let installed = state.find(&args.package).cloned();
    // Only the installed version has a file; any other release is the source's
    // to answer for.
    let elsewhere = args.latest || matches!(spec.version, VersionSpec::Exact(_));

    let mut whole_file = None;
    if !args.release {
        if let Some(pkg) = installed.as_ref().filter(|_| !elsewhere) {
            let version = pkg.version.to_string();
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

    let sources = SourceRegistry::load(cfg);
    let mut rows = Vec::new();
    for source in sources.all() {
        let hits = match source.search(query, args.limit) {
            Ok(h) => h,
            Err(e) => {
                ui::warn(&format!("{}: {e}", source.scheme()));
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
    rows.truncate(args.limit);

    if rows.is_empty() {
        if known.is_empty() {
            ui::out(&format!("no results for `{query}`"));
        }
        return Ok(());
    }
    ui::out(&ui::bold("repositories"));
    ui::table(&["package", "stars", "description"], &rows);
    Ok(())
}

fn describe_origin(origin: &ManifestOrigin) -> String {
    match origin {
        ManifestOrigin::Builtin => "the built-in registry".to_string(),
        ManifestOrigin::Registry(path) => path.display().to_string(),
        ManifestOrigin::User(path) => path.display().to_string(),
        ManifestOrigin::Inferred => "inference".to_string(),
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| Error::parse("json output".to_string(), e.to_string()))?;
    ui::out(&text);
    Ok(())
}
