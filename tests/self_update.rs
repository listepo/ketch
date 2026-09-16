//! `ketch self update --dry-run` must not claim an update when already current.

use assert_cmd::Command;
use assert_fs::fixture::PathChild;
use assert_fs::TempDir;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

struct GithubLatestMock {
    api_base: String,
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl GithubLatestMock {
    fn spawn(tag: &str) -> Self {
        Self::spawn_with_body(tag, None)
    }

    fn spawn_with_body(tag: &str, release_body: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock github");
        let addr = listener.local_addr().expect("addr");
        let api_base = format!("http://127.0.0.1:{}", addr.port());
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
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            ready_tx.send(()).expect("ready");
            while !stop_flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if stop_flag.load(Ordering::Relaxed) {
                            break;
                        }
                        let mut buf = [0u8; 8192];
                        let _ = stream.read(&mut buf);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                    }
                    Err(_) if stop_flag.load(Ordering::Relaxed) => break,
                    Err(e) => panic!("mock github accept: {e}"),
                }
            }
        });
        ready_rx.recv().expect("mock thread started");
        // One probe so the first real caller never races a cold listener.
        if let Ok(mut probe) = TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1))
        {
            let _ = probe.write_all(b"GET / HTTP/1.0\r\n\r\n");
            let mut buf = [0u8; 64];
            let _ = probe.read(&mut buf);
        }
        Self {
            api_base,
            addr,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for GithubLatestMock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Unblock a waiting accept so the join cannot hang.
        let _ = TcpStream::connect_timeout(&self.addr, std::time::Duration::from_millis(200));
        if let Some(handle) = self.handle.take() {
            handle.join().expect("mock github thread");
        }
    }
}

fn ketch_self_update_dry_run(
    mock: &GithubLatestMock,
    root: &std::path::Path,
) -> assert_cmd::assert::Assert {
    Command::cargo_bin("ketch")
        .unwrap()
        .args([
            "--root",
            root.to_str().unwrap(),
            "self",
            "update",
            "--dry-run",
        ])
        .env("NO_COLOR", "1")
        .env("KETCH_GITHUB_API", &mock.api_base)
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .assert()
}

#[test]
fn dry_run_when_already_current_does_not_claim_an_update() {
    let mock = GithubLatestMock::spawn(&format!("v{}", env!("CARGO_PKG_VERSION")));
    let temp = TempDir::new().unwrap();

    ketch_self_update_dry_run(&mock, temp.child("root").path())
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("already current"))
        .stderr(predicate::str::contains("would update").not());
}

#[test]
fn dry_run_when_a_newer_release_exists_claims_an_update() {
    let mock = GithubLatestMock::spawn("v999.0.0");
    let temp = TempDir::new().unwrap();

    ketch_self_update_dry_run(&mock, temp.child("root").path())
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

    let assert = ketch_self_update_dry_run(&mock, temp.child("root").path()).success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(
        !stdout.contains('\u{202e}'),
        "a bidi override reached the terminal: {stdout:?}"
    );
    assert!(stdout.contains("saferelease notes"), "{stdout:?}");
}
