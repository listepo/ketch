//! `ketch push`: offering a package to the registry.
//!
//! A `ketch.toml` at a project's root is the same file a registry package
//! folder holds, so contributing the package is a matter of getting that file
//! into `<name>/ketch.toml` of the registry as a pull request. This module does
//! that through the GitHub API alone — no `git`, no `gh` — so it works from any
//! machine ketch itself runs on, with the same token every other command uses.
//!
//! Someone with push access to the registry gets a branch on it directly;
//! everyone else gets a fork, kept in step with the registry before the branch
//! is cut from it. Either way the branch is `ketch/<name>`, so pushing the same
//! package twice updates one pull request rather than opening a second.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::model::{normalize_name, Manifest};
use crate::registry::PACKAGE_FILE;
use octocrab::params::repos::Reference;
use octocrab::Octocrab;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::path::Path;

/// One package, as read from a project's `ketch.toml`.
#[derive(Debug, Clone)]
pub struct Proposal {
    /// The registry folder the file goes into.
    pub name: String,
    /// The file exactly as written: the registry gets the author's text, not a
    /// re-serialisation that would drop their comments and ordering.
    pub body: String,
    pub manifest: Manifest,
}

/// Read and validate a project's `ketch.toml`.
///
/// `name` is optional, as it is in a registry folder: without it, the folder
/// the file sits in names the package, which is also how the registry will
/// read the file once it lands.
pub fn load(path: &Path) -> Result<Proposal> {
    let what = path.display().to_string();
    let body = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let parsed: toml::Value =
        toml::from_str(&body).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    let mut value =
        serde_json::to_value(parsed).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    let table = value.as_object_mut().ok_or_else(|| {
        Error::parse(
            what.as_str(),
            "expected a table of package fields".to_string(),
        )
    })?;
    let name = match table.get("name").and_then(Value::as_str) {
        Some(declared) => declared.to_string(),
        None => {
            let folder = path
                .canonicalize()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .ok_or_else(|| {
                    Error::msg(format!(
                        "{what} has no `name` and sits in no folder that could supply one"
                    ))
                })?;
            let name = normalize_name(&folder);
            table.insert("name".into(), Value::String(name.clone()));
            name
        }
    };
    let manifest =
        Manifest::deserialize(value).map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    manifest
        .validate()
        .map_err(|e| Error::parse(what.as_str(), e.to_string()))?;
    Ok(Proposal {
        name,
        body,
        manifest,
    })
}

/// What `open` needs to know about a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub default_branch: String,
    /// Whether the token may push to it directly, or must go through a fork.
    pub can_push: bool,
}

/// A file as the registry holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// The blob sha, which the contents API demands back before it will
    /// overwrite the file.
    pub sha: String,
    pub text: String,
}

/// The handful of GitHub operations `open` composes, all by `owner/repo`.
///
/// A trait only so the sequence — fork or not, create or update, open or
/// find — can be tested without github.com. [`GitHub`] is the one real
/// implementation.
pub trait Api {
    /// `None` when the repository does not exist, or the token cannot see it.
    fn repository(&self, repo: &str) -> Result<Option<Repo>>;
    /// Fork `repo` for the token's user, or return the fork that exists.
    fn fork(&self, repo: &str) -> Result<String>;
    /// Bring `branch` of `fork` up to the repository it was forked from.
    fn sync_fork(&self, fork: &str, branch: &str) -> Result<()>;
    /// The commit `branch` points at, or `None` when there is no such branch.
    fn branch(&self, repo: &str, branch: &str) -> Result<Option<String>>;
    fn create_branch(&self, repo: &str, branch: &str, sha: &str) -> Result<()>;
    fn reset_branch(&self, repo: &str, branch: &str, sha: &str) -> Result<()>;
    fn file(&self, repo: &str, branch: &str, path: &str) -> Result<Option<File>>;
    /// Commit `text` at `path`; `replacing` is the sha of the file it overwrites.
    fn write_file(
        &self,
        repo: &str,
        branch: &str,
        path: &str,
        message: &str,
        text: &str,
        replacing: Option<&str>,
    ) -> Result<()>;
    /// The new pull request's URL, or `None` when `head` already has one open.
    fn open_pull(
        &self,
        repo: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<Option<String>>;
    fn find_pull(&self, repo: &str, head: &str) -> Result<Option<String>>;
}

/// What `open` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The registry already holds exactly this file; nothing to propose.
    Unchanged,
    Opened(PullRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    pub url: String,
    /// True when the branch already had an open pull request, which the new
    /// commit simply joined.
    pub already_open: bool,
}

/// Put the proposal on a branch and open a pull request for it.
pub fn open(api: &dyn Api, registry: &str, proposal: &Proposal) -> Result<Outcome> {
    let repo = api.repository(registry)?.ok_or_else(|| {
        Error::msg(format!(
            "registry {registry} does not exist, or the token cannot see it"
        ))
    })?;
    let base = repo.default_branch;
    let head_repo = if repo.can_push {
        registry.to_string()
    } else {
        let fork = api.fork(registry)?;
        api.sync_fork(&fork, &base)?;
        fork
    };
    let base_sha = api
        .branch(registry, &base)?
        .ok_or_else(|| Error::msg(format!("{registry} has no branch {base}")))?;

    // Always branch from the registry's current tip. A branch left by an
    // earlier push carries an earlier proposal and nothing else worth keeping.
    let branch = format!("ketch/{}", proposal.name);
    if api.branch(&head_repo, &branch)?.is_some() {
        api.reset_branch(&head_repo, &branch, &base_sha)?;
    } else {
        api.create_branch(&head_repo, &branch, &base_sha)?;
    }

    let path = format!("{}/{PACKAGE_FILE}", proposal.name);
    let existing = api.file(&head_repo, &branch, &path)?;
    let (verb, replacing) = match &existing {
        Some(current) if current.text == proposal.body => return Ok(Outcome::Unchanged),
        Some(current) => ("update", Some(current.sha.as_str())),
        None => ("add", None),
    };
    api.write_file(
        &head_repo,
        &branch,
        &path,
        &format!("{verb} {}", proposal.name),
        &proposal.body,
        replacing,
    )?;

    let head_owner = head_repo.split('/').next().unwrap_or(&head_repo);
    let head = format!("{head_owner}:{branch}");
    let title = format!("{} {}", capitalised(verb), proposal.name);
    if let Some(url) =
        api.open_pull(registry, &head, &base, &title, &pull_request_body(proposal))?
    {
        return Ok(Outcome::Opened(PullRequest {
            url,
            already_open: false,
        }));
    }
    // The commit is on the branch already, so finding its pull request is the
    // whole job left.
    let url = api.find_pull(registry, &head)?.ok_or_else(|| {
        Error::msg(format!(
            "{registry} refused the pull request for {head}, and none is open"
        ))
    })?;
    Ok(Outcome::Opened(PullRequest {
        url,
        already_open: true,
    }))
}

fn pull_request_body(proposal: &Proposal) -> String {
    let m = &proposal.manifest;
    let mut lines = vec![format!("`{}` from `{}`.", proposal.name, m.source)];
    if let Some(description) = &m.description {
        lines.push(String::new());
        lines.push(description.clone());
    }
    if let Some(homepage) = &m.homepage {
        lines.push(String::new());
        lines.push(homepage.clone());
    }
    lines.push(String::new());
    lines.push("Opened with `ketch push`.".to_string());
    lines.join("\n")
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// github.com, through octocrab.
///
/// octocrab is async and the rest of ketch is not, so every call is blocked on
/// a private single-threaded runtime: `push` makes a dozen sequential requests
/// and gains nothing from concurrency.
pub struct GitHub {
    runtime: tokio::runtime::Runtime,
    client: Octocrab,
}

impl GitHub {
    pub fn new(cfg: &Config) -> Result<GitHub> {
        let token = cfg.github_token.clone().ok_or_else(|| {
            Error::msg(
                "`ketch push` opens a pull request as you, which needs a GitHub token: \
                 set KETCH_GITHUB_TOKEN, or `github_token` in config.toml",
            )
        })?;
        GitHub::connect(token)
    }

    fn connect(token: String) -> Result<GitHub> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::io("tokio runtime", e))?;
        // The client wraps its transport in a tower buffer that spawns a worker
        // task as it is built, and panics when no runtime is current. Entering
        // ours for the duration of the build is what makes `new` callable from
        // ketch's synchronous code at all.
        let client = {
            let _guard = runtime.enter();
            Octocrab::builder()
                .personal_token(token)
                .base_uri(crate::source::github::api_base())
                .map_err(github_error)?
                .build()
                .map_err(github_error)?
        };
        Ok(GitHub { runtime, client })
    }

    fn run<T>(&self, call: impl Future<Output = octocrab::Result<T>>) -> Result<T> {
        self.runtime.block_on(call).map_err(github_error)
    }

    /// Like `run`, with 404 as `None`: a missing branch or file is an answer.
    fn find<T>(&self, call: impl Future<Output = octocrab::Result<T>>) -> Result<Option<T>> {
        match self.runtime.block_on(call) {
            Ok(value) => Ok(Some(value)),
            Err(e) if status_of(&e) == Some(404) => Ok(None),
            Err(e) => Err(github_error(e)),
        }
    }
}

fn split(repo: &str) -> (String, String) {
    let (owner, name) = repo.split_once('/').unwrap_or((repo, ""));
    (owner.to_string(), name.to_string())
}

fn status_of(error: &octocrab::Error) -> Option<u16> {
    match error {
        octocrab::Error::GitHub { source, .. } => Some(source.status_code.as_u16()),
        _ => None,
    }
}

fn github_error(error: octocrab::Error) -> Error {
    match &error {
        octocrab::Error::GitHub { source, .. } => Error::Http {
            url: "GitHub".to_string(),
            status: source.status_code.as_u16(),
            detail: Some(source.message.clone()),
        },
        _ => Error::msg(format!("GitHub: {error}")),
    }
}

impl Api for GitHub {
    fn repository(&self, repo: &str) -> Result<Option<Repo>> {
        let (owner, name) = split(repo);
        let found = self.find(self.client.repos(owner, name).get())?;
        found
            .map(|r| {
                let default_branch = r.default_branch.ok_or_else(|| {
                    Error::parse(repo.to_string(), "no default branch".to_string())
                })?;
                Ok(Repo {
                    default_branch,
                    can_push: r.permissions.map(|p| p.push).unwrap_or(false),
                })
            })
            .transpose()
    }

    fn fork(&self, repo: &str) -> Result<String> {
        let (owner, name) = split(repo);
        let fork = self.run(self.client.repos(owner, name).create_fork().send())?;
        let full_name = fork
            .full_name
            .ok_or_else(|| Error::parse(repo.to_string(), "fork has no name".to_string()))?;
        let default_branch = fork.default_branch.unwrap_or_else(|| "main".to_string());
        // A new fork is created asynchronously; its branches appear a few
        // seconds after the API has already answered.
        for _ in 0..10 {
            if self.branch(&full_name, &default_branch)?.is_some() {
                return Ok(full_name);
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        Err(Error::msg(format!(
            "fork {full_name} is still being created; try again in a minute"
        )))
    }

    fn sync_fork(&self, fork: &str, branch: &str) -> Result<()> {
        // octocrab has no typed call for this endpoint.
        let body = serde_json::json!({ "branch": branch });
        self.run(
            self.client
                .post::<_, Value>(format!("/repos/{fork}/merge-upstream"), Some(&body)),
        )?;
        Ok(())
    }

    fn branch(&self, repo: &str, branch: &str) -> Result<Option<String>> {
        let (owner, name) = split(repo);
        let found = self.find(
            self.client
                .repos(owner, name)
                .get_ref(&Reference::Branch(branch.to_string())),
        )?;
        Ok(found.map(|r| match r.object {
            octocrab::models::repos::Object::Commit { sha, .. }
            | octocrab::models::repos::Object::Tag { sha, .. } => sha,
            _ => String::new(),
        }))
    }

    fn create_branch(&self, repo: &str, branch: &str, sha: &str) -> Result<()> {
        let (owner, name) = split(repo);
        self.run(
            self.client
                .repos(owner, name)
                .create_ref(&Reference::Branch(branch.to_string()), sha),
        )?;
        Ok(())
    }

    fn reset_branch(&self, repo: &str, branch: &str, sha: &str) -> Result<()> {
        let body = serde_json::json!({ "sha": sha, "force": true });
        self.run(self.client.patch::<Value, _, _>(
            format!("/repos/{repo}/git/refs/heads/{branch}"),
            Some(&body),
        ))?;
        Ok(())
    }

    fn file(&self, repo: &str, branch: &str, path: &str) -> Result<Option<File>> {
        let (owner, name) = split(repo);
        let found = self.find(
            self.client
                .repos(owner, name)
                .get_content()
                .path(path)
                .r#ref(branch)
                .send(),
        )?;
        Ok(found.and_then(|mut items| {
            let item = items.items.pop()?;
            Some(File {
                text: item.decoded_content().unwrap_or_default(),
                sha: item.sha,
            })
        }))
    }

    fn write_file(
        &self,
        repo: &str,
        branch: &str,
        path: &str,
        message: &str,
        text: &str,
        replacing: Option<&str>,
    ) -> Result<()> {
        let (owner, name) = split(repo);
        let repos = self.client.repos(owner, name);
        let write = match replacing {
            Some(sha) => repos.update_file(path, message, text, sha),
            None => repos.create_file(path, message, text),
        };
        self.run(write.branch(branch).send())?;
        Ok(())
    }

    fn open_pull(
        &self,
        repo: &str,
        head: &str,
        base: &str,
        title: &str,
        body: &str,
    ) -> Result<Option<String>> {
        let (owner, name) = split(repo);
        let created = self.runtime.block_on(
            self.client
                .pulls(owner, name)
                .create(title, head, base)
                .body(body)
                .send(),
        );
        match created {
            Ok(pull) => Ok(Some(
                pull.html_url.map(|u| u.to_string()).unwrap_or_default(),
            )),
            // 422 is how GitHub says the branch already has an open pull request.
            Err(e) if status_of(&e) == Some(422) => Ok(None),
            Err(e) => Err(github_error(e)),
        }
    }

    fn find_pull(&self, repo: &str, head: &str) -> Result<Option<String>> {
        let (owner, name) = split(repo);
        let page = self.run(
            self.client
                .pulls(owner, name)
                .list()
                .state(octocrab::params::State::Open)
                .head(head)
                .send(),
        )?;
        Ok(page
            .items
            .into_iter()
            .next()
            .and_then(|pull| pull.html_url.map(|u| u.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_is_built_from_synchronous_code_without_a_running_runtime() {
        // octocrab's transport spawns onto the current runtime while the
        // client is built; there is none in ketch's synchronous callers.
        GitHub::connect("token".to_string()).unwrap();
    }
    use pretty_assertions::assert_eq;
    use std::cell::RefCell;

    /// A registry in one struct, recording what was asked of it.
    struct Fake {
        can_push: bool,
        branches: Vec<String>,
        file: Option<File>,
        pull_already_open: bool,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn maintainer() -> Fake {
            Fake {
                can_push: true,
                branches: Vec::new(),
                file: None,
                pull_already_open: false,
                calls: RefCell::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn note(&self, call: String) {
            self.calls.borrow_mut().push(call);
        }
    }

    impl Api for Fake {
        fn repository(&self, repo: &str) -> Result<Option<Repo>> {
            self.note(format!("repository {repo}"));
            Ok(Some(Repo {
                default_branch: "main".into(),
                can_push: self.can_push,
            }))
        }

        fn fork(&self, repo: &str) -> Result<String> {
            self.note(format!("fork {repo}"));
            Ok("me/registry".into())
        }

        fn sync_fork(&self, fork: &str, branch: &str) -> Result<()> {
            self.note(format!("sync_fork {fork} {branch}"));
            Ok(())
        }

        fn branch(&self, repo: &str, branch: &str) -> Result<Option<String>> {
            self.note(format!("branch {repo} {branch}"));
            if branch == "main" {
                return Ok(Some("abc123".into()));
            }
            Ok(self
                .branches
                .contains(&branch.to_string())
                .then(|| "stale".into()))
        }

        fn create_branch(&self, repo: &str, branch: &str, sha: &str) -> Result<()> {
            self.note(format!("create_branch {repo} {branch} {sha}"));
            Ok(())
        }

        fn reset_branch(&self, repo: &str, branch: &str, sha: &str) -> Result<()> {
            self.note(format!("reset_branch {repo} {branch} {sha}"));
            Ok(())
        }

        fn file(&self, repo: &str, branch: &str, path: &str) -> Result<Option<File>> {
            self.note(format!("file {repo} {branch} {path}"));
            Ok(self.file.clone())
        }

        fn write_file(
            &self,
            repo: &str,
            branch: &str,
            path: &str,
            message: &str,
            _text: &str,
            replacing: Option<&str>,
        ) -> Result<()> {
            self.note(format!(
                "write_file {repo} {branch} {path} '{message}' {}",
                replacing.unwrap_or("new")
            ));
            Ok(())
        }

        fn open_pull(
            &self,
            repo: &str,
            head: &str,
            base: &str,
            title: &str,
            _body: &str,
        ) -> Result<Option<String>> {
            self.note(format!("open_pull {repo} {head} {base} '{title}'"));
            Ok((!self.pull_already_open).then(|| "https://github.com/acme/registry/pull/1".into()))
        }

        fn find_pull(&self, repo: &str, head: &str) -> Result<Option<String>> {
            self.note(format!("find_pull {repo} {head}"));
            Ok(Some("https://github.com/acme/registry/pull/9".into()))
        }
    }

    fn proposal() -> Proposal {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ketch.toml");
        std::fs::write(
            &file,
            "name = \"tool\"\nsource = \"github:acme/tool\"\ndescription = \"A tool\"\n",
        )
        .unwrap();
        load(&file).unwrap()
    }

    #[test]
    fn a_maintainer_branches_the_registry_itself() {
        let api = Fake::maintainer();
        let out = open(&api, "acme/registry", &proposal()).unwrap();
        assert_eq!(
            out,
            Outcome::Opened(PullRequest {
                url: "https://github.com/acme/registry/pull/1".into(),
                already_open: false,
            })
        );
        assert_eq!(
            api.calls(),
            vec![
                "repository acme/registry",
                "branch acme/registry main",
                "branch acme/registry ketch/tool",
                "create_branch acme/registry ketch/tool abc123",
                "file acme/registry ketch/tool tool/ketch.toml",
                "write_file acme/registry ketch/tool tool/ketch.toml 'add tool' new",
                "open_pull acme/registry acme:ketch/tool main 'Add tool'",
            ]
        );
    }

    #[test]
    fn an_outsider_goes_through_a_fork_kept_in_step_with_the_registry() {
        let api = Fake {
            can_push: false,
            ..Fake::maintainer()
        };
        open(&api, "acme/registry", &proposal()).unwrap();
        let calls = api.calls();
        assert!(calls.contains(&"fork acme/registry".to_string()));
        assert!(calls.contains(&"sync_fork me/registry main".to_string()));
        assert!(calls.contains(&"create_branch me/registry ketch/tool abc123".to_string()));
        assert!(calls.contains(
            &"write_file me/registry ketch/tool tool/ketch.toml 'add tool' new".to_string()
        ));
        assert!(
            calls.contains(&"open_pull acme/registry me:ketch/tool main 'Add tool'".to_string())
        );
    }

    #[test]
    fn an_existing_branch_is_reset_to_the_registry_tip_and_its_file_updated() {
        let api = Fake {
            branches: vec!["ketch/tool".into()],
            file: Some(File {
                sha: "filesha".into(),
                text: "name = \"tool\"\nsource = \"github:acme/old\"\n".into(),
            }),
            ..Fake::maintainer()
        };
        open(&api, "acme/registry", &proposal()).unwrap();
        let calls = api.calls();
        assert!(calls.contains(&"reset_branch acme/registry ketch/tool abc123".to_string()));
        assert!(!calls.iter().any(|c| c.starts_with("create_branch")));
        assert!(calls.contains(
            &"write_file acme/registry ketch/tool tool/ketch.toml 'update tool' filesha"
                .to_string()
        ));
        assert!(calls
            .contains(&"open_pull acme/registry acme:ketch/tool main 'Update tool'".to_string()));
    }

    #[test]
    fn a_file_the_registry_already_has_verbatim_opens_nothing() {
        let api = Fake {
            file: Some(File {
                sha: "filesha".into(),
                text: proposal().body,
            }),
            ..Fake::maintainer()
        };
        let out = open(&api, "acme/registry", &proposal()).unwrap();
        assert_eq!(out, Outcome::Unchanged);
        assert!(!api.calls().iter().any(|c| c.starts_with("write_file")));
    }

    #[test]
    fn a_pull_request_already_open_is_found_rather_than_duplicated() {
        let api = Fake {
            pull_already_open: true,
            ..Fake::maintainer()
        };
        let out = open(&api, "acme/registry", &proposal()).unwrap();
        assert_eq!(
            out,
            Outcome::Opened(PullRequest {
                url: "https://github.com/acme/registry/pull/9".into(),
                already_open: true,
            })
        );
        assert!(api
            .calls()
            .contains(&"find_pull acme/registry acme:ketch/tool".to_string()));
    }

    #[test]
    fn the_folder_names_the_package_when_the_file_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("My-Tool.rs");
        std::fs::create_dir(&project).unwrap();
        let file = project.join("ketch.toml");
        std::fs::write(&file, "source = \"github:acme/tool\"\n").unwrap();
        let out = load(&file).unwrap();
        assert_eq!(out.name, "my-tool");
        assert_eq!(out.manifest.name, "my-tool");
        // The file itself is sent untouched.
        assert_eq!(out.body, "source = \"github:acme/tool\"\n");
    }

    #[test]
    fn a_name_that_would_escape_the_store_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ketch.toml");
        std::fs::write(&file, "name = \"../evil\"\nsource = \"github:acme/tool\"\n").unwrap();
        assert!(load(&file).is_err());
    }

    #[test]
    fn an_unknown_key_is_refused_before_anything_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ketch.toml");
        std::fs::write(
            &file,
            "name = \"tool\"\nsource = \"github:acme/tool\"\nbinary = \"tool\"\n",
        )
        .unwrap();
        assert!(load(&file).is_err());
    }
}
