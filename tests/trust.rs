//! Publisher signatures end to end: a manifest's `trust` table held against
//! a release served by the offline test plugin.
//!
//! The unit tests in `src/trust.rs` prove each verifier. These prove the
//! pipeline around them: the sidecar is found among the release assets, a
//! refusal stops the install before anything is placed, and what verified
//! reaches `state.json`, `ketch info` and the log.
//!
//! The fixture tarball holds a shell script, and the checksum list names
//! only macOS and Linux builds, so the suite runs on those two.
#![cfg(any(target_os = "macos", target_os = "linux"))]

mod support;

use serde_json::Value;
use std::path::{Path, PathBuf};
use support::{host_arch, Asset, Release, Sandbox};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/trust")
        .join(name)
}

fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("read fixture")
}

/// The name `tests/fixtures/trust/SHA256SUMS` lists for this host.
fn asset_name() -> String {
    format!("signedtool-{}-{}.tar.gz", std::env::consts::OS, host_arch())
}

/// The signed tarball, served under this host's name.
fn tarball(sandbox: &Sandbox) -> Asset {
    sandbox.file_asset(&asset_name(), &fixture("signedtool.tar.gz"))
}

fn write_manifest(sandbox: &Sandbox, trust: &str) {
    let dir = sandbox.root().join("manifests");
    std::fs::create_dir_all(&dir).expect("manifests dir");
    std::fs::write(
        dir.join("signedtool.toml"),
        format!("name = \"signedtool\"\nsource = \"test:signedtool\"\n\n[trust]\n{trust}"),
    )
    .expect("write manifest");
}

fn minisign_trust(mode: &str) -> String {
    let key = fixture_text("minisign.pub");
    let key_line = key.lines().last().expect("key line").trim();
    format!("verifier = \"minisign\"\nmode = \"{mode}\"\npublic_key = \"{key_line}\"\n")
}

fn info(sandbox: &Sandbox) -> Value {
    serde_json::from_str(sandbox.ok(&["info", "signedtool", "--json"]).trim())
        .expect("info --json is JSON")
}

fn stderr(sandbox: &Sandbox, args: &[&str]) -> String {
    let out = sandbox.ketch(args);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        out.status.success(),
        "`ketch {}` failed:\n{stderr}",
        args.join(" ")
    );
    stderr
}

#[test]
fn a_minisign_signed_release_installs_and_records_the_pinned_key() {
    let sandbox = Sandbox::new();
    let sidecar = format!("{}.minisig", asset_name());
    sandbox.publish(
        "signedtool",
        &[Release::new(
            "1.0.0",
            vec![
                tarball(&sandbox),
                sandbox.file_asset(&sidecar, &fixture("signedtool.tar.gz.minisig")),
            ],
        )],
    );
    write_manifest(&sandbox, &minisign_trust("require"));

    let said = stderr(&sandbox, &["install", "signedtool"]);
    assert!(
        said.contains("minisign signature by minisign key"),
        "{said}"
    );
    // The signature's own comments are the signer's words, never printed.
    assert!(!said.contains("MARKER"), "{said}");

    let info = info(&sandbox);
    assert_eq!(info["publisher_trust"], "signed");
    assert_eq!(info["provenance"]["verifier"], "minisign");
    assert_eq!(info["provenance"]["signature"], sidecar.as_str());
    assert!(info["provenance"].get("signed").is_none(), "{info}");

    let text = sandbox.ok(&["info", "signedtool"]);
    assert!(text.contains("signed"), "{text}");
    assert!(sandbox.log().contains("verified"), "{}", sandbox.log());
}

#[test]
fn a_gpg_signed_checksum_list_vouches_for_the_asset_it_names() {
    let sandbox = Sandbox::new();
    sandbox.publish(
        "signedtool",
        &[Release::new(
            "1.0.0",
            vec![
                tarball(&sandbox),
                sandbox.file_asset("SHA256SUMS", &fixture("SHA256SUMS")),
                sandbox.file_asset("SHA256SUMS.asc", &fixture("SHA256SUMS.asc")),
            ],
        )],
    );
    let fingerprint = fixture_text("publisher.fpr").trim().to_string();
    write_manifest(
        &sandbox,
        &format!(
            "verifier = \"gpg\"\nsigned = \"SHA256SUMS\"\nfingerprint = \"{fingerprint}\"\n\
             public_key = '''\n{}'''\n",
            fixture_text("publisher.asc")
        ),
    );

    sandbox.ok(&["install", "signedtool"]);
    let info = info(&sandbox);
    assert_eq!(info["provenance"]["verifier"], "gpg");
    assert_eq!(info["provenance"]["signed"], "SHA256SUMS");
    assert_eq!(
        info["provenance"]["identity"],
        format!("OpenPGP key {fingerprint}").as_str()
    );
}

#[test]
fn a_signature_by_another_key_stops_the_install_before_anything_is_placed() {
    let sandbox = Sandbox::new();
    sandbox.publish(
        "signedtool",
        &[Release::new(
            "1.0.0",
            vec![
                tarball(&sandbox),
                sandbox.file_asset(
                    &format!("{}.minisig", asset_name()),
                    &fixture("signedtool.tar.gz.other.minisig"),
                ),
            ],
        )],
    );
    write_manifest(&sandbox, &minisign_trust("require"));

    let err = sandbox.fails(&["install", "signedtool"]);
    assert!(err.contains("could not be verified"), "{err}");
    assert!(err.contains("not made by the pinned key"), "{err}");
    assert!(!sandbox.store().join("signedtool").exists());
    assert!(!sandbox.bin().join("signedtool").exists());
}

#[test]
fn a_release_without_the_required_sidecar_is_refused() {
    let sandbox = Sandbox::new();
    sandbox.publish(
        "signedtool",
        &[Release::new("1.0.0", vec![tarball(&sandbox)])],
    );
    write_manifest(&sandbox, &minisign_trust("require"));

    let err = sandbox.fails(&["install", "signedtool"]);
    assert!(err.contains("publishes no"), "{err}");
    assert!(!sandbox.bin().join("signedtool").exists());
}

#[test]
fn warn_mode_installs_on_the_checksum_alone_and_says_so() {
    let sandbox = Sandbox::new();
    sandbox.publish(
        "signedtool",
        &[Release::new("1.0.0", vec![tarball(&sandbox)])],
    );
    write_manifest(&sandbox, &minisign_trust("warn"));

    let said = stderr(&sandbox, &["install", "signedtool"]);
    assert!(said.contains("signature not verified"), "{said}");
    let info = info(&sandbox);
    assert_eq!(info["publisher_trust"], "checksum");
    assert!(info["provenance"].is_null(), "{info}");
}
