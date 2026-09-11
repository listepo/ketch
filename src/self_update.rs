//! Installing and updating ketch with ketch.
//!
//! ketch prefers to be one of its own packages: `self install` puts the running
//! release into the store under the name `ketch` and links it from the bin dir,
//! exactly as `ketch install listepo/ketch` would, so `list`, `history` and
//! `doctor` see it and `self update` is an ordinary upgrade. A ketch copied flat
//! into the bin dir by an older installer is still updated in place.
//!
//! Either way this is deliberately stricter than a normal install: the running
//! binary is the thing that verifies every other download, so it is replaced
//! only against a published checksum — never on trust-on-first-use — and, in
//! place, the previous binary is kept until the new one has proven it can run.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::install::{InstallRequest, Installed};
use crate::model::{AssetSelector, PackageSpec, Version, VersionSpec};
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
pub fn install_self(cfg: &Config, force: bool) -> Result<Installed> {
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
    let flat = cfg.bin_dir.join(SELF_NAME);
    let aside = std::fs::symlink_metadata(&flat)
        .is_ok_and(|m| m.is_file())
        .then(|| flat.with_extension("old"));
    if let Some(aside) = &aside {
        std::fs::rename(&flat, aside).map_err(|e| Error::io(&flat, e))?;
    }
    let result = install::install(cfg, &sources, &mut state, &req).and_then(|out| {
        state.save(cfg)?;
        Ok(out)
    });
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

/// Where the running binary lives, with symlinks resolved so we replace the
/// real file rather than the link pointing at it.
pub fn current_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(std::fs::canonicalize(&exe).unwrap_or(exe))
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
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755)) {
            return Err(restore(Error::io(exe, e)));
        }
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
    /// The Homebrew cask's own directory, when ketch came from `brew`.
    pub cask: Option<PathBuf>,
    /// The running binary, when it lives inside the root and so goes with it.
    pub exe: Option<PathBuf>,
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
        cask: (!no_brew).then(cask_dir).flatten(),
        exe: current_exe().ok().filter(|exe| exe.starts_with(&cfg.root)),
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
fn brew_prefixes() -> Vec<PathBuf> {
    match std::env::var_os("HOMEBREW_PREFIX") {
        Some(prefix) => vec![PathBuf::from(prefix)],
        None => vec![PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")],
    }
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

#[cfg(test)]
mod tests {
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
}
