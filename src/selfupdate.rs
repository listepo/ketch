//! Updating ketch with ketch.
//!
//! Deliberately stricter than a normal install: the running binary is the thing
//! that verifies every other download, so it is replaced only against a
//! published checksum — never on trust-on-first-use — and the previous binary is
//! kept until the new one has proven it can run.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::{AssetSelector, Version, VersionSpec};
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

/// The version this binary was built as.
pub fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION"))
}

/// Where the running binary lives, with symlinks resolved so we replace the
/// real file rather than the link pointing at it.
pub fn current_exe() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    Ok(std::fs::canonicalize(&exe).unwrap_or(exe))
}

/// Fetch the latest ketch release and replace this binary.
pub fn update(cfg: &Config, force: bool, dry_run: bool) -> Result<SelfUpdate> {
    let _lock = Lock::acquire(cfg)?;
    let from = current_version();

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

/// Remove ketch itself. With `purge`, also removes the store, cache and state.
pub fn uninstall_self(cfg: &Config, purge: bool) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();

    if purge {
        // Uninstall properly rather than deleting the root: links and copied
        // app bundles live outside it and would otherwise be left dangling.
        let lock = Lock::acquire(cfg)?;
        let mut state = State::load(cfg)?;
        let names: Vec<String> = state.names().into_iter().map(|n| n.to_string()).collect();
        for name in names {
            match install::uninstall(cfg, &mut state, &name) {
                Ok(pkg) => removed.push(pkg.prefix),
                Err(e) => ui::warn(&format!("{name}: {e}")),
            }
        }
        // Save first: if removing the tree fails, state still matches reality.
        state.save(cfg)?;
        drop(lock);

        if cfg.root.is_dir() {
            std::fs::remove_dir_all(&cfg.root).map_err(|e| Error::io(&cfg.root, e))?;
            removed.push(cfg.root.clone());
        }
    }

    let exe = current_exe()?;
    // Under --purge the binary may already have gone with the root.
    if exe.exists() {
        std::fs::remove_file(&exe).map_err(|e| Error::io(&exe, e))?;
        removed.push(exe);
    }
    Ok(removed)
}
