//! `install.sh`: argument handling, and the paths it installs into.
//!
//! The script is the one piece of the release that runs before any ketch
//! exists, so its flags cannot be covered by driving the binary. The flag cases
//! below empty PATH: a check that fails to stop the script there runs into a
//! missing `id`/`curl` rather than the network. The path cases run the whole
//! script against a stub `curl` serving a stub release, which is the only way
//! to see where the binary and its bootstrap link actually land.

#![cfg(unix)]

use assert_cmd::Command;
use predicates::prelude::*;

fn install_sh() -> Command {
    let mut cmd = Command::new("/bin/bash");
    cmd.arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .env("PATH", "/nonexistent")
        .env("HOME", "/nonexistent-home");
    cmd
}

/// Reaching `id` means the flags were accepted and the script went on to the
/// checks that need a real machine.
fn fails_after_flag_parse() -> impl predicates::Predicate<str> {
    predicate::str::contains("id").or(predicate::str::contains("command not found"))
}

/// `--install-dir` is a bootstrap location, not a second way to name the root:
/// it may sit outside `--root`, need not be called `bin`, and needs no `--root`
/// beside it. Only the retired coupling check refused these before anything ran.
#[test]
fn an_install_dir_need_not_sit_under_the_root() {
    for args in [
        vec!["--root", "/tmp/a", "--install-dir", "/tmp/b/bin"],
        vec!["--install-dir", "/tmp/myapp/tools"],
    ] {
        install_sh()
            .args(&args)
            .assert()
            .failure()
            .stderr(fails_after_flag_parse())
            .stderr(predicate::str::contains("is not").not());
    }
}

#[test]
fn help_describes_root_and_install_dir_without_naming_the_root() {
    // `--help` prints via `cat` and exits before `id`; only /bin is needed.
    Command::new("/bin/bash")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .env("PATH", "/bin")
        .env("HOME", "/nonexistent-home")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--root"))
        .stdout(predicate::str::contains("--install-dir"))
        .stdout(predicate::str::contains("names the root").not())
        .stdout(predicate::str::contains("the root becomes its parent").not())
        .stdout(predicate::str::contains("macOS-only").not());
}

/// The script used to abort on anything but Darwin before flags were even
/// exercised. The host tarball names are the contract with package.sh.
#[test]
fn the_script_names_linux_and_windows_release_tarballs() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"));
    assert!(src.contains("unknown-linux-gnu"), "{src}");
    assert!(src.contains("pc-windows-msvc"), "{src}");
    assert!(!src.contains("macOS-only at the moment"), "{src}");
}

/// The whole script, with the network replaced by a stub `curl` and the release
/// by a stub `ketch`.
///
/// macOS: the script fetches a host tarball and runs `self install`. Linux CI
/// runners are often root, which the script refuses; the package job covers
/// the Linux tarball instead. Windows Git Bash is `target_os = "windows"`.
#[cfg(target_os = "macos")]
mod against_a_stub_release {
    use assert_cmd::Command;
    use assert_fs::prelude::*;
    use assert_fs::TempDir;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use predicates::prelude::*;
    use sha2::{Digest, Sha256};
    use std::path::Path;

    /// A stand-in for the host binary. `self install` copies itself where the
    /// real one would put itself, so the script's own steps — and nothing else
    /// — decide where the binary and the bootstrap link end up.
    const STUB_KETCH: &str = r#"#!/bin/bash
case "${1:-}" in
  self)
    mkdir -p "${KETCH_ROOT}/bin"
    cp "$0" "${KETCH_ROOT}/bin/ketch"
    link_dir=""
    shift
    while [ $# -gt 0 ]; do
      case "$1" in
        --link-dir)
          shift
          link_dir="${1:-}"
          ;;
      esac
      shift
    done
    if [ -n "${link_dir}" ]; then
      mkdir -p "${link_dir}"
      dest="$(cd "${link_dir}" && pwd -P)"
      bin="$(cd "${KETCH_ROOT}/bin" && pwd -P)"
      if [ "${dest}" != "${bin}" ]; then
        ln -sfn "${KETCH_ROOT}/bin/ketch" "${link_dir}/ketch"
      fi
    fi
    ;;
  path) ;;
  *) echo 'ketch (stub)' ;;
esac
exit 0
"#;

    /// Serves the three URLs the script asks for, and nothing else.
    const STUB_CURL: &str = "#!/bin/bash\n\
        out=''\n\
        url=''\n\
        while [ $# -gt 0 ]; do\n\
        \x20 case \"$1\" in\n\
        \x20   -o) shift; out=\"$1\" ;;\n\
        \x20   -*) ;;\n\
        \x20   *) url=\"$1\" ;;\n\
        \x20 esac\n\
        \x20 shift\n\
        done\n\
        case \"${url}\" in\n\
        \x20 */releases/latest) printf '{\"tag_name\": \"v9.9.9\"}\\n' ;;\n\
        \x20 */SHA256SUMS) cp \"${FAKE_RELEASE_DIR}/SHA256SUMS\" \"${out}\" ;;\n\
        \x20 *) cp \"${FAKE_RELEASE_DIR}/payload.tar.gz\" \"${out}\" ;;\n\
        esac\n";

    /// A release tree on disk: one tarball, the checksums that name it under
    /// both architectures, and a `curl` that hands them out.
    struct StubRelease {
        dir: TempDir,
    }

    impl StubRelease {
        fn new() -> Self {
            let dir = TempDir::new().unwrap();
            let payload = dir.child("payload.tar.gz");
            write_payload(payload.path());
            let digest = hex::encode(Sha256::digest(std::fs::read(payload.path()).unwrap()));
            // Both names, because which one the script asks for depends on the
            // machine and on whether it is running translated.
            dir.child("SHA256SUMS")
                .write_str(&format!(
                    "{digest}  ketch-aarch64-apple-darwin.tar.gz\n\
                     {digest}  ketch-x86_64-apple-darwin.tar.gz\n\
                     {digest}  ketch-aarch64-unknown-linux-gnu.tar.gz\n\
                     {digest}  ketch-x86_64-unknown-linux-gnu.tar.gz\n\
                     {digest}  ketch-x86_64-pc-windows-msvc.tar.gz\n"
                ))
                .unwrap();

            let curl = dir.child("bin/curl");
            dir.child("bin").create_dir_all().unwrap();
            curl.write_str(STUB_CURL).unwrap();
            executable(curl.path());
            StubRelease { dir }
        }

        /// PATH with the stub first and the tools the script shells out to on it.
        fn path(&self) -> String {
            format!(
                "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                self.dir.child("bin").path().display()
            )
        }
    }

    fn write_payload(path: &Path) {
        let gz = GzEncoder::new(std::fs::File::create(path).unwrap(), Compression::default());
        let mut tar = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_size(STUB_KETCH.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "ketch", STUB_KETCH.as_bytes())
            .unwrap();
        tar.into_inner().unwrap().finish().unwrap();
    }

    fn executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn run(release: &StubRelease, work: &TempDir, args: &[&Path]) -> assert_cmd::assert::Assert {
        let mut cmd = Command::new("/bin/bash");
        cmd.arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
            .arg("--no-modify-path")
            .current_dir(work.path())
            .env("PATH", release.path())
            .env("FAKE_RELEASE_DIR", release.dir.path())
            .env("HOME", work.path());
        for arg in args {
            cmd.arg(arg);
        }
        cmd.assert()
    }

    /// An explicit `--install-dir` gets a link to the store's binary, so
    /// `ketch self update` — which replaces that binary in place — is what the
    /// bootstrap path runs afterwards. A copy would keep running this version.
    #[test]
    fn an_explicit_install_dir_gets_a_link_to_the_installed_binary() {
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

        let installed = root.child("bin/ketch");
        installed.assert(predicate::path::is_file());
        let link = bootstrap.child("ketch");
        assert!(
            std::fs::symlink_metadata(link.path())
                .unwrap()
                .file_type()
                .is_symlink(),
            "the bootstrap path must follow the store binary, not freeze a copy"
        );
        assert!(
            link.path().is_file(),
            "the link must resolve to the binary ketch installed"
        );
        assert_eq!(
            std::fs::read_link(link.path()).unwrap(),
            installed.path(),
            "the link must point at the binary ketch updates"
        );
    }

    /// `--root` is where ketch lives for good, so a relative one belongs to the
    /// directory the user ran the script in. The script cds into a temp
    /// directory of its own, and a root resolved after that is created inside
    /// it — then deleted with it when the script exits.
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

        work.child("relative-root/bin/ketch")
            .assert(predicate::path::is_file());
    }

    /// `--install-dir <root>/bin` in any other spelling names the directory the
    /// binary is already in. Treated as a bootstrap location it points the
    /// installed binary at itself, and the install is gone.
    #[test]
    fn an_install_dir_that_only_respells_the_root_bin_dir_is_left_alone() {
        let release = StubRelease::new();
        let work = TempDir::new().unwrap();
        let root = work.child("root");
        // The same directory through a link: `/tmp/x` is `/private/tmp/x` on
        // macOS, and any dotfiles-style link does the same thing.
        let alias = work.child("alias");
        std::os::unix::fs::symlink(work.path(), alias.path()).unwrap();

        for (label, spelled_root, spelled_bin) in [
            (
                "a doubled slash",
                root.path().display().to_string(),
                format!("{}//", root.child("bin").path().display()),
            ),
            (
                "a symlinked parent",
                alias.child("root").path().display().to_string(),
                root.child("bin").path().display().to_string(),
            ),
        ] {
            run(
                &release,
                &work,
                &[
                    Path::new("--root"),
                    Path::new(&spelled_root),
                    Path::new("--install-dir"),
                    Path::new(&spelled_bin),
                ],
            )
            .success();

            let installed = root.child("bin/ketch");
            installed.assert(predicate::path::is_file());
            assert!(
                std::fs::symlink_metadata(installed.path())
                    .unwrap()
                    .file_type()
                    .is_file(),
                "{label}: the installed binary must still be the binary, not a link to itself"
            );
        }
    }
}
