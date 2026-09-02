//! HTTP access.
//!
//! One agent for the whole process so connections are reused. Downloads hash
//! while they stream, so verification costs no extra read of the file.

use crate::config::{Config, USER_AGENT};
use crate::error::{Error, Result};
use crate::ui::ProgressSink;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// Cap on API response bodies. Release lists are small; anything larger is a
/// sign we are being fed something we should not buffer.
const MAX_API_BODY: u64 = 32 * 1024 * 1024;

pub struct Http {
    agent: ureq::Agent,
    token: Option<String>,
}

impl Http {
    pub fn new(cfg: &Config) -> Self {
        let agent = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(120))
            .build();
        Http {
            agent,
            token: cfg.github_token.clone(),
        }
    }

    /// Without a token for hosts that are not GitHub.
    pub fn anonymous() -> Self {
        let agent = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(120))
            .build();
        Http { agent, token: None }
    }

    // Part of the public surface, with no caller in the tree yet.
    #[allow(dead_code)]
    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn request(&self, url: &str, accept: &str, authed: bool) -> ureq::Request {
        let mut req = self.agent.get(url).set("Accept", accept);
        if authed {
            if let Some(token) = &self.token {
                req = req.set("Authorization", &format!("Bearer {token}"));
                req = req.set("X-GitHub-Api-Version", "2022-11-28");
            }
        }
        req
    }

    /// GET and deserialize JSON.
    pub fn get_json<T: DeserializeOwned>(&self, url: &str, authed: bool) -> Result<T> {
        let body = self.get_string(url, "application/vnd.github+json", authed)?;
        serde_json::from_str(&body).map_err(|e| Error::parse(url.to_string(), e.to_string()))
    }

    /// GET a text body (checksum files, plain manifests).
    pub fn get_text(&self, url: &str, authed: bool) -> Result<String> {
        self.get_string(url, "text/plain, */*", authed)
    }

    fn get_string(&self, url: &str, accept: &str, authed: bool) -> Result<String> {
        crate::ui::debug(&format!("GET {url}"));
        let response = self
            .request(url, accept, authed)
            .call()
            .map_err(|e| classify(url, e))?;
        let mut body = String::new();
        response
            .into_reader()
            .take(MAX_API_BODY)
            .read_to_string(&mut body)
            .map_err(|e| Error::io(url, e))?;
        Ok(body)
    }

    /// Like `get_json`, but `None` on 404 instead of an error. Used where a
    /// missing resource is an ordinary answer rather than a failure.
    pub fn get_json_opt<T: DeserializeOwned>(&self, url: &str, authed: bool) -> Result<Option<T>> {
        match self.get_json(url, authed) {
            Ok(value) => Ok(Some(value)),
            Err(Error::Http { status: 404, .. }) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Stream a URL to `dest`, hashing as it goes.
    ///
    /// Returns the lowercase hex SHA-256 of the bytes written. The file is
    /// written in full or not at all: we stage next to the destination and
    /// rename, so an interrupted download never looks like a complete one.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        headers: &BTreeMap<String, String>,
        authed: bool,
        progress: &dyn ProgressSink,
    ) -> Result<String> {
        crate::ui::debug(&format!("GET {url} -> {}", dest.display()));
        let mut req = self.request(url, "application/octet-stream", authed);
        for (key, value) in headers {
            req = req.set(key, value);
        }
        let response = req.call().map_err(|e| classify(url, e))?;

        let total: Option<u64> = response
            .header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|n| *n > 0);
        let label = dest
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".to_string());
        progress.start(total, &label);

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let parent = dest.parent().unwrap_or(Path::new("."));
        let mut staged =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| Error::io(parent, e))?;

        let mut reader = response.into_reader();
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 128 * 1024];
        let mut written: u64 = 0;
        loop {
            let n = reader.read(&mut buffer).map_err(|e| Error::io(url, e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            staged
                .write_all(&buffer[..n])
                .map_err(|e| Error::io(staged.path(), e))?;
            written += n as u64;
            progress.advance(n as u64);
        }
        staged.flush().map_err(|e| Error::io(staged.path(), e))?;

        // A truncated transfer that still returned 200 would otherwise be
        // indistinguishable from success until the checksum stage.
        if let Some(expected) = total {
            if written != expected {
                return Err(Error::msg(format!(
                    "download of {label} ended early: got {written} of {expected} bytes"
                )));
            }
        }

        staged.persist(dest).map_err(|e| Error::io(dest, e.error))?;
        progress.finish(&format!("{label} ({})", crate::ui::bytes(written)));
        Ok(hex::encode(hasher.finalize()))
    }
}

/// Turn a `ureq` failure into our error type, keeping the server's own message
/// when it sent one — GitHub's bodies explain rate limits precisely.
fn classify(url: &str, err: ureq::Error) -> Error {
    match err {
        ureq::Error::Status(status, response) => {
            let detail = response
                .into_string()
                .ok()
                .and_then(|body| extract_message(&body))
                .filter(|d| !d.is_empty());
            Error::Http {
                url: url.to_string(),
                status,
                detail,
            }
        }
        transport => Error::Network {
            url: url.to_string(),
            source: Box::new(transport),
        },
    }
}

/// Pull `message` out of a JSON error body, else return a trimmed snippet.
fn extract_message(body: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value.get("message").and_then(|m| m.as_str()) {
            return Some(message.to_string());
        }
    }
    let snippet = body.trim();
    if snippet.is_empty() {
        None
    } else {
        Some(crate::ui::truncate(snippet, 200))
    }
}

/// Hash a file that is already on disk.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 128 * 1024];
    loop {
        let n = file.read(&mut buffer).map_err(|e| Error::io(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_github_error_messages() {
        let body = r#"{"message":"API rate limit exceeded","documentation_url":"https://x"}"#;
        assert_eq!(
            extract_message(body).as_deref(),
            Some("API rate limit exceeded")
        );
    }

    #[test]
    fn falls_back_to_body_snippet() {
        assert_eq!(
            extract_message("bad gateway").as_deref(),
            Some("bad gateway")
        );
        assert_eq!(extract_message("   "), None);
    }

    #[test]
    fn hashes_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
