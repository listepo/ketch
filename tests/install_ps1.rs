//! `install.ps1`: argument handling, and the paths it installs into.
//!
//! Like `install.sh`, this script runs before any ketch binary exists, so its
//! flags are exercised here instead of through the CLI. Text checks run on every
//! host; `pwsh` cases are skipped when PowerShell is not installed. The full
//! stub-release run is Windows-only.

#[cfg(windows)]
use assert_cmd::Command;
#[cfg(windows)]
use predicates::prelude::*;

#[cfg(windows)]
fn pwsh_bin() -> Option<std::path::PathBuf> {
    // Resolve before tests clear PATH — otherwise `Command::new("pwsh")` cannot
    // spawn even when PowerShell is installed on the runner.
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|dir| {
            let candidate = dir.join(if cfg!(windows) { "pwsh.exe" } else { "pwsh" });
            candidate.is_file().then_some(candidate)
        })
    })
}

#[cfg(windows)]
fn pwsh_available() -> bool {
    pwsh_bin().is_some()
}

#[cfg(windows)]
fn install_ps1() -> Command {
    let pwsh = pwsh_bin().expect("pwsh_available checked");
    let mut cmd = Command::new(pwsh);
    cmd.args([
        "-NoProfile",
        "-File",
        concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"),
    ])
    // Absolute pwsh survives this; the script's own PATH lookups stay isolated.
    .env(
        "PATH",
        if cfg!(windows) {
            r"C:\nonexistent-ketch-path"
        } else {
            "/nonexistent"
        },
    )
    .env(
        "USERPROFILE",
        std::env::temp_dir()
            .join("ketch-ps1-test-home")
            .to_string_lossy()
            .as_ref(),
    );
    cmd
}

/// Reaching the release fetch means the flags were accepted.
#[cfg(windows)]
fn fails_after_flag_parse() -> impl predicates::Predicate<str> {
    predicate::str::contains("Fetching latest release")
        .or(predicate::str::contains("Installing ketch"))
        .or(predicate::str::contains("Failed to fetch"))
}

#[test]
fn the_script_names_the_windows_release_tarball() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"));
    assert!(src.contains("pc-windows-msvc"), "{src}");
    assert!(
        src.contains("ketch-$TarballArch-pc-windows-msvc.tar.gz"),
        "{src}"
    );
}

#[test]
fn the_script_defaults_to_userprofile_not_home() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"));
    assert!(src.contains("USERPROFILE"), "{src}");
    assert!(
        !src.contains("$HOME"),
        "Windows dirs::home_dir ignores HOME; the script must not default to it: {src}"
    );
}

#[test]
fn the_script_prefers_system32_tar_over_git_bash_tar() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"));
    assert!(
        src.contains("System32") && src.contains("tar.exe"),
        "install.ps1 must prefer System32 tar.exe so Git Bash GNU tar does not misparse C: paths"
    );
    assert!(
        src.contains("--force-local"),
        "install.ps1 must pass --force-local when falling back to a PATH tar"
    );
}

#[test]
fn the_script_writes_path_through_dotnet_not_setx() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"));
    assert!(src.contains("SetEnvironmentVariable('Path'"), "{src}");
    assert!(!src.contains("setx"), "{src}");
}

/// `--install-dir` is a bootstrap location, not a second way to name the root:
/// it may sit outside `--root`, need not be called `bin`, and needs no `--root`
/// beside it. Only the retired coupling check refused these before anything ran.
///
/// A probe `--version` forces failure after flag parse so the assertion does not
/// depend on GitHub being unreachable (CI can otherwise install for real).
#[test]
#[cfg(windows)]
fn an_install_dir_need_not_sit_under_the_root() {
    if !pwsh_available() {
        return;
    }
    for args in [
        vec![
            "--root",
            "/tmp/a",
            "--install-dir",
            "/tmp/b/bin",
            "--version",
            "v0.0.0-ketch-flag-probe",
        ],
        vec![
            "--install-dir",
            "/tmp/myapp/tools",
            "--version",
            "v0.0.0-ketch-flag-probe",
        ],
    ] {
        install_ps1()
            .args(&args)
            .assert()
            .failure()
            .stdout(fails_after_flag_parse())
            .stdout(predicate::str::contains("is not").not());
    }
}

#[test]
#[cfg(windows)]
fn help_describes_root_and_install_dir_without_naming_the_root() {
    if !pwsh_available() {
        return;
    }
    let pwsh = pwsh_bin().expect("pwsh_available checked");
    Command::new(pwsh)
        .args([
            "-NoProfile",
            "-File",
            concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"),
            "--help",
        ])
        // Linux/mac runners have no USERPROFILE; the script Join-Paths it at load.
        .env("USERPROFILE", "/tmp/ketch-ps1-help-home")
        .assert()
        .success()
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--install-dir"))
        .stdout(predicate::str::contains("names the root").not())
        .stdout(predicate::str::contains("the root becomes its parent").not());
}

/// The whole script, with the network replaced by a local release tree and the
/// archive by a stub `ketch.exe`.
#[cfg(windows)]
mod against_a_stub_release {
    use assert_cmd::Command;
    use assert_fs::prelude::*;
    use assert_fs::TempDir;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use predicates::prelude::*;
    use sha2::{Digest, Sha256};
    use std::path::{Path, PathBuf};

    const STUB_RUST: &str = r#"
use std::path::PathBuf;
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("self") => {
            let root = std::env::var("KETCH_ROOT").expect("KETCH_ROOT");
            let dest = PathBuf::from(&root).join("bin").join("ketch.exe");
            std::fs::create_dir_all(dest.parent().expect("bin parent")).expect("mkdir");
            std::fs::copy(std::env::current_exe().expect("exe"), &dest).expect("copy");
            let mut link_dir = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--link-dir" {
                    i += 1;
                    if i < args.len() {
                        link_dir = Some(args[i].clone());
                    }
                }
                i += 1;
            }
            if let Some(dir) = link_dir {
                std::fs::create_dir_all(&dir).expect("bootstrap dir");
                let dest_key = std::fs::canonicalize(dest.parent().unwrap())
                    .unwrap()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                let dir_key = std::fs::canonicalize(&dir)
                    .unwrap()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                if dest_key != dir_key {
                    std::fs::copy(&dest, PathBuf::from(dir).join("ketch.exe")).expect("bootstrap");
                }
            }
        }
        Some("--help") | Some("-h") => println!("ketch (stub)"),
        _ => println!("ketch (stub)"),
    }
}
"#;

    struct StubRelease {
        dir: TempDir,
        // Kept so the compiled stub path stays live for the payload write;
        // readers only need `dir`.
        _stub_exe: PathBuf,
    }

    impl StubRelease {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let stub_dir = dir.child("stub-src");
            stub_dir.create_dir_all().unwrap();
            let src = stub_dir.child("main.rs");
            src.write_str(STUB_RUST).unwrap();
            let stub_exe = dir.child("ketch.exe");
            let status = std::process::Command::new("rustc")
                .arg(src.path())
                .arg("-o")
                .arg(stub_exe.path())
                .status()
                .expect("compile stub ketch");
            assert!(status.success(), "rustc could not build the stub ketch.exe");

            let payload = dir.child("payload.tar.gz");
            write_payload(payload.path(), stub_exe.path());
            let digest = hex::encode(Sha256::digest(std::fs::read(payload.path()).unwrap()));
            dir.child("SHA256SUMS")
                .write_str(&format!("{digest}  ketch-x86_64-pc-windows-msvc.tar.gz\n"))
                .unwrap();

            StubRelease {
                dir,
                _stub_exe: stub_exe.path().to_path_buf(),
            }
        }
    }

    fn write_payload(path: &Path, stub_exe: &Path) {
        let gz = GzEncoder::new(std::fs::File::create(path).unwrap(), Compression::default());
        let mut tar = tar::Builder::new(gz);
        let bytes = std::fs::read(stub_exe).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "ketch.exe", bytes.as_slice())
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
    }

    fn run(release: &StubRelease, work: &TempDir, args: &[&Path]) -> assert_cmd::assert::Assert {
        let mut cmd = Command::new("pwsh");
        cmd.args([
            "-NoProfile",
            "-File",
            concat!(env!("CARGO_MANIFEST_DIR"), "/install.ps1"),
            "--no-modify-path",
        ])
        .current_dir(work.path())
        .env("KETCH_INSTALL_RELEASE_DIR", release.dir.path())
        .env("USERPROFILE", work.path());
        for arg in args {
            cmd.arg(arg);
        }
        cmd.assert()
    }

    #[test]
    fn an_explicit_install_dir_gets_a_copy_of_the_installed_binary() {
        let release = StubRelease::new();
        let work = TempDir::new().unwrap();
        let root = work.child("root");
        let bootstrap = work.child("bootstrap");

        run(
            &release,
            &work,
            &[
                Path::new("--root"),
                root.path(),
                Path::new("--install-dir"),
                bootstrap.path(),
            ],
        )
        .success();

        let installed = root.child("bin/ketch.exe");
        installed.assert(predicate::path::is_file());
        let bootstrap_bin = bootstrap.child("ketch.exe");
        bootstrap_bin.assert(predicate::path::is_file());
        assert_ne!(
            std::fs::read(installed.path()).unwrap(),
            b"",
            "the store binary must be installed"
        );
        assert_eq!(
            std::fs::read(bootstrap_bin.path()).unwrap(),
            std::fs::read(installed.path()).unwrap(),
            "the bootstrap path must follow the store binary"
        );
    }

    #[test]
    fn a_relative_root_lands_beside_the_caller_not_in_the_temp_dir() {
        let release = StubRelease::new();
        let work = TempDir::new().unwrap();

        run(
            &release,
            &work,
            &[Path::new("--root"), Path::new("relative-root")],
        )
        .success();

        work.child("relative-root/bin/ketch.exe")
            .assert(predicate::path::is_file());
    }

    #[test]
    fn an_install_dir_that_only_respells_the_root_bin_dir_is_left_alone() {
        let release = StubRelease::new();
        let work = TempDir::new().unwrap();
        let root = work.child("root");

        run(
            &release,
            &work,
            &[
                Path::new("--root"),
                root.path(),
                Path::new("--install-dir"),
                root.child("bin").path(),
            ],
        )
        .success();

        let installed = root.child("bin/ketch.exe");
        installed.assert(predicate::path::is_file());
        assert!(
            !bootstrap_exists_beside_store(&root),
            "the installed binary must still be the binary, not a bootstrap copy beside it"
        );
    }

    fn bootstrap_exists_beside_store(root: &assert_fs::fixture::ChildPath) -> bool {
        let bin = root.child("bin");
        let entries = std::fs::read_dir(bin.path()).unwrap();
        entries.count() > 1
    }
}
