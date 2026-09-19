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
    /// Refresh the package registry before `install` and `upgrade`.
    pub auto_update: Option<bool>,
    pub self_repo: Option<String>,
    /// `owner/repo` of the package registry.
    pub registry: Option<String>,
    /// How many packages a batch install works on at once.
    pub jobs: Option<usize>,
    /// `off`, `error`, `warn`, `info` or `debug`.
    pub log_level: Option<String>,
    /// `text` or `json`.
    pub log_format: Option<String>,
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
    /// History and statistics. Unlike `state_file`, losing this loses only the
    /// record of what happened — see `stats.rs`.
    pub stats_db: PathBuf,
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
    /// Refresh the package registry before `install` and `upgrade`.
    pub auto_update: bool,
    pub self_repo: String,
    pub registry: String,
    pub registry_dir: PathBuf,
    /// Fetch record for the local registry, beside the package folders.
    pub registry_meta: PathBuf,
    pub target: TargetSpec,
    /// Packages installed at once by a batch install. Never zero.
    pub jobs: usize,
    pub log_file: PathBuf,
    pub log_level: crate::log::Level,
    pub log_format: crate::log::Format,
}

impl Config {
    /// Builds the effective configuration from command-line, environment, file, and default settings.
    ///
    /// The optional `root_override` takes precedence over `KETCH_ROOT` and the configured default root.
    /// Configuration-file and environment values are validated before the resolved configuration is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use ketch::config::Config;
    ///
    /// let config = Config::load(None).unwrap();
    /// assert!(config.root.is_absolute());
    /// ```
    pub fn load(root_override: Option<PathBuf>) -> Result<Self> {
        // A variable that is set but empty means "unset" here, as it does for
        // every other setting below. `KETCH_ROOT=` is what a CI job writes when
        // it clears a variable, and reading it literally makes the working
        // directory the ketch root and fills it with store/, bin/ and cache/.
        let root = root_override
            .or_else(|| {
                std::env::var_os("KETCH_ROOT")
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            })
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
            .filter(|v| !v.is_empty())
            .map(|v| expand_tilde(Path::new(&v)))
            .or_else(|| file.apps_dir.map(|p| expand_tilde(&p)))
            .unwrap_or_else(|| {
                // `/Applications` is absolute on Unix and the macOS convention.
                // On Windows it is a relative path, so Config::load would refuse
                // every run; park unused app bundles under the root instead.
                if cfg!(target_os = "macos") {
                    PathBuf::from("/Applications")
                } else {
                    root.join("apps")
                }
            });

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

        // Each variable is filtered before the next is tried: `KETCH_GITHUB_TOKEN=`
        // is how CI clears a secret without blocking GITHUB_TOKEN or GH_TOKEN.
        let github_token = std::env::var("KETCH_GITHUB_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .or_else(|| {
                std::env::var("GITHUB_TOKEN")
                    .ok()
                    .filter(|t| !t.trim().is_empty())
            })
            .or_else(|| {
                std::env::var("GH_TOKEN")
                    .ok()
                    .filter(|t| !t.trim().is_empty())
            })
            .or(file.github_token)
            .filter(|t| !t.trim().is_empty());

        // A parse failure here is the user's own config or environment, so it
        // is an error rather than a silent fall back to the default.
        let log_level = parsed("KETCH_LOG_LEVEL", "log_level", file.log_level)?.unwrap_or_default();
        let log_format =
            parsed("KETCH_LOG_FORMAT", "log_format", file.log_format)?.unwrap_or_default();
        let jobs = match std::env::var("KETCH_JOBS")
            .ok()
            .filter(|v| !v.trim().is_empty())
        {
            Some(v) => Some(v.trim().parse::<usize>().map_err(|_| {
                Error::Config(format!("KETCH_JOBS must be a whole number, not `{v}`"))
            })?),
            None => file.jobs,
        };

        Ok(Config {
            bin_dir: root.join("bin"),
            store_dir: root.join("store"),
            cache_dir: root.join("cache"),
            manifest_dir: root.join("manifests"),
            plugin_dir: root.join("plugins"),
            state_file: root.join("state.json"),
            stats_db: root.join("stats.db"),
            lock_file: root.join(".lock"),
            config_file,
            apps_dir,
            github_token,
            prerelease: env_bool("KETCH_PRERELEASE")?
                .or(file.prerelease)
                .unwrap_or(false),
            allow_emulation: env_bool("KETCH_ALLOW_EMULATION")?
                .or(file.allow_emulation)
                .unwrap_or(true),
            link_apps: env_bool("KETCH_LINK_APPS")?
                .or(file.link_apps)
                .unwrap_or(false),
            require_checksums: env_bool("KETCH_REQUIRE_CHECKSUMS")?
                .or(file.require_checksums)
                .unwrap_or(false),
            strip_quarantine: env_bool("KETCH_STRIP_QUARANTINE")?
                .or(file.strip_quarantine)
                .unwrap_or(true),
            auto_update: env_bool("KETCH_AUTO_UPDATE")?
                .or(file.auto_update)
                .unwrap_or(true),
            self_repo,
            registry,
            // Deliberately not in `ensure_dirs`: the directory existing is how
            // ketch knows the registry has been fetched.
            registry_dir: root.join("registry"),
            registry_meta: root.join("registry.meta.toml"),
            log_file: root.join("logs").join("ketch.log"),
            log_level,
            log_format,
            // Downloads dominate an install and spend their time waiting, so
            // the useful number is well above the core count. Capped anyway:
            // a hundred parallel requests is how a source starts refusing them.
            jobs: jobs.filter(|n| *n > 0).unwrap_or(4).min(16),
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

    /// The `config.toml` body for the compiled defaults: what
    /// `ketch config reset` writes.
    pub fn default_toml() -> String {
        let file = ConfigFile {
            apps_dir: None,
            github_token: None,
            prerelease: Some(false),
            allow_emulation: Some(true),
            link_apps: Some(false),
            require_checksums: Some(false),
            strip_quarantine: Some(true),
            auto_update: Some(true),
            self_repo: Some(SELF_REPO.to_string()),
            registry: Some(REGISTRY_REPO.to_string()),
            jobs: Some(4),
            log_level: Some(crate::log::Level::default().to_string()),
            log_format: Some(crate::log::Format::default().to_string()),
            root: None,
        };
        format!(
            "# Written by `ketch config reset`. Edit freely.\n{}",
            toml::to_string_pretty(&file).unwrap_or_default()
        )
    }

    /// True when the bin dir is on the caller's PATH.
    ///
    /// On Windows the comparison folds case, `/` vs `\\`, and a trailing
    /// separator — the same rules `shell` uses when editing the user PATH —
    /// so a PATH entry written as `C:\\Users\\…\\.ketch\\bin` still
    /// matches a bin dir resolved as `c:/users/…/.ketch/bin`.
    pub fn bin_dir_on_path(&self) -> bool {
        let Some(path) = std::env::var_os("PATH") else {
            return false;
        };
        let want = path_lookup_key(&self.bin_dir);
        std::env::split_paths(&path).any(|p| path_lookup_key(&p) == want)
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
    if text == "~" {
        return dirs::home_dir().unwrap_or_else(|| path.to_path_buf());
    }
    // PowerShell and Windows config often write `~\.ketch`; Unix always uses `~/`.
    // Join by components so `~\.ketch\bin` is two segments on every host —
    // `home.join(".ketch\bin")` would be one literal name on Unix.
    let rest = text.strip_prefix("~/").or_else(|| text.strip_prefix("~\\"));
    if let Some(rest) = rest {
        if let Some(home) = dirs::home_dir() {
            return join_tilde_rest(&home, rest);
        }
    }
    path.to_path_buf()
}

/// Join a tilde-relative remainder that may use `/` or `\` separators.
fn join_tilde_rest(base: &Path, rest: &str) -> PathBuf {
    let mut out = base.to_path_buf();
    for part in rest.split(['/', '\\']).filter(|s| !s.is_empty()) {
        out.push(part);
    }
    out
}

/// Fold a PATH entry for comparison: separators and a trailing slash everywhere;
/// case only on Windows, where the filesystem does not distinguish it.
fn path_lookup_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/');
    if cfg!(windows) {
        s.to_ascii_lowercase()
    } else {
        s.to_string()
    }
}

/// A setting that has to be parsed, from the environment or the config file.
///
/// A typo is reported against whichever one supplied it, because "unknown log
/// level `verbose`" is only actionable if you know which file to fix.
fn parsed<T: std::str::FromStr<Err = String>>(
    env_key: &str,
    file_key: &str,
    from_file: Option<String>,
) -> Result<Option<T>> {
    let (value, where_from) = match std::env::var(env_key).ok().filter(|v| !v.trim().is_empty()) {
        Some(v) => (v, env_key.to_string()),
        None => match from_file {
            Some(v) => (v, format!("`{file_key}` in config.toml")),
            None => return Ok(None),
        },
    };
    value
        .parse()
        .map(Some)
        .map_err(|e: String| Error::Config(format!("{where_from}: {e}")))
}

fn env_bool(key: &str) -> Result<Option<bool>> {
    let value = match std::env::var(key) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(e) => return Err(Error::Config(format!("{key}: {e}"))),
    };
    // `KETCH_PRERELEASE=` clears the override without forcing a parse error.
    if value.trim().is_empty() {
        return Ok(None);
    }
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(Some(true)),
        "0" | "false" | "no" | "off" => Ok(Some(false)),
        _ => Err(Error::Config(format!(
            "{key} must be a boolean, not `{value}`"
        ))),
    }
}

/// Accept only `owner/repo`, since it is about to become a URL.
///
/// A `github:` prefix is tolerated because that is how the same repository is
/// written everywhere else in ketch; the stored form drops it.
pub fn validate_repo(what: &str, raw: String) -> Result<String> {
    let repo = raw.trim().trim_start_matches("github:");
    let mut parts = repo.split('/');
    // `.` is not a traversal, but it is not an owner or a repository either:
    // `a/.` becomes a URL the path parser rewrites into a different endpoint
    // than the one that was named.
    let named = |part: &str| !part.is_empty() && part != "." && part != "..";
    let shaped = matches!((parts.next(), parts.next(), parts.next()), (Some(o), Some(r), None)
        if named(o) && named(r));
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
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => '-',
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
        assert_eq!(sanitize_component("safe\u{202e}sudo"), "safe-sudo");
    }

    #[test]
    fn resolves_relative_roots_against_the_current_directory() {
        let root = absolute_path(Path::new("scratch")).unwrap();
        assert!(root.is_absolute());
        assert_eq!(root, std::env::current_dir().unwrap().join("scratch"));
    }

    #[test]
    fn rejects_unrecognized_boolean_environment_values() {
        const KEY: &str = "KETCH_TEST_BOOLEAN";
        std::env::set_var(KEY, "sometimes");
        let error = env_bool(KEY).unwrap_err();
        std::env::remove_var(KEY);

        assert!(error.to_string().contains(KEY));
    }

    #[test]
    fn an_empty_ketch_github_token_falls_back_to_the_next_token_variable() {
        std::env::set_var("KETCH_GITHUB_TOKEN", "");
        std::env::set_var("GITHUB_TOKEN", "ghp_fallback");

        let cfg = Config::load(Some(std::env::temp_dir().join("ketch-empty-token-test"))).unwrap();

        std::env::remove_var("KETCH_GITHUB_TOKEN");
        std::env::remove_var("GITHUB_TOKEN");

        assert_eq!(cfg.github_token.as_deref(), Some("ghp_fallback"));
    }

    #[test]
    fn an_empty_boolean_environment_variable_is_treated_as_unset() {
        const KEY: &str = "KETCH_TEST_BOOLEAN_EMPTY";
        std::env::set_var(KEY, "");
        assert_eq!(env_bool(KEY).unwrap(), None);
        std::env::remove_var(KEY);
    }

    #[test]
    fn expand_tilde_accepts_slash_backslash_and_bare_home() {
        let home = dirs::home_dir().expect("home");
        assert_eq!(expand_tilde(Path::new("~")), home);
        assert_eq!(expand_tilde(Path::new("~/scratch")), home.join("scratch"));
        assert_eq!(expand_tilde(Path::new("~\\.ketch")), home.join(".ketch"));
        assert_eq!(expand_tilde(Path::new("/abs")), PathBuf::from("/abs"));
        // Multi-component PowerShell paths must not collapse into one segment.
        assert_eq!(
            expand_tilde(Path::new("~\\.ketch\\bin")),
            home.join(".ketch").join("bin")
        );
        assert_eq!(
            expand_tilde(Path::new("~/scratch\\nested")),
            home.join("scratch").join("nested")
        );
    }
    #[test]
    fn path_lookup_key_folds_case_separators_and_trailing_slash() {
        assert_eq!(
            path_lookup_key(Path::new(r"C:\Users\u\.ketch\bin")),
            path_lookup_key(Path::new("C:/Users/u/.ketch/bin/"))
        );
        if cfg!(windows) {
            assert_eq!(
                path_lookup_key(Path::new(r"C:\Users\U\.ketch\bin")),
                path_lookup_key(Path::new(r"c:\users\u\.ketch\bin"))
            );
        }
    }

    #[test]
    fn bin_dir_on_path_matches_folded_windows_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(Some(tmp.path().to_path_buf())).unwrap();
        let mixed = cfg.bin_dir.to_string_lossy().replace('\\', "/");
        let with_slash = format!("{mixed}/");
        let path = std::env::join_paths([Path::new("/elsewhere"), Path::new(&with_slash)]).unwrap();
        let previous = std::env::var_os("PATH");
        std::env::set_var("PATH", &path);
        let on = cfg.bin_dir_on_path();
        match previous {
            Some(v) => std::env::set_var("PATH", v),
            None => std::env::remove_var("PATH"),
        }
        assert!(on, "folded PATH entry must count as on PATH");
    }
}
