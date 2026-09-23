//! Installing and updating ketch with ketch.
//!
//! ketch prefers to be one of its own packages: `self install` puts the running
//! release into the store under the name `ketch` and links it from the bin dir,
//! exactly as `ketch install listepo/ketch` would, so `list`, `history` and
//! `doctor` see it and `self upgrade` is an ordinary upgrade. A ketch copied flat
//! into the bin dir by an older installer is still updated in place.
//!
//! Either way this is deliberately stricter than a normal install: the running
//! binary is the thing that verifies every other download, so it is replaced
//! only against a published checksum — never on trust-on-first-use — and, in
//! place, the previous binary is kept until the new one has proven it can run.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::install::{InstallRequest, Installed};
use crate::model::{
    AssetSelector, CompletionShell, LinkKind, LinkRecord, LinkRole, PackageSpec, Version,
    VersionSpec,
};
use crate::source::{ListOpts, SourceRegistry};
use crate::state::{Lock, State};
use crate::{install, ui};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Outcome of a self-update attempt.
#[derive(Debug, Clone)]
pub struct SelfUpdate {
    pub from: Version,
    pub to: Version,
    /// False when already current, or when `dry_run` was set.
    pub replaced: bool,
    pub notes: Option<String>,
}

/// The package name ketch records itself under.
pub const SELF_NAME: &str = "ketch";

/// The version this binary was built as.
/// Channel label shown next to the package version in user-facing output.
pub const VERSION_CHANNEL: &str = "preview";

/// Package version as clap / `--version` / `self version` print it.
pub fn display_version() -> String {
    format!("{} · {}", env!("CARGO_PKG_VERSION"), VERSION_CHANNEL)
}

pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION"))
}

/// Install the running release of ketch as a package.
///
/// The binary is fetched again from the release rather than copied from
/// wherever this process happens to run: a copy would be whatever the installer
/// downloaded, and this path is the one that verifies it against the published
/// checksum. Returns `Error::AlreadyInstalled` when this version already is the
/// package and `force` is off, like any other install.
pub fn install_self(cfg: &Config, force: bool, link_dir: Option<&Path>) -> Result<Installed> {
    let _lock = Lock::acquire(cfg)?;
    let mut state = State::load(cfg)?;
    // Built-in sources only, as in `update`.
    let sources = SourceRegistry::builtin_only(cfg);
    let mut req = InstallRequest::new(PackageSpec::parse(&format!(
        "{}@v{}",
        cfg.self_repo,
        current_version()
    )));
    req.force = force;
    req.require_checksum = true;

    // A ketch copied flat into the bin dir is where the link now has to go,
    // and the platform refuses to replace a file ketch did not put there. It
    // is moved aside rather than deleted so that a failed install still leaves
    // a ketch on PATH; the running image survives either way.
    let flat_name = if cfg!(windows) {
        "ketch.exe"
    } else {
        SELF_NAME
    };
    let flat = cfg.bin_dir.join(flat_name);
    let aside = std::fs::symlink_metadata(&flat)
        .is_ok_and(|m| m.is_file())
        .then(|| flat.with_extension("old"));
    if let Some(aside) = &aside {
        std::fs::rename(&flat, aside).map_err(|e| Error::io(&flat, e))?;
    }
    let result = match install::install(cfg, &sources, &mut state, &req) {
        Ok(out) => (|| {
            if let Some(dir) = link_dir {
                record_bootstrap_link(cfg, &mut state, dir)?;
            }
            expose_self_docs(cfg, &mut state)?;
            state.save(cfg)?;
            Ok(out)
        })(),
        Err(Error::AlreadyInstalled { name, version }) => {
            if let Some(dir) = link_dir {
                record_bootstrap_link(cfg, &mut state, dir)?;
            }
            if let Err(e) = expose_self_docs(cfg, &mut state) {
                ui::warn(&format!("could not install man page and completions: {e}"));
            } else if let Err(e) = state.save(cfg) {
                ui::warn(&format!("could not record man page and completions: {e}"));
            }
            Err(Error::AlreadyInstalled { name, version })
        }
        Err(e) => Err(e),
    };
    if let Some(aside) = aside {
        if result.is_ok() {
            let _ = std::fs::remove_file(&aside);
        } else if let Err(e) = std::fs::rename(&aside, &flat) {
            ui::warn(&format!(
                "could not put {} back ({e}); move it to {} by hand",
                aside.display(),
                flat.display()
            ));
        }
    }
    result
}

/// The bootstrap binary name on this platform.
fn bootstrap_binary_name() -> &'static str {
    if cfg!(windows) {
        "ketch.exe"
    } else {
        SELF_NAME
    }
}

/// Whether a link record is the install.sh bootstrap outside the bin dir.
fn is_bootstrap_record(record: &LinkRecord, bin_dir: &Path) -> bool {
    let target = bin_dir.join(bootstrap_binary_name());
    record.target == target
        && record
            .link
            .file_name()
            .is_some_and(|n| n == bootstrap_binary_name())
        && record
            .link
            .parent()
            .is_some_and(|parent| canonical_dir(parent).ok() != Some(bin_dir.to_path_buf()))
}

/// Resolve a directory the way install.sh does before comparing paths.
fn canonical_dir(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
        }
        std::fs::create_dir_all(path).map_err(|e| Error::io(path, e))?;
    }
    dunce::canonicalize(path).map_err(|e| Error::io(path, e))
}

fn remove_any(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Record a bootstrap link or copy outside `<root>/bin`, for install.sh.
///
/// The link follows `<root>/bin/ketch` so `self upgrade` keeps the bootstrap
/// path current. Uninstall removes it through the package's link records.
fn record_bootstrap_link(cfg: &Config, state: &mut State, link_dir: &Path) -> Result<()> {
    let platform = crate::platform::host()?;
    let bin_dir = canonical_dir(&cfg.bin_dir)?;
    let link_dir = canonical_dir(link_dir)?;
    let target = cfg.bin_dir.join(bootstrap_binary_name());

    let old: Vec<LinkRecord> = state
        .get(SELF_NAME)
        .map(|pkg| {
            pkg.links
                .iter()
                .filter(|record| is_bootstrap_record(record, &bin_dir))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if !old.is_empty() {
        platform.unplace(&old)?;
    }

    if link_dir == bin_dir {
        if let Some(entry) = state.get_mut(SELF_NAME) {
            entry
                .links
                .retain(|record| !is_bootstrap_record(record, &bin_dir));
        }
        return Ok(());
    }

    if !target.is_file() {
        return Err(Error::msg(format!(
            "{} is missing after install",
            target.display()
        )));
    }

    let link = link_dir.join(bootstrap_binary_name());
    let recorded = state
        .get(SELF_NAME)
        .map(|pkg| pkg.links.as_slice())
        .unwrap_or(&[]);
    clear_bootstrap_destination(&link, recorded)?;
    let record = create_bootstrap_link(&link, &target)?;

    let Some(entry) = state.get_mut(SELF_NAME) else {
        return Err(Error::msg("ketch package not installed"));
    };
    entry
        .links
        .retain(|record| !is_bootstrap_record(record, &bin_dir));
    entry.links.push(record);
    Ok(())
}

fn clear_bootstrap_destination(link: &Path, recorded: &[LinkRecord]) -> Result<()> {
    match std::fs::symlink_metadata(link) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) if recorded.iter().any(|record| record.link == link) => {
            remove_any(link).map_err(|e| Error::io(link, e))
        }
        Ok(_) => Err(Error::msg(format!(
            "{} already exists and was not installed by ketch for this package; move it aside first",
            link.display()
        ))),
        Err(e) => Err(Error::io(link, e)),
    }
}

fn create_bootstrap_link(link: &Path, target: &Path) -> Result<LinkRecord> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).map_err(|e| Error::io(link, e))?;
    #[cfg(windows)]
    std::fs::copy(target, link).map_err(|e| Error::io(link, e))?;
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target);
        Err(Error::msg("bootstrap links are not supported on this OS"))
    }
    #[cfg(any(unix, windows))]
    Ok(LinkRecord {
        link: link.to_path_buf(),
        target: target.to_path_buf(),
        kind: {
            #[cfg(unix)]
            {
                LinkKind::Symlink
            }
            #[cfg(windows)]
            {
                LinkKind::CopiedFile
            }
        },
        role: LinkRole::Binary,
    })
}

/// Generate ketch's man page and completions into the store prefix and link
/// them into the user directories `doctor` reports.
fn expose_self_docs(_cfg: &Config, state: &mut State) -> Result<()> {
    let Some(pkg) = state.get(SELF_NAME).cloned() else {
        return Ok(());
    };
    let platform = crate::platform::host()?;
    let extras = crate::extra::write_ketch_docs(&pkg.prefix)?;
    let planned = crate::extra::plan(&extras, &platform.user_man_root(), |shell| {
        platform.completion_dir(shell)
    })?;
    let owned = pkg.prefix.parent().unwrap_or(&pkg.prefix);
    let extra_links = crate::platform::expose_extras(&planned, &pkg.prefix, owned, &pkg.links)?;
    let stale: Vec<LinkRecord> = pkg
        .links
        .iter()
        .filter(|record| {
            !record.role.is_binary() && !extra_links.iter().any(|fresh| fresh.link == record.link)
        })
        .cloned()
        .collect();
    if !stale.is_empty() {
        platform.unplace(&stale)?;
    }
    if let Some(entry) = state.get_mut(SELF_NAME) {
        entry.links.retain(|record| {
            record.role.is_binary() || extra_links.iter().any(|fresh| fresh.link == record.link)
        });
        for link in extra_links {
            if !entry
                .links
                .iter()
                .any(|existing| existing.link == link.link)
            {
                entry.links.push(link);
            }
        }
    }
    Ok(())
}

/// Install one shell's completion script the same way `self install` does.
pub fn install_completion_script(cfg: &Config, shell: clap_complete::Shell) -> Result<()> {
    let Some(want) = CompletionShell::from_clap(shell) else {
        return Err(Error::msg(format!(
            "{shell} completions cannot be installed into a user directory"
        )));
    };
    let mut state = State::load(cfg)?;
    if state.get(SELF_NAME).is_none() {
        return Err(Error::msg(
            "ketch is not installed as a package; run `ketch self install` first",
        ));
    }
    expose_self_docs(cfg, &mut state)?;
    state.save(cfg)?;
    let platform = crate::platform::host()?;
    let dest = platform.completion_dir(want);
    crate::ui::success(
        "installed",
        &format!("{} completions in {}", want.as_str(), dest.display()),
    );
    Ok(())
}

/// Where the running binary lives, with symlinks resolved so we replace the
/// real file rather than the link pointing at it.
pub fn current_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(dunce::canonicalize(&exe).unwrap_or(exe))
}

/// Fetch the latest ketch release and install it: as an upgrade of the `ketch`
/// package when there is one, otherwise by replacing this binary in place.
pub fn update(cfg: &Config, force: bool, dry_run: bool) -> Result<SelfUpdate> {
    let _lock = Lock::acquire(cfg)?;
    let mut state = State::load(cfg)?;
    // When ketch is a package, the package is what gets updated, so its
    // version is the one that counts — not this binary's, which a Homebrew
    // upgrade or a fresh install.sh may already have moved ahead of the store.
    let installed = state.get(SELF_NAME).map(|p| p.version.clone());
    let from = installed.clone().unwrap_or_else(current_version);

    // With no package, the upgrade would rewrite this binary in place. mise
    // names the version directory after the release it unpacked, so that
    // would leave `mise ls` and `mise upgrade` believing in a version that is
    // no longer the one on disk. Refused before any network call, dry run
    // included, so the two never disagree about what would happen.
    if installed.is_none() {
        if let Some(tool) = mise_tool_dir() {
            return Err(Error::msg(format!(
                "this ketch is managed by mise ({}): upgrade it with `mise upgrade`, \
                 or run `ketch self install` to let ketch manage itself",
                tool.display()
            )));
        }
    }

    // Built-in sources only: a third-party plugin must never be in a position
    // to hand ketch its own replacement.
    let sources = SourceRegistry::builtin_only(cfg);
    let source = sources.get("github")?;
    ui::step("checking", &cfg.self_repo);
    let release = source.resolve(&cfg.self_repo, &VersionSpec::Latest, &ListOpts::default())?;
    let to = release.version.clone();

    if to <= from && !force {
        return Ok(SelfUpdate {
            from,
            to,
            replaced: false,
            notes: None,
        });
    }
    if dry_run {
        return Ok(SelfUpdate {
            from,
            to,
            replaced: false,
            notes: release.notes.clone(),
        });
    }

    if installed.is_some() {
        let sources = SourceRegistry::builtin_only(cfg);
        let mut req = InstallRequest::new(PackageSpec::parse(&format!(
            "{}@{}",
            cfg.self_repo, release.tag
        )));
        req.force = force;
        req.require_checksum = true;
        install::install(cfg, &sources, &mut state, &req)?;
        expose_self_docs(cfg, &mut state)?;
        state.save(cfg)?;
        return Ok(SelfUpdate {
            from,
            to,
            replaced: true,
            notes: release.notes,
        });
    }

    let platform = crate::platform::host()?;
    let selector = AssetSelector::default();
    let chosen = install::score_assets(cfg, platform.as_ref(), &release, &selector)
        .into_iter()
        .next()
        .ok_or_else(|| Error::NoCompatibleAsset {
            id: cfg.self_repo.clone(),
            tag: release.tag.clone(),
            target: cfg.target.to_string(),
        })?;

    std::fs::create_dir_all(&cfg.cache_dir).map_err(|e| Error::io(&cfg.cache_dir, e))?;
    let work = tempfile::tempdir_in(&cfg.cache_dir).map_err(|e| Error::io(&cfg.cache_dir, e))?;
    // The asset name is the release author's string, not ketch's. It reaches a
    // path here, so it goes through the same guard every other asset name does.
    let download = work
        .path()
        .join(crate::config::sanitize_component(&chosen.asset.name));
    let progress = ui::progress();
    let sha256 = source.download(&chosen.asset, &download, progress.as_ref())?;

    // `require` is hard-coded: for its own binary ketch does not accept the
    // trust-on-first-use path it allows for packages.
    install::verify_checksum(
        source.as_ref(),
        &cfg.self_repo,
        &release,
        &chosen.asset,
        &sha256,
        true,
    )?;

    let unpacked = work.path().join("payload");
    crate::extract::extract_auto(&download, &unpacked, &platform.extractors())?;
    let fresh = find_binary(&unpacked)?;

    let exe = current_exe()?;
    replace_binary(&exe, &fresh)?;
    Ok(SelfUpdate {
        from,
        to,
        replaced: true,
        notes: release.notes,
    })
}

/// Swap `fresh` into `exe`, keeping the old binary until the new one has shown
/// it can run. A ketch that cannot start is a ketch that cannot fix itself.
fn replace_binary(exe: &Path, fresh: &Path) -> Result<()> {
    let backup = exe.with_file_name(format!(
        "{}.old",
        exe.file_name().and_then(|n| n.to_str()).unwrap_or("ketch")
    ));
    // Rename rather than overwrite: the running image stays valid, and a failed
    // copy leaves something to put back.
    std::fs::rename(exe, &backup).map_err(|e| Error::io(exe, e))?;

    let restore = |detail: Error| -> Error {
        let _ = std::fs::remove_file(exe);
        match std::fs::rename(&backup, exe) {
            Ok(()) => detail,
            Err(e) => Error::msg(format!(
                "{detail}; could not restore the previous binary ({e}): move {} back to {} by hand",
                backup.display(),
                exe.display()
            )),
        }
    };

    // Copy, not rename: the download lives in the cache dir, which may be on a
    // different filesystem.
    if let Err(e) = std::fs::copy(fresh, exe) {
        return Err(restore(Error::io(exe, e)));
    }
    #[cfg(unix)]
    if let Err(e) = crate::platform::unix::ensure_executable(exe) {
        return Err(restore(e));
    }

    match Command::new(exe).arg("--version").output() {
        Ok(out) if out.status.success() => {
            let _ = std::fs::remove_file(&backup);
            Ok(())
        }
        Ok(out) => Err(restore(Error::Command {
            cmd: format!("{} --version", exe.display()),
            status: out.status.to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })),
        Err(e) => Err(restore(Error::io(exe, e))),
    }
}

/// The one file in an unpacked ketch release that is ketch.
fn find_binary(payload: &Path) -> Result<PathBuf> {
    let wanted = if cfg!(windows) { "ketch.exe" } else { "ketch" };
    walkdir::WalkDir::new(payload)
        .follow_links(false)
        .into_iter()
        .flatten()
        .find(|e| e.file_type().is_file() && e.file_name() == wanted)
        .map(|e| e.into_path())
        .ok_or_else(|| Error::EmptyPayload(payload.to_path_buf()))
}

/// What `uninstall_self` will remove, worked out before anything is touched.
///
/// The plan exists so the question can name what is about to be lost. "remove
/// ketch?" and "remove ketch, these four packages, the block in your .zshrc and
/// the Homebrew cask?" are different questions, and only the second one can be
/// answered honestly.
#[derive(Debug, Default)]
pub struct UninstallPlan {
    /// Packages to uninstall properly, ketch itself included.
    pub packages: Vec<String>,
    /// The ketch root, when there is one to take apart.
    pub root: Option<PathBuf>,
    /// Shell startup files holding a ketch PATH block.
    pub shell_files: Vec<PathBuf>,
    /// The Windows user PATH names the bin dir.
    pub user_path: bool,
    /// The Homebrew cask's own directory, when ketch came from `brew`.
    pub cask: Option<PathBuf>,
    /// The running binary, when it lives inside the root and so goes with it.
    pub exe: Option<PathBuf>,
    /// The mise install the running binary came from. Removed only when the
    /// caller leaves it set, which the command does after asking separately.
    pub mise: Option<MiseInstall>,
}

/// A ketch that `mise use` installed: where it lives, and the tool name mise
/// knows it by, which is what `mise unuse` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiseInstall {
    pub dir: PathBuf,
    pub tool: String,
}

/// Work out what removing ketch would take, without removing any of it.
pub fn uninstall_plan(cfg: &Config, keep_packages: bool, no_brew: bool) -> Result<UninstallPlan> {
    let state = State::load(cfg)?;
    let packages: Vec<String> = if keep_packages {
        state
            .get(SELF_NAME)
            .map(|p| p.name.clone())
            .into_iter()
            .collect()
    } else {
        state.names().into_iter().map(|n| n.to_string()).collect()
    };
    Ok(UninstallPlan {
        packages,
        // With `--keep-packages` the tree stays: the other packages live in it.
        root: (!keep_packages && cfg.root.is_dir()).then(|| cfg.root.clone()),
        shell_files: if keep_packages {
            Vec::new()
        } else {
            crate::shell::files_with_block()
        },
        user_path: !keep_packages && crate::shell::user_path_configured(cfg),
        cask: (!no_brew).then(cask_dir).flatten(),
        exe: current_exe().ok().filter(|exe| {
            let root = dunce::canonicalize(&cfg.root).unwrap_or_else(|_| cfg.root.clone());
            let exe = dunce::canonicalize(exe).unwrap_or_else(|_| exe.clone());
            // Windows Path::starts_with is case-sensitive; a root typed with
            // different ASCII case than the running image must still count.
            crate::platform::path_is_within(&exe, &root)
        }),
        mise: mise_tool_dir().map(|dir| MiseInstall {
            tool: mise_tool_name(&dir, &cfg.self_repo),
            dir,
        }),
    })
}

/// Carry out `plan`, removing what it names. Returns the paths that are gone.
///
/// Best effort past the first package: someone who has said yes to this wants
/// ketch gone, and stopping halfway would leave a tree they now have to take
/// apart by hand. Every failure is warned about instead.
pub fn uninstall_self(cfg: &Config, plan: &UninstallPlan) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();

    // Uninstall properly rather than deleting files: links and copied app
    // bundles live outside the root and would otherwise be left dangling.
    let lock = Lock::acquire(cfg)?;
    let mut state = State::load(cfg)?;
    for name in &plan.packages {
        match install::uninstall(cfg, &mut state, name) {
            Ok(pkg) => removed.push(pkg.prefix),
            Err(e) => ui::warn(&format!("{name}: {e}")),
        }
    }
    // Save first: if removing the tree fails, state still matches reality.
    state.save(cfg)?;
    drop(lock);

    if let Some(root) = &plan.root {
        removed.extend(remove_root(cfg, root));
    }

    // After the root, so the nested `ketch self uninstall` the cask runs on its
    // way out finds no binary and gives up harmlessly instead of recursing.
    if let Some(cask) = &plan.cask {
        if remove_cask(cask) {
            removed.push(cask.clone());
        }
    }

    for file in &plan.shell_files {
        match crate::shell::uninstall_file(file) {
            Ok(true) => removed.push(file.clone()),
            Ok(false) => {}
            Err(e) => ui::warn(&format!("{}: {e}", file.display())),
        }
    }

    #[cfg(windows)]
    if plan.user_path {
        match crate::shell::uninstall_user(cfg, false) {
            Ok(_) => {}
            Err(e) => ui::warn(&format!("user PATH: {e}")),
        }
    }

    match &plan.exe {
        // Usually already gone with the store prefix or the bin dir; a ketch
        // that was copied in flat by an older installer is not.
        Some(exe) if exe.exists() => {
            std::fs::remove_file(exe).map_err(|e| Error::io(exe, e))?;
            removed.push(exe.clone());
        }
        Some(_) => {}
        // A ketch outside the root is not ketch's to delete: a `cargo run`
        // build, or a copy someone put on PATH themselves.
        // Nor is a mise install, but it has an owner that can take it away:
        // below when the user agreed, and the command says how when not.
        None if mise_tool_dir().is_some() => {}
        None => {
            if let Ok(exe) = current_exe() {
                ui::note(&format!(
                    "{} is outside {} and was kept",
                    exe.display(),
                    cfg.root.display()
                ));
            }
        }
    }

    // Last: mise deletes the very binary running this, and on Windows that
    // fails outright while it runs, so everything else is done by then.
    if let Some(mise) = &plan.mise {
        #[cfg(windows)]
        move_out_of(&mise.dir);
        if unuse_mise(&mise.tool) {
            removed.push(mise.dir.clone());
        }
    }
    Ok(removed)
}

/// Take the root apart by naming what ketch owns, then removing the directory
/// itself only if nothing else is left in it.
///
/// Never `remove_dir_all(root)`: `install.sh --install-dir ~/bin` makes the
/// root the parent of that directory, which is the user's home in the worst
/// case. When the root *is* the home directory, the named children (`bin`,
/// `store`, `cache`, …) are not emptied either — they are shared with the
/// rest of the account. A dedicated root like `~/.ketch` is still wiped.
fn remove_root(cfg: &Config, root: &Path) -> Vec<PathBuf> {
    remove_root_at(cfg, root, dirs::home_dir().as_deref())
}

fn remove_root_at(cfg: &Config, root: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let wipe = home.is_none_or(|h| cfg.root != h);
    let mut removed = Vec::new();
    let dirs = [
        &cfg.bin_dir,
        &cfg.store_dir,
        &cfg.cache_dir,
        &cfg.manifest_dir,
        &cfg.plugin_dir,
        &cfg.registry_dir,
    ];
    let files = [
        &cfg.state_file,
        &cfg.stats_db,
        &cfg.config_file,
        &cfg.lock_file,
        &cfg.registry_meta,
    ];
    for dir in dirs {
        remove_owned_dir(dir, wipe, &mut removed);
    }
    // The log directory holds the file being written to as this runs, so it
    // goes whole and last among the directories.
    if let Some(logs) = cfg.log_file.parent() {
        remove_owned_dir(logs, wipe, &mut removed);
    }
    for file in files {
        if !wipe {
            continue;
        }
        match std::fs::remove_file(file) {
            Ok(()) => removed.push(file.clone()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => ui::warn(&format!("{}: {e}", file.display())),
        }
    }
    if wipe {
        match std::fs::remove_dir(root) {
            Ok(()) => removed.push(root.to_path_buf()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // Anything left is something ketch did not write. Say so and leave it.
            Err(_) => ui::note(&format!(
                "{} was left in place: it holds files ketch did not put there",
                root.display()
            )),
        }
    }
    removed
}

fn remove_owned_dir(dir: &Path, wipe: bool, removed: &mut Vec<PathBuf>) {
    let result = if wipe {
        std::fs::remove_dir_all(dir)
    } else {
        std::fs::remove_dir(dir)
    };
    match result {
        Ok(()) => removed.push(dir.to_path_buf()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) if wipe => ui::warn(&format!("{}: {e}", dir.display())),
        Err(_) => ui::note(&format!(
            "{} was left in place: it holds files ketch did not put there",
            dir.display()
        )),
    }
}

/// The Homebrew cask's own directory, when ketch was installed with `brew`.
///
/// Homebrew records a cask under `<prefix>/Caskroom/<token>`, so its presence
/// is the question "did brew install this?" answered without running anything.
pub(crate) fn cask_dir() -> Option<PathBuf> {
    cask_dir_in(&brew_prefixes())
}

fn cask_dir_in(prefixes: &[PathBuf]) -> Option<PathBuf> {
    prefixes
        .iter()
        .map(|prefix| prefix.join("Caskroom").join(SELF_NAME))
        .find(|dir| dir.is_dir())
}

/// Where Homebrew might live: its own answer first, then the two standard
/// prefixes for Apple Silicon and Intel.
///
/// `HOMEBREW_PREFIX` alone is not enough: a leftover Intel cask under
/// `/usr/local` must still be found when the active brew is Apple Silicon
/// (and the reverse). Skipping the standards left `self uninstall` and
/// `doctor`'s leftover-cask check blind to the other prefix.
fn brew_prefixes() -> Vec<PathBuf> {
    let mut prefixes = Vec::new();
    if let Some(prefix) = std::env::var_os("HOMEBREW_PREFIX") {
        let prefix = PathBuf::from(prefix);
        if !prefix.as_os_str().is_empty() {
            prefixes.push(prefix);
        }
    }
    for candidate in ["/opt/homebrew", "/usr/local"] {
        let candidate = PathBuf::from(candidate);
        if !prefixes.contains(&candidate) {
            prefixes.push(candidate);
        }
    }
    prefixes
}

/// The `brew` that owns `cask`: the one in the prefix the cask was found under,
/// since a machine can carry both an Apple Silicon and an Intel Homebrew, and
/// only one of them knows about this cask. `PATH` is the last resort.
fn brew_binary(cask: &Path) -> PathBuf {
    cask.parent()
        .and_then(|caskroom| caskroom.parent())
        .map(|prefix| prefix.join("bin").join("brew"))
        .filter(|brew| brew.is_file())
        .unwrap_or_else(|| PathBuf::from("brew"))
}

/// Hand the cask back to Homebrew, which is the only thing that can forget it.
///
/// Deleting the Caskroom directory would leave `brew` believing ketch is still
/// installed, so this runs the real command and reports rather than guesses.
fn remove_cask(cask: &Path) -> bool {
    let brew = brew_binary(cask);
    ui::step("removing", "the Homebrew cask");
    // Inherited stdio: `brew` prints its own progress, and asking it to be
    // quiet would hide the sudo prompt it may need.
    match Command::new(&brew)
        .args(["uninstall", "--cask", SELF_NAME])
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            ui::warn(&format!(
                "`brew uninstall --cask {SELF_NAME}` failed ({status}); \
                 run it by hand to finish removing the cask"
            ));
            false
        }
        Err(e) => {
            ui::warn(&format!("could not run {}: {e}", brew.display()));
            false
        }
    }
}

/// The mise tool directory (`<data dir>/installs/<tool>`) holding the running
/// binary, when ketch was installed with `mise use`.
///
/// Answered from the path alone, like the cask: a mise install is a directory
/// under mise's data dir, and asking `mise` itself would mean running a
/// program ketch did not install to learn something the path already says.
pub(crate) fn mise_tool_dir() -> Option<PathBuf> {
    mise_tool_dir_in(&current_exe().ok()?, &mise_data_dirs())
}

fn mise_tool_dir_in(exe: &Path, data_dirs: &[PathBuf]) -> Option<PathBuf> {
    data_dirs.iter().find_map(|data| {
        let installs = data.join("installs");
        // The data dir may itself sit behind a symlink; `exe` is canonical.
        let installs = dunce::canonicalize(&installs).unwrap_or(installs);
        if !crate::platform::path_is_strict_within(exe, &installs) {
            return None;
        }
        let depth = installs.components().count() + 1;
        exe.ancestors()
            .find(|dir| dir.components().count() == depth)
            .map(Path::to_path_buf)
    })
}

/// The name `mise unuse` takes for the tool in `dir`.
///
/// mise names an install directory after the tool with `:` and `/` turned
/// into `-`, so `github:listepo/ketch` lives in `github-listepo-ketch`. That
/// is only reversible for a name that ends in this repository; anything else
/// (a registry short name such as `ketch`) is already the tool name.
fn mise_tool_name(dir: &Path, self_repo: &str) -> String {
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(SELF_NAME);
    let backend = name
        .strip_suffix(&self_repo.replace('/', "-"))
        .and_then(|rest| rest.strip_suffix('-'))
        .filter(|b| !b.is_empty() && b.chars().all(|c| c.is_ascii_alphanumeric()));
    match backend {
        Some(backend) => format!("{backend}:{self_repo}"),
        None => name.to_string(),
    }
}

/// Hand the install back to mise, the only thing that can forget it: removing
/// the directory would leave the tool in mise's config, to be reinstalled on
/// the next `mise install`. `-g` because that is how the README installs it.
fn unuse_mise(tool: &str) -> bool {
    ui::step("removing", &format!("{tool} from mise"));
    // Inherited stdio, as for brew: mise prints its own progress. `--yes`
    // because the user has just answered this very question, and mise would
    // otherwise ask it again for every version it prunes.
    match Command::new("mise")
        .args(["--yes", "unuse", "-g", tool])
        .status()
    {
        Ok(status) if status.success() => true,
        Ok(status) => {
            ui::warn(&format!(
                "`mise unuse -g {tool}` failed ({status}); run it by hand to finish"
            ));
            false
        }
        Err(e) => {
            ui::warn(&format!(
                "could not run mise ({e}); run `mise unuse -g {tool}` by hand"
            ));
            false
        }
    }
}

/// Move the running binary out of `dir`, so mise can delete the directory.
///
/// Windows will not delete a directory holding a mapped image, but it will
/// rename the image: the process keeps its handle either way. The temp dir is
/// on the same volume as the profile mise lives in, so this is a rename, not
/// a copy, and the file is left for the system's own temp cleanup — nothing
/// else can delete it while this process is still running.
#[cfg(windows)]
fn move_out_of(dir: &Path) {
    let Ok(exe) = current_exe() else {
        return;
    };
    if !crate::platform::path_is_within(&exe, dir) {
        return;
    }
    let aside = std::env::temp_dir().join(format!("ketch-uninstalled-{}.exe", std::process::id()));
    if let Err(e) = std::fs::rename(&exe, &aside) {
        ui::warn(&format!(
            "could not move {} out of mise's tree ({e}); mise may fail to remove it",
            exe.display()
        ));
    }
}

/// Where mise might keep its installs: its own override first, then the
/// default it picks on this OS.
fn mise_data_dirs() -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Some(dir) = std::env::var_os("MISE_DATA_DIR").filter(|v| !v.is_empty()) {
        found.push(PathBuf::from(dir));
    }
    let default = if cfg!(windows) {
        dirs::data_local_dir().map(|d| d.join("mise"))
    } else {
        // mise follows XDG on macOS too, not Application Support.
        Some(crate::platform::data_home().join("mise"))
    };
    found.extend(default.filter(|d| !found.contains(d)));
    found
}

#[cfg(test)]
mod tests {
    #[test]
    fn display_version_marks_preview_channel() {
        let shown = display_version();
        assert_eq!(
            shown,
            format!("{} · {}", env!("CARGO_PKG_VERSION"), VERSION_CHANNEL)
        );
        assert_eq!(VERSION_CHANNEL, "preview");
    }

    use super::*;

    #[test]
    fn a_caskroom_directory_is_what_says_brew_installed_ketch() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let prefix = tmp.path().join("homebrew");
        // The only test that touches this variable, so nothing races with it.
        std::env::set_var("HOMEBREW_PREFIX", &prefix);

        assert_eq!(cask_dir(), None, "no cask means brew did not install ketch");

        let cask = prefix.join("Caskroom").join(SELF_NAME);
        std::fs::create_dir_all(&cask).expect("create caskroom");
        assert_eq!(cask_dir(), Some(cask));

        std::env::remove_var("HOMEBREW_PREFIX");
    }

    #[test]
    fn a_binary_under_mise_installs_belongs_to_that_tool_directory() {
        let data = PathBuf::from("/home/u/.local/share/mise");
        let exe = data.join("installs/github-listepo-ketch/0.4.7/ketch");
        assert_eq!(
            mise_tool_dir_in(&exe, &[PathBuf::from("/elsewhere"), data.clone()]),
            Some(data.join("installs/github-listepo-ketch"))
        );
    }

    #[test]
    fn a_binary_outside_mise_installs_is_not_mises() {
        let data = PathBuf::from("/home/u/.local/share/mise");
        for exe in [
            "/home/u/.ketch/store/ketch/0.4.7/ketch",
            "/home/u/.local/share/mise/shims/ketch",
            "/home/u/.local/share/mise/installs",
        ] {
            assert_eq!(
                mise_tool_dir_in(Path::new(exe), std::slice::from_ref(&data)),
                None,
                "{exe}"
            );
        }
    }

    #[test]
    fn a_mise_install_directory_maps_back_to_the_tool_name_unuse_takes() {
        for (dir, tool) in [
            ("github-listepo-ketch", "github:listepo/ketch"),
            ("ubi-listepo-ketch", "ubi:listepo/ketch"),
            ("ketch", "ketch"),
            ("-listepo-ketch", "-listepo-ketch"),
            ("github-other-ketch", "github-other-ketch"),
        ] {
            let dir = Path::new("/mise/installs").join(dir);
            assert_eq!(mise_tool_name(&dir, "listepo/ketch"), tool);
        }
    }

    #[test]
    fn both_standard_prefixes_are_tried_when_homebrew_has_not_said_where_it_is() {
        // Not asserted against the environment: the machine running the suite
        // may well have HOMEBREW_PREFIX set, and that is a valid answer too.
        for prefix in [PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")] {
            let cask = prefix.join("Caskroom").join(SELF_NAME);
            let found = cask_dir_in(std::slice::from_ref(&prefix));
            assert_eq!(found, cask.is_dir().then_some(cask));
        }
    }

    #[test]
    fn homebrew_prefix_does_not_hide_a_cask_under_another_prefix() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let active = tmp.path().join("opt-homebrew");
        let other = tmp.path().join("usr-local");
        std::fs::create_dir_all(active.join("bin")).expect("active brew");
        let cask = other.join("Caskroom").join(SELF_NAME);
        std::fs::create_dir_all(&cask).expect("other cask");

        // Active brew answered, but the cask lives under the other tree —
        // the same shape as Apple Silicon HOMEBREW_PREFIX + leftover Intel cask.
        let found = cask_dir_in(&[active, other.clone()]);
        assert_eq!(found, Some(cask));
    }

    #[test]
    fn the_brew_beside_the_cask_is_preferred_to_the_one_on_path() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let prefix = tmp.path().join("homebrew");
        let cask = prefix.join("Caskroom").join(SELF_NAME);
        std::fs::create_dir_all(&cask).expect("create caskroom");

        // Nothing installed there yet, so there is no better answer than PATH.
        assert_eq!(brew_binary(&cask), PathBuf::from("brew"));

        let brew = prefix.join("bin").join("brew");
        std::fs::create_dir_all(brew.parent().expect("bin dir")).expect("create bin");
        std::fs::write(&brew, "#!/bin/sh\n").expect("write brew");
        assert_eq!(brew_binary(&cask), brew);
    }

    #[test]
    fn uninstall_does_not_wipe_bin_when_the_root_is_home() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let home = tmp.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        let cfg = Config::load(Some(home.clone())).expect("config");
        std::fs::create_dir_all(&cfg.bin_dir).expect("bin");
        std::fs::write(cfg.bin_dir.join("keep-me"), b"stay").expect("keep-me");
        std::fs::create_dir_all(&cfg.store_dir).expect("store");
        std::fs::write(cfg.store_dir.join("mine"), b"also").expect("store file");

        remove_root_at(&cfg, &cfg.root, Some(&home));

        assert_eq!(
            std::fs::read(cfg.bin_dir.join("keep-me")).expect("kept bin file"),
            b"stay"
        );
        assert_eq!(
            std::fs::read(cfg.store_dir.join("mine")).expect("kept store file"),
            b"also"
        );
    }

    #[test]
    fn uninstall_wipes_a_dedicated_root() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let home = tmp.path().join("home");
        let root = tmp.path().join(".ketch");
        std::fs::create_dir_all(&home).expect("home");
        let cfg = Config::load(Some(root.clone())).expect("config");
        std::fs::create_dir_all(&cfg.bin_dir).expect("bin");
        std::fs::write(cfg.bin_dir.join("gone"), b"x").expect("bin file");

        remove_root_at(&cfg, &cfg.root, Some(&home));

        assert!(!cfg.bin_dir.exists(), "dedicated bin dir should be gone");
    }

    fn installed_ketch(prefix: PathBuf) -> crate::model::InstalledPackage {
        crate::model::InstalledPackage {
            name: SELF_NAME.into(),
            version: Version::parse("1.0.0"),
            source: crate::model::PackageRef::github("listepo/ketch"),
            tag: "v1.0.0".into(),
            target: crate::model::TargetSpec::host(),
            asset_name: "a.tar.gz".into(),
            sha256: "0".repeat(64),
            checksum_verified: true,
            installed_at: 0,
            prefix,
            links: Vec::new(),
            pinned: false,
            origin: crate::model::ManifestOrigin::Inferred,
            manifest: None,
            local_kind: None,
            local_path: None,
            trust: Default::default(),
            retained: Vec::new(),
            provenance: None,
        }
    }

    #[test]
    fn a_bootstrap_link_dir_is_recorded_and_placed() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(Some(tmp.path().join("root"))).unwrap();
        std::fs::create_dir_all(&cfg.bin_dir).unwrap();
        let bin = cfg.bin_dir.join(bootstrap_binary_name());
        std::fs::write(&bin, b"ketch").unwrap();

        let mut state = State::load(&cfg).unwrap();
        state.insert(installed_ketch(cfg.store_dir.join(SELF_NAME)));
        let bootstrap = tmp.path().join("bootstrap");
        record_bootstrap_link(&cfg, &mut state, &bootstrap).unwrap();

        let link = dunce::canonicalize(&bootstrap)
            .unwrap()
            .join(bootstrap_binary_name());
        #[cfg(unix)]
        {
            assert!(
                link.symlink_metadata().unwrap().file_type().is_symlink(),
                "the bootstrap path must follow the bin-dir binary"
            );
            assert_eq!(
                dunce::canonicalize(std::fs::read_link(&link).unwrap()).unwrap(),
                dunce::canonicalize(&bin).unwrap()
            );
        }
        #[cfg(windows)]
        {
            assert!(link.is_file());
            assert_eq!(std::fs::read(&link).unwrap(), b"ketch");
        }
        let pkg = state.get(SELF_NAME).unwrap();
        assert_eq!(pkg.links.len(), 1);
        assert_eq!(pkg.links[0].link, link);
        assert_eq!(
            dunce::canonicalize(&pkg.links[0].target).unwrap(),
            dunce::canonicalize(&bin).unwrap()
        );
    }

    #[test]
    fn a_link_dir_that_is_the_bin_dir_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(Some(tmp.path().join("root"))).unwrap();
        std::fs::create_dir_all(&cfg.bin_dir).unwrap();
        let bin = cfg.bin_dir.join(bootstrap_binary_name());
        std::fs::write(&bin, b"ketch").unwrap();

        let mut state = State::load(&cfg).unwrap();
        state.insert(installed_ketch(cfg.store_dir.join(SELF_NAME)));
        record_bootstrap_link(&cfg, &mut state, &cfg.bin_dir.clone()).unwrap();

        assert!(
            std::fs::symlink_metadata(&bin)
                .unwrap()
                .file_type()
                .is_file(),
            "the installed binary must still be the binary, not a link to itself"
        );
        assert!(state.get(SELF_NAME).unwrap().links.is_empty());
    }
}
