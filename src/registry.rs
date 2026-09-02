//! The package registry: a GitHub repository laid out one folder per package.
//!
//! Every top-level folder names a package and holds a `ketch.toml` describing
//! it. Anything else in the repository — README, licence, CI config — has no
//! `ketch.toml` and is simply not a package, so the registry needs no index
//! file that could drift out of step with its contents.
//!
//! `ketch update` downloads the repository and replaces the local copy under
//! `<root>/registry`. Nothing fetches it implicitly: a package that resolves
//! today must keep resolving offline tomorrow.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::extract::{archive::TarGzExtractor, unwrap_single_dir, Extractor};
use crate::http::Http;
use crate::model::{normalize_name, Manifest};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The file inside a package folder. The folder already names the package, so
/// the file does not have to repeat it.
pub const PACKAGE_FILE: &str = "ketch.toml";

/// True once a copy has been fetched.
pub fn exists(cfg: &Config) -> bool {
    cfg.registry_dir.is_dir()
}

/// Every package in the local copy, each paired with the file it came from.
pub fn load(cfg: &Config) -> Vec<(Manifest, PathBuf)> {
    load_dir(&cfg.registry_dir)
}

/// Fetch the registry and swap it in, returning how many packages it holds.
///
/// The download is staged and only moved into place once it parses, so a bad
/// or truncated fetch leaves the working copy alone.
pub fn update(cfg: &Config) -> Result<usize> {
    let repo = &cfg.registry;
    crate::ui::stage("registry", crate::ui::ProgressStage::Downloading);
    crate::ui::step("updating", &format!("registry {repo}"));

    let staging = tempfile::tempdir_in(&cfg.root).map_err(|e| Error::io(&cfg.root, e))?;
    let tarball = staging.path().join("registry.tar.gz");
    // The API tarball endpoint follows the default branch and honours the
    // token, which keeps unauthenticated rate limits out of the way. It answers
    // 415 to the octet-stream `Accept` that asset downloads use, so ask for the
    // API media type and let it redirect to the gzip.
    let url = format!("https://api.github.com/repos/{repo}/tarball");
    let accept = [(
        "Accept".to_string(),
        "application/vnd.github+json".to_string(),
    )];
    let headers = BTreeMap::from(accept);
    Http::new(cfg).download(
        &url,
        &tarball,
        &headers,
        true,
        crate::ui::progress().as_ref(),
    )?;

    let unpacked = staging.path().join("tree");
    std::fs::create_dir_all(&unpacked).map_err(|e| Error::io(&unpacked, e))?;
    crate::ui::stage("registry", crate::ui::ProgressStage::Extracting);
    TarGzExtractor.extract(&tarball, &unpacked)?;
    // GitHub wraps the tree in one `owner-repo-<sha>` directory.
    let root = unwrap_single_dir(&unpacked)?;

    swap_in(cfg, &root, repo)
}

/// Move a freshly-unpacked tree into place, returning its package count.
///
/// A tree with no packages is refused: a repository that moved, emptied or
/// answered with something unexpected must not wipe a working registry.
fn swap_in(cfg: &Config, tree: &Path, repo: &str) -> Result<usize> {
    let packages = load_dir(tree);
    for problem in collisions(&packages) {
        crate::ui::warn(&problem);
    }
    let count = packages.len();
    if count == 0 {
        return Err(Error::msg(format!(
            "{repo} has no package folders containing `{PACKAGE_FILE}` \
             — leaving the current registry in place"
        )));
    }
    if cfg.registry_dir.exists() {
        std::fs::remove_dir_all(&cfg.registry_dir).map_err(|e| Error::io(&cfg.registry_dir, e))?;
    }
    std::fs::rename(tree, &cfg.registry_dir).map_err(|e| Error::io(&cfg.registry_dir, e))?;
    Ok(count)
}

/// Names that two packages both answer to.
///
/// Nothing else can catch this: each folder is valid on its own, the loser is
/// shadowed silently, and which one loses depends on sort order. Reported as
/// warnings rather than errors so one careless entry cannot block an update for
/// everybody.
fn collisions(packages: &[(Manifest, PathBuf)]) -> Vec<String> {
    let mut claimed: BTreeMap<String, String> = BTreeMap::new();
    let mut out = Vec::new();
    for (manifest, _) in packages {
        for name in std::iter::once(&manifest.name).chain(manifest.provides.iter()) {
            let first = claimed
                .entry(normalize_name(name))
                .or_insert_with(|| manifest.name.clone());
            if first != &manifest.name {
                out.push(format!(
                    "`{name}` is claimed by both `{first}` and `{}`; only `{first}` will resolve",
                    manifest.name
                ));
            }
        }
    }
    out
}

fn load_dir(dir: &Path) -> Vec<(Manifest, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(PACKAGE_FILE).is_file())
        .collect();
    folders.sort();

    let mut out = Vec::new();
    for folder in folders {
        let path = folder.join(PACKAGE_FILE);
        let name = folder
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        match read_package(&path, &name) {
            Ok(manifest) => out.push((manifest, path)),
            // One broken entry must not hide the rest of the registry.
            Err(e) => crate::ui::warn(&format!("ignoring registry package `{name}`: {e}")),
        }
    }
    out
}

/// Parse one package folder.
///
/// The folder is the package name, so `name` in the file is optional — and
/// when it is present it must agree, or the package would be unreachable under
/// the name its folder advertises.
fn read_package(path: &Path, folder: &str) -> Result<Manifest> {
    let what = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let mut value: toml::Value =
        toml::from_str(&text).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    let table = value.as_table_mut().ok_or_else(|| {
        Error::parse(
            what.as_str(),
            "expected a table of package fields".to_string(),
        )
    })?;

    match table.get("name").and_then(|v| v.as_str()) {
        Some(declared) if normalize_name(declared) != normalize_name(folder) => {
            return Err(Error::parse(
                what.as_str(),
                format!("declares name `{declared}` but sits in folder `{folder}`"),
            ))
        }
        Some(_) => {}
        None => {
            table.insert("name".into(), toml::Value::String(folder.to_string()));
        }
    }
    let manifest =
        Manifest::deserialize(value).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    manifest
        .validate()
        .map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, folder: &str, body: &str) {
        let package = dir.join(folder);
        std::fs::create_dir_all(&package).unwrap();
        std::fs::write(package.join(PACKAGE_FILE), body).unwrap();
    }

    #[test]
    fn the_folder_names_the_package() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "ripgrep",
            "source = \"github:BurntSushi/ripgrep\"\n",
        );
        let found = load_dir(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "ripgrep");
        assert_eq!(found[0].0.source.id, "BurntSushi/ripgrep");
    }

    #[test]
    fn a_declared_name_must_match_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "fzf",
            "name = \"fzy\"\nsource = \"github:junegunn/fzf\"\n",
        );
        assert!(load_dir(tmp.path()).is_empty());
    }

    #[test]
    fn non_package_folders_and_broken_entries_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".github/workflows")).unwrap();
        std::fs::write(tmp.path().join("README.md"), "hi").unwrap();
        write(tmp.path(), "broken", "source = 12\n");
        write(tmp.path(), "jq", "source = \"github:jqlang/jq\"\n");
        let found = load_dir(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "jq");
    }

    #[test]
    fn a_name_two_packages_claim_is_reported_once_against_its_first_owner() {
        let tmp = tempfile::tempdir().unwrap();
        // `fd` provides its own name, which is not a collision with itself.
        write(
            tmp.path(),
            "fd",
            "source = \"github:sharkdp/fd\"\nprovides = [\"fd\"]\n",
        );
        write(tmp.path(), "rg", "source = \"github:BurntSushi/ripgrep\"\n");
        assert!(collisions(&load_dir(tmp.path())).is_empty());

        write(
            tmp.path(),
            "zfd",
            "source = \"github:someone/zfd\"\nprovides = [\"fd\"]\n",
        );
        let found = collisions(&load_dir(tmp.path()));
        assert_eq!(found.len(), 1);
        assert!(found[0].contains("both `fd` and `zfd`"), "{}", found[0]);
    }

    #[test]
    fn an_empty_tree_never_replaces_a_working_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(Some(tmp.path().to_path_buf())).unwrap();
        std::fs::create_dir_all(&cfg.registry_dir).unwrap();
        write(&cfg.registry_dir, "jq", "source = \"github:jqlang/jq\"\n");

        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(swap_in(&cfg, &empty, "someone/registry").is_err());
        assert_eq!(load(&cfg).len(), 1, "the old registry must still be there");

        let fresh = tmp.path().join("fresh");
        write(&fresh, "fd", "source = \"github:sharkdp/fd\"\n");
        write(&fresh, "rg", "source = \"github:BurntSushi/ripgrep\"\n");
        assert_eq!(swap_in(&cfg, &fresh, "someone/registry").unwrap(), 2);
        let names: Vec<String> = load(&cfg).into_iter().map(|(m, _)| m.name).collect();
        assert_eq!(names, ["fd", "rg"]);
    }

    #[test]
    fn a_package_that_would_install_outside_the_store_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "evil",
            "name = \"evil\"\nsource = \"github:a/b\"\n\
             bin = [{ name = \"../../../.zshrc\" }]\n",
        );
        write(
            tmp.path(),
            "typo",
            "source = \"github:a/b\"\nbinary = \"x\"\n",
        );
        write(tmp.path(), "ok", "source = \"github:a/b\"\n");
        let found = load_dir(tmp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.name, "ok");
    }
}
