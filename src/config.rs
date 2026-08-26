//! Runtime configuration: where things live and what we are allowed to do.
//!
//! Precedence, lowest to highest: built-in defaults, `config.toml` in the ketch
//! root, environment variables, command-line flags.

use crate::error::{Error, Result};
use crate::model::TargetSpec;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The upstream repository ketch updates itself from.
pub const SELF_REPO: &str = "listepo/ketch";
/// The package registry ketch resolves names against: a GitHub repository
/// with one folder per package. See `registry.rs`.
pub const REGISTRY_REPO: &str = "listepo/ketch-registry";
pub const USER_AGENT: &str = concat!("ketch/", env!("CARGO_PKG_VERSION"));

/// On-disk settings. Every field optional so a partial file is valid.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub root: Option<PathBuf>,
    pub apps_dir: Option<PathBuf>,
    pub github_token: Option<String>,
    pub prerelease: Option<bool>,
    /// Allow installing x86_64 assets on Apple Silicon (via Rosetta).
    pub allow_emulation: Option<bool>,
    /// Symlink `.app` bundles instead of copying them.
    pub link_apps: Option<bool>,
    /// Refuse to install when the release publishes no checksum.
    pub require_checksums: Option<bool>,
    /// Remove the quarantine flag from code that passes signature checks.
    pub strip_quarantine: Option<bool>,
    pub self_repo: Option<String>,
    /// `owner/repo` of the package registry.
    pub registry: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub root: PathBuf,
    pub bin_dir: PathBuf,
    pub store_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub manifest_dir: PathBuf,
    pub plugin_dir: PathBuf,
    pub state_file: PathBuf,
    pub lock_file: PathBuf,
    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
    pub config_file: PathBuf,
    pub apps_dir: PathBuf,
    pub github_token: Option<String>,
    pub prerelease: bool,
    pub allow_emulation: bool,
    pub link_apps: bool,
    pub require_checksums: bool,
    pub strip_quarantine: bool,
    pub self_repo: String,
    pub registry: String,
    pub registry_dir: PathBuf,
    pub target: TargetSpec,
}

impl Config {
    /// Build the effective config. `root_override` comes from `--root`.
    pub fn load(root_override: Option<PathBuf>) -> Result<Self> {
        let root = root_override
            .or_else(|| std::env::var_os("KETCH_ROOT").map(PathBuf::from))
            .map(|p| expand_tilde(&p))
            .map(|p| absolute_path(&p))
            .transpose()?
            .unwrap_or_else(default_root);

        let config_file = root.join("config.toml");
        let file: ConfigFile = if config_file.is_file() {
            let text =
                std::fs::read_to_string(&config_file).map_err(|e| Error::io(&config_file, e))?;
            toml::from_str(&text)
                .map_err(|e| Error::parse(config_file.display().to_string(), e.to_string()))?
        } else {
            ConfigFile::default()
        };

        // Environment over file, as every other setting here resolves: the file
        // is the standing preference, the variable is this run's override.
        let apps_dir = std::env::var_os("KETCH_APPS_DIR")
            .map(|v| expand_tilde(Path::new(&v)))
            .or_else(|| file.apps_dir.map(|p| expand_tilde(&p)))
            .unwrap_or_else(|| PathBuf::from("/Applications"));

        // A relative apps dir would resolve against whatever directory the
        // user happened to run ketch from, and install somewhere different
        // every time.
        if !apps_dir.is_absolute() {
            return Err(Error::Config(format!(
                "apps_dir must be an absolute path, not `{}`",
                apps_dir.display()
            )));
        }

        // The file lives inside the root, so it cannot choose it. Saying so is
        // better than honouring the key nowhere and explaining it nowhere.
        if file.root.is_some() {
            crate::ui::warn(&format!(
                "`root` in {} has no effect; set KETCH_ROOT or pass --root",
                config_file.display()
            ));
        }

        let self_repo = validate_repo(
            "self_repo",
            std::env::var("KETCH_SELF_REPO")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or(file.self_repo)
                .unwrap_or_else(|| SELF_REPO.to_string()),
        )?;
        let registry = validate_repo(
            "registry",
            std::env::var("KETCH_REGISTRY")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .or(file.registry)
                .unwrap_or_else(|| REGISTRY_REPO.to_string()),
        )?;

        let github_token = std::env::var("KETCH_GITHUB_TOKEN")
            .ok()
            .or_else(|| std::env::var("GITHUB_TOKEN").ok())
            .or_else(|| std::env::var("GH_TOKEN").ok())
            .or(file.github_token)
            .filter(|t| !t.trim().is_empty());

        Ok(Config {
            bin_dir: root.join("bin"),
            store_dir: root.join("store"),
            cache_dir: root.join("cache"),
            manifest_dir: root.join("manifests"),
            plugin_dir: root.join("plugins"),
            state_file: root.join("state.json"),
            lock_file: root.join(".lock"),
            config_file,
            apps_dir,
            github_token,
            prerelease: env_bool("KETCH_PRERELEASE")
                .or(file.prerelease)
                .unwrap_or(false),
            allow_emulation: env_bool("KETCH_ALLOW_EMULATION")
                .or(file.allow_emulation)
                .unwrap_or(true),
            link_apps: env_bool("KETCH_LINK_APPS")
                .or(file.link_apps)
                .unwrap_or(false),
            require_checksums: env_bool("KETCH_REQUIRE_CHECKSUMS")
                .or(file.require_checksums)
                .unwrap_or(false),
            strip_quarantine: env_bool("KETCH_STRIP_QUARANTINE")
                .or(file.strip_quarantine)
                .unwrap_or(true),
            self_repo,
            registry,
            // Deliberately not in `ensure_dirs`: the directory existing is how
            // ketch knows the registry has been fetched.
            registry_dir: root.join("registry"),
            target: TargetSpec::host(),
            root,
        })
    }

    /// Create the directory layout. Safe to call repeatedly.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.bin_dir,
            &self.store_dir,
            &self.cache_dir,
            &self.manifest_dir,
            &self.plugin_dir,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        Ok(())
    }

    /// Where a specific version of a package is unpacked.
    pub fn package_dir(&self, name: &str, version: &str) -> PathBuf {
        self.store_dir.join(name).join(sanitize_component(version))
    }

    /// True when the bin dir is on the caller's PATH.
    pub fn bin_dir_on_path(&self) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        std::env::split_paths(&path).any(|p| p == self.bin_dir)
    }
}

fn default_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ketch")
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .map_err(|e| Error::io(Path::new("."), e))
}

fn expand_tilde(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

fn env_bool(key: &str) -> Option<bool> {
    match std::env::var(key)
        .ok()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// Accept only `owner/repo`, since it is about to become a URL.
///
/// A `github:` prefix is tolerated because that is how the same repository is
/// written everywhere else in ketch; the stored form drops it.
pub fn validate_repo(what: &str, raw: String) -> Result<String> {
    let repo = raw.trim().trim_start_matches("github:");
    let mut parts = repo.split('/');
    let shaped = matches!((parts.next(), parts.next(), parts.next()), (Some(o), Some(r), None)
        if !o.is_empty() && !r.is_empty());
    let printable = repo
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
    if shaped && printable && !repo.contains("..") {
        Ok(repo.to_string())
    } else {
        Err(Error::Config(format!(
            "{what} `{raw}` is not a GitHub repository; expected `owner/repo`"
        )))
    }
}

/// Make a string safe to use as one path component. Version tags can legally
/// contain `/` (e.g. `release/1.2`), which would otherwise escape the store.
pub fn sanitize_component(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '\0' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim_matches(['.', ' ', '-']).to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_owner_repo_is_accepted_as_a_repository() {
        let want = "listepo/ketch-registry";
        assert_eq!(validate_repo("registry", want.into()).unwrap(), want);
        assert_eq!(
            validate_repo("registry", "github:listepo/ketch-registry".into()).unwrap(),
            want
        );
        for bad in [
            "",
            "listepo",
            "a/b/c",
            "../etc",
            "a/../b",
            "o/r?x=1",
            "http://x/y",
        ] {
            assert!(
                validate_repo("registry", bad.into()).is_err(),
                "{bad} must be rejected"
            );
        }
    }

    #[test]
    fn sanitizes_path_components() {
        assert_eq!(sanitize_component("v1.2.3"), "v1.2.3");
        assert_eq!(sanitize_component("release/1.2"), "release-1.2");
        assert_eq!(sanitize_component("../../etc"), "etc");
        assert_eq!(sanitize_component(".."), "unknown");
        assert_eq!(sanitize_component(""), "unknown");
    }

    #[test]
    fn resolves_relative_roots_against_the_current_directory() {
        let root = absolute_path(Path::new("scratch")).unwrap();
        assert!(root.is_absolute());
        assert_eq!(root, std::env::current_dir().unwrap().join("scratch"));
    }
}
