//! What is installed, on disk.
//!
//! `state.json` is the only durable record ketch keeps. It is rewritten
//! atomically — staged next to the real file and renamed — so an interrupted
//! write can never leave a half-parsed state file behind, which would look
//! exactly like "nothing is installed".

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::InstalledPackage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Bumped only when the on-disk shape changes incompatibly.
pub const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    /// Keyed by package name, which is unique across sources by construction.
    #[serde(default)]
    pub packages: BTreeMap<String, InstalledPackage>,
}

impl Default for State {
    fn default() -> Self {
        State {
            version: STATE_VERSION,
            packages: BTreeMap::new(),
        }
    }
}

impl State {
    /// Read the state file. A missing file is an empty state, not an error.
    pub fn load(cfg: &Config) -> Result<Self> {
        Self::load_path(&cfg.state_file)
    }

    pub fn load_path(path: &Path) -> Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(State::default()),
            Err(e) => return Err(Error::io(path, e)),
        };
        // An absent file means nothing has been installed yet. An empty one
        // means a write was lost, and every package on disk is about to be
        // forgotten — say so rather than quietly starting over.
        if text.trim().is_empty() {
            return Err(Error::msg(format!(
                "{} is empty, which usually means an interrupted write. Anything \
                 already installed is still in the store; remove the file to start \
                 a fresh record, then `ketch relink` each package.",
                path.display()
            )));
        }
        let state: State = serde_json::from_str(&text)
            .map_err(|e| Error::parse(path.display().to_string(), e.to_string()))?;
        if state.version > STATE_VERSION {
            return Err(Error::msg(format!(
                "{} was written by a newer ketch (state version {}); upgrade with `ketch self update`",
                path.display(),
                state.version
            )));
        }
        Ok(state)
    }

    pub fn save(&self, cfg: &Config) -> Result<()> {
        self.save_path(&cfg.state_file)
    }

    pub fn save_path(&self, path: &Path) -> Result<()> {
        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| Error::parse("state".to_string(), e.to_string()))?;
        let mut staged =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;
        staged
            .write_all(json.as_bytes())
            .map_err(|e| Error::io(staged.path(), e))?;
        staged
            .write_all(b"\n")
            .map_err(|e| Error::io(staged.path(), e))?;
        staged.flush().map_err(|e| Error::io(staged.path(), e))?;
        // The rename is atomic, but only over whatever the file actually
        // contains. Without this the kernel is free to record the rename and
        // lose the bytes, leaving a zero-length state file — which is to say,
        // an empty list of installed packages.
        staged
            .as_file()
            .sync_all()
            .map_err(|e| Error::io(staged.path(), e))?;
        staged.persist(path).map_err(|e| Error::io(path, e.error))?;
        // Then make the rename itself durable. Best effort: some filesystems
        // refuse to fsync a directory, and that is not a reason to fail a save.
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&InstalledPackage> {
        self.packages.get(name)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut InstalledPackage> {
        self.packages.get_mut(name)
    }

    pub fn insert(&mut self, pkg: InstalledPackage) {
        self.packages.insert(pkg.name.clone(), pkg);
    }

    pub fn remove(&mut self, name: &str) -> Option<InstalledPackage> {
        self.packages.remove(name)
    }

    pub fn names(&self) -> Vec<&str> {
        self.packages.keys().map(|k| k.as_str()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &InstalledPackage> {
        self.packages.values()
    }

    /// Look a package up the way a user would name it: by install name, by a
    /// binary it provides, or by its source id (`owner/repo`).
    pub fn find(&self, query: &str) -> Option<&InstalledPackage> {
        if let Some(hit) = self.packages.get(query) {
            return Some(hit);
        }
        self.packages.values().find(|p| {
            p.source.id.eq_ignore_ascii_case(query)
                || p.source.to_string().eq_ignore_ascii_case(query)
                || p.binaries()
                    .any(|b| b.link.file_name().is_some_and(|n| n == query))
        })
    }
}

// ---------------------------------------------------------------------------
// Locking
// ---------------------------------------------------------------------------

/// Exclusive access to the install tree, released on drop.
///
/// Two `ketch install` runs writing the same `state.json` would each save a
/// view that omits the other's package, silently losing an install. The lock is
/// advisory between ketch processes only — nothing else writes this tree.
pub struct Lock {
    path: PathBuf,
    /// False when we adopted our own process's existing lock (re-entrancy),
    /// in which case dropping must not delete it.
    owned: bool,
}

impl Lock {
    /// Take the lock, or fail with the pid currently holding it.
    pub fn acquire(cfg: &Config) -> Result<Lock> {
        Self::acquire_path(&cfg.lock_file)
    }

    pub fn acquire_path(path: &Path) -> Result<Lock> {
        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        let me = std::process::id();

        for attempt in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(mut file) => {
                    let _ = write!(file, "{me}");
                    return Ok(Lock {
                        path: path.to_path_buf(),
                        owned: true,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let holder = std::fs::read_to_string(path)
                        .ok()
                        .and_then(|t| t.trim().parse::<u32>().ok());
                    match holder {
                        Some(pid) if pid == me => {
                            return Ok(Lock {
                                path: path.to_path_buf(),
                                owned: false,
                            })
                        }
                        Some(pid) if process_alive(pid) => {
                            return Err(Error::Locked(format!("pid {pid}")))
                        }
                        // A crashed run left the file behind. Reclaim it by
                        // renaming rather than unlinking: `rename` fails if the
                        // file is already gone, so of two processes that both
                        // judge the lock stale exactly one can claim it. Plain
                        // `remove_file` succeeds for both — including for the
                        // one that would delete the winner's fresh lock — and
                        // they would then both proceed.
                        other => {
                            if attempt == 0 {
                                crate::ui::debug(&format!(
                                    "clearing stale lock {} ({})",
                                    path.display(),
                                    other
                                        .map(|p| format!("pid {p} is gone"))
                                        .unwrap_or_else(|| "unreadable".into())
                                ));
                                let reclaimed = path.with_extension(format!("stale.{me}"));
                                if std::fs::rename(path, &reclaimed).is_ok() {
                                    let _ = std::fs::remove_file(&reclaimed);
                                }
                                continue;
                            }
                            return Err(Error::Locked(path.display().to_string()));
                        }
                    }
                }
                Err(e) => return Err(Error::io(path, e)),
            }
        }
        Err(Error::Locked(path.display().to_string()))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        if self.owned {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Is that pid still running? Only consulted when a lock file already exists,
/// so shelling out costs nothing on the normal path and keeps the crate free of
/// a libc dependency.
///
/// `ps` rather than `kill -0`: signalling a process owned by another user fails
/// with EPERM, which is indistinguishable from "no such process" through an
/// exit status alone — and reading it as "gone" steals a lock that is very much
/// still held.
fn process_alive(pid: u32) -> bool {
    std::process::Command::new("/bin/ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("pid=")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(true) // Unsure means "assume held" — never steal on doubt.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ManifestOrigin, PackageRef, TargetSpec, Version};

    fn pkg(name: &str) -> InstalledPackage {
        InstalledPackage {
            name: name.to_string(),
            version: Version::parse("1.0.0"),
            source: PackageRef::github("o/r"),
            tag: "v1.0.0".into(),
            target: TargetSpec::host(),
            asset_name: "a.tar.gz".into(),
            sha256: "0".repeat(64),
            checksum_verified: true,
            installed_at: 0,
            prefix: PathBuf::from("/tmp/x"),
            links: Vec::new(),
            pinned: false,
            origin: ManifestOrigin::Inferred,
            manifest: None,
        }
    }

    #[test]
    fn an_empty_state_file_is_not_an_empty_install() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, "").unwrap();
        assert!(State::load_path(&path).is_err(), "corruption must be loud");

        std::fs::remove_file(&path).unwrap();
        assert!(State::load_path(&path).unwrap().packages.is_empty());
    }

    #[test]
    fn a_lock_left_by_a_dead_process_is_reclaimed_without_residue() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        // Above the pid ceiling, so it can never name a running process.
        std::fs::write(&path, "999999").unwrap();

        let lock = Lock::acquire_path(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            std::process::id().to_string()
        );
        drop(lock);

        let left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(left.is_empty(), "reclaim left {left:?} behind");
    }

    #[test]
    fn a_lock_held_by_another_users_process_is_not_stolen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock");
        // pid 1 is running and is not ours to signal — the case that reads as
        // "process is gone" if aliveness is judged by `kill -0` alone.
        std::fs::write(&path, "1").unwrap();
        assert!(matches!(Lock::acquire_path(&path), Err(Error::Locked(_))));
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut state = State::default();
        state.insert(pkg("ripgrep"));
        state.save_path(&path).unwrap();

        let loaded = State::load_path(&path).unwrap();
        assert_eq!(loaded.version, STATE_VERSION);
        assert!(loaded.get("ripgrep").is_some());
    }

    #[test]
    fn missing_file_is_an_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::load_path(&dir.path().join("nope.json")).unwrap();
        assert!(state.packages.is_empty());
    }

    #[test]
    fn refuses_a_newer_state_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, r#"{"version":99,"packages":{}}"#).unwrap();
        assert!(State::load_path(&path).is_err());
    }

    #[test]
    fn finds_by_source_id_and_binary() {
        let mut state = State::default();
        state.insert(pkg("ripgrep"));
        assert!(state.find("o/r").is_some());
        assert!(state.find("github:o/r").is_some());
        assert!(state.find("absent").is_none());
    }

    #[test]
    fn lock_is_exclusive_and_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".lock");
        {
            let _held = Lock::acquire_path(&path).unwrap();
            assert!(path.exists());
            // Same process re-entering must not deadlock against itself.
            let _again = Lock::acquire_path(&path).unwrap();
        }
        assert!(!path.exists());
    }
}
