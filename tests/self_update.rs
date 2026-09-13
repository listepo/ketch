//! `ketch self update --dry-run` must not claim an update when already current.

use assert_cmd::Command;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

struct GithubLatestMock {
    api_base: String,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl GithubLatestMock {
    fn spawn(tag: &str) -> Self {
        Self::spawn_with_body(tag, None)
    }

    fn spawn_with_body(tag: &str, release_body: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock github");
        let api_base = format!(
            "http://127.0.0.1:{}",
            listener.local_addr().expect("addr").port()
        );
        let mut payload = serde_json::json!({
            "tag_name": tag,
            "prerelease": false,
            "draft": false,
            "assets": [],
        });
        if let Some(body) = release_body {
            payload["body"] = body.into();
        }
        let body = payload.to_string();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_flag = Arc::clone(&stop);
        let handle = thread::spawn(move || {
            listener
                .set_nonblocking(true)
                .expect("nonblocking mock github");
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0u8; 8192];
                        let _ = stream.read(&mut buf);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(e) => panic!("mock github accept: {e}"),
                }
            }
        });
        Self {
            api_base,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for GithubLatestMock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().expect("mock github thread");
        }
    }
}

#[test]
fn dry_run_when_already_current_does_not_claim_an_update() {
    let mock = GithubLatestMock::spawn(&format!("v{}", env!("CARGO_PKG_VERSION")));
    let temp = TempDir::new().unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            temp.child("root").path().to_str().unwrap(),
            "self",
            "update",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env("KETCH_GITHUB_API", &mock.api_base)
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("already current"))
        .stderr(predicate::str::contains("would update").not());
}

#[test]
fn dry_run_when_a_newer_release_exists_claims_an_update() {
    let mock = GithubLatestMock::spawn("v999.0.0");
    let temp = TempDir::new().unwrap();

    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            temp.child("root").path().to_str().unwrap(),
            "self",
            "update",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env("KETCH_GITHUB_API", &mock.api_base)
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("would update"))
        .stderr(predicate::str::contains("already current").not());
}

#[test]
fn release_notes_are_filtered_before_they_reach_stdout() {
    let notes = "safe\u{202e}release notes".to_string();
    let mock = GithubLatestMock::spawn_with_body("v999.0.0", Some(&notes));
    let temp = TempDir::new().unwrap();

    let assert = Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            temp.child("root").path().to_str().unwrap(),
            "self",
            "update",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env("KETCH_GITHUB_API", &mock.api_base)
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains('\u{202e}'),
        "a bidi override reached the terminal: {stdout:?}"
    );
    assert!(stdout.contains("saferelease notes"), "{stdout:?}");
}
