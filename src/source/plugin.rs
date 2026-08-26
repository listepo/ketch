//! External source plugins.
//!
//! A plugin is any executable named `ketch-source-<scheme>` found in the
//! plugins directory or on PATH. ketch invokes it with a subcommand and reads
//! one JSON document from stdout. That is the entire contract — plugins can be
//! written in any language and need no ketch release to ship.
//!
//! The protocol is specified in `docs/PLUGINS.md`; `PROTOCOL_VERSION` is what
//! this build speaks.
//!
//! ```text
//! capabilities              -> {"protocol":1,"scheme":"gitlab","download":false,"search":true}
//! describe <id>             -> a SourceInfo object, or null
//! releases <id> [--prerelease] [--limit N]
//!                           -> [ {"tag":"v1.2.3","version":"1.2.3","assets":[...]}, ... ]
//! search <query> --limit N  -> [ SourceInfo, ... ]
//! download <url> <dest>     -> only when capabilities.download is true
//! ```

use super::{ListOpts, Source};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::http::{self, Http};
use crate::model::{Release, ReleaseAsset, SourceInfo};
use crate::ui::ProgressSink;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub const PROTOCOL_VERSION: u32 = 1;

/// Executable prefix a plugin must use to be discovered.
pub const PLUGIN_PREFIX: &str = "ketch-source-";

/// How long a plugin has to answer one subcommand.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);

/// How much it may write to one pipe while doing so.
const PLUGIN_MAX_OUTPUT: u64 = 8 << 20;

/// How often the wait loop looks to see whether it has finished.
const PLUGIN_POLL: Duration = Duration::from_millis(20);

#[derive(Deserialize)]
struct Capabilities {
    protocol: u32,
    scheme: String,
    /// The plugin fetches assets itself, e.g. because they need credentials.
    #[serde(default)]
    download: bool,
    #[serde(default)]
    search: bool,
}

/// A discovered plugin executable.
pub struct PluginSource {
    path: PathBuf,
    scheme: String,
    downloads: bool,
    searches: bool,
}

impl PluginSource {
    /// Interrogate an executable and adopt it if it speaks a version we know.
    pub fn probe(path: &Path) -> Result<Self> {
        let caps: Capabilities = parse(path, &output(path, &["capabilities"])?)?;
        if caps.protocol != PROTOCOL_VERSION {
            return Err(Error::Plugin {
                name: file_name(path),
                detail: format!(
                    "speaks protocol {} but this ketch speaks {PROTOCOL_VERSION}",
                    caps.protocol
                ),
            });
        }
        // The scheme ends up in user input and in recorded state, so it has to
        // be something that can be typed and round-tripped unambiguously.
        if caps.scheme.is_empty()
            || !caps
                .scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::Plugin {
                name: file_name(path),
                detail: format!("reports an unusable scheme `{}`", caps.scheme),
            });
        }
        Ok(PluginSource {
            path: path.to_path_buf(),
            scheme: caps.scheme,
            downloads: caps.download,
            searches: caps.search,
        })
    }

    /// File name of the executable, for diagnostics.
    pub fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("plugin")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn run<T: serde::de::DeserializeOwned>(&self, args: &[&str]) -> Result<T> {
        parse(&self.path, &output(&self.path, args)?)
    }
}

impl Source for PluginSource {
    fn scheme(&self) -> &str {
        &self.scheme
    }

    fn describe(&self, id: &str) -> Result<Option<SourceInfo>> {
        self.run(&["describe", id])
    }

    fn list_releases(&self, id: &str, opts: &ListOpts) -> Result<Vec<Release>> {
        let limit = opts.limit.to_string();
        let mut args = vec!["releases", id, "--limit", &limit];
        if opts.include_prerelease {
            args.push("--prerelease");
        }
        let mut releases: Vec<Release> = self.run(&args)?;
        // The trait promises drafts are gone and prereleases are filtered; a
        // plugin that ignores its flags must not change what ketch installs.
        releases.retain(|r| !r.draft && (opts.include_prerelease || !r.prerelease));
        Ok(releases)
    }

    fn download(
        &self,
        asset: &ReleaseAsset,
        dest: &Path,
        progress: &dyn ProgressSink,
    ) -> Result<String> {
        if !self.downloads {
            // No token is ever handed to a plugin's URLs: whatever credentials
            // an asset needs must come from the plugin's own headers.
            return Http::anonymous().download(&asset.url, dest, &asset.headers, false, progress);
        }
        let dest_str = dest.to_string_lossy().to_string();
        output(&self.path, &["download", &asset.url, &dest_str])?;
        if !dest.exists() {
            return Err(Error::Plugin {
                name: self.name().to_string(),
                detail: format!("reported success but wrote no file to {}", dest.display()),
            });
        }
        // Hash what actually landed on disk. A plugin does not get to assert
        // the checksum of its own download.
        http::sha256_file(dest)
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<SourceInfo>> {
        if !self.searches {
            return Ok(Vec::new());
        }
        self.run(&["search", query, "--limit", &limit.to_string()])
    }
}

/// Find every plugin available to this run.
///
/// Returns one entry per candidate so a single broken plugin can be reported
/// without hiding the ones that work.
pub fn discover(cfg: &Config) -> Vec<Result<PluginSource>> {
    let Ok(platform) = crate::platform::host() else {
        return Vec::new();
    };

    let mut dirs = vec![cfg.plugin_dir.clone()];
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }

    let mut found = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<_> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let named =
                    matches!(name.strip_prefix(PLUGIN_PREFIX), Some(rest) if !rest.is_empty());
                named.then(|| (name, e.path()))
            })
            .collect();
        // Directory order is arbitrary; a stable list keeps `plugin list`
        // and any shadowing warning reproducible.
        names.sort();

        for (name, path) in names {
            // The plugins dir comes first, so it wins over a copy on PATH.
            if seen.contains(&name) || !platform.is_executable(&path) {
                continue;
            }
            seen.push(name);
            found.push(PluginSource::probe(&path));
        }
    }
    found
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plugin")
        .to_string()
}

/// Run one plugin subcommand and return its stdout.
///
/// A plugin is a third-party executable, so this is a trust boundary and not
/// just a convenience wrapper. Three things are enforced here: no stdin, so a
/// plugin cannot sit waiting on a terminal nobody is typing at; a bound on how
/// much it may write, so it cannot exhaust memory; and a deadline, after which
/// it is killed. Without them a single misbehaving plugin hangs every ketch
/// command, because discovery probes all of them before anything else runs.
fn output(path: &Path, args: &[&str]) -> Result<String> {
    let fail = |detail: String| Error::Plugin {
        name: file_name(path),
        detail,
    };
    let mut child = Command::new(path)
        .args(args)
        .env("KETCH_PROTOCOL_VERSION", PROTOCOL_VERSION.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| fail(format!("could not run {}: {e}", path.display())))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (out, err, status) = std::thread::scope(|scope| {
        // Both pipes are drained at once. Filling either one blocks the child,
        // and a child blocked writing to stderr never closes stdout.
        let reading_out = scope.spawn(move || capped(stdout));
        let reading_err = scope.spawn(move || capped(stderr));
        let status = wait_with_deadline(&mut child, PLUGIN_TIMEOUT);
        (
            reading_out.join().unwrap_or_default(),
            reading_err.join().unwrap_or_default(),
            status,
        )
    });

    let status = status.map_err(fail)?;
    if out.len() as u64 > PLUGIN_MAX_OUTPUT {
        return Err(fail(format!(
            "wrote more than {PLUGIN_MAX_OUTPUT} bytes to stdout"
        )));
    }
    if !status.success() {
        return Err(Error::Command {
            cmd: format!("{} {}", file_name(path), args.join(" ")),
            status: status.to_string(),
            stderr: String::from_utf8_lossy(&err).to_string(),
        });
    }
    String::from_utf8(out).map_err(|e| fail(format!("wrote output that is not UTF-8: {e}")))
}

/// Read a pipe to the end, or to the cap — whichever comes first.
fn capped<R: std::io::Read>(pipe: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(pipe) = pipe {
        // One byte past the cap, so the caller can tell "exactly the limit"
        // from "did not stop".
        let _ = pipe.take(PLUGIN_MAX_OUTPUT + 1).read_to_end(&mut buf);
    }
    buf
}

/// Wait for the child, killing it if it outstays its welcome.
fn wait_with_deadline(
    child: &mut std::process::Child,
    timeout: Duration,
) -> std::result::Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "did not answer within {}s and was stopped",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(PLUGIN_POLL),
            Err(e) => {
                let _ = child.kill();
                return Err(format!("could not be waited on: {e}"));
            }
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(path: &Path, body: &str) -> Result<T> {
    serde_json::from_str(body).map_err(|e| Error::Plugin {
        name: file_name(path),
        detail: format!("returned JSON ketch cannot read: {e}"),
    })
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_plugin_that_never_finishes_is_killed() {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 60"])
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        let outcome = wait_with_deadline(&mut child, Duration::from_millis(100));
        assert!(
            outcome.is_err(),
            "a hung plugin must not be waited on forever"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_plugin_cannot_write_without_end() {
        let flood = vec![b'x'; PLUGIN_MAX_OUTPUT as usize + 4096];
        let read = capped(Some(std::io::Cursor::new(flood)));
        assert_eq!(read.len() as u64, PLUGIN_MAX_OUTPUT + 1);
    }

    /// A plugin is just an executable; the smallest honest one is a shell case.
    fn fake_plugin(dir: &Path, scheme: &str, protocol: u32) -> PathBuf {
        let path = dir.join(format!("{PLUGIN_PREFIX}{scheme}"));
        std::fs::write(
            &path,
            format!(
                r#"#!/bin/sh
case "$1" in
  capabilities) echo '{{"protocol":{protocol},"scheme":"{scheme}","search":true}}' ;;
  releases) echo '[{{"tag":"v1.0.0","version":"1.0.0","assets":[]}},
                   {{"tag":"v2.0.0-rc1","version":"2.0.0-rc1","prerelease":true,"assets":[]}},
                   {{"tag":"v3.0.0","version":"3.0.0","draft":true,"assets":[]}}]' ;;
  *) exit 1 ;;
esac
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn probes_a_plugin_and_filters_what_it_returns() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_plugin(dir.path(), "demo", PROTOCOL_VERSION);

        let plugin = PluginSource::probe(&path).unwrap();
        assert_eq!(plugin.scheme(), "demo");

        // Drafts always go, prereleases only when asked for.
        let stable = plugin.list_releases("x/y", &ListOpts::default()).unwrap();
        assert_eq!(stable.len(), 1, "draft and prerelease must be dropped");
        assert_eq!(stable[0].tag, "v1.0.0");

        let opts = ListOpts {
            include_prerelease: true,
            ..Default::default()
        };
        assert_eq!(plugin.list_releases("x/y", &opts).unwrap().len(), 2);

        // Unsupported subcommands surface as errors, not as empty results.
        assert!(plugin.describe("x/y").is_err());
    }

    #[test]
    fn refuses_a_protocol_it_does_not_speak() {
        let dir = tempfile::tempdir().unwrap();
        let path = fake_plugin(dir.path(), "future", PROTOCOL_VERSION + 1);
        assert!(PluginSource::probe(&path).is_err());
    }
}
