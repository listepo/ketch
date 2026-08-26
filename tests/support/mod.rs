//! Scaffolding for the end-to-end tests: a throwaway ketch root, fixture
//! archives that stand in for real release assets, and a source plugin that
//! serves them.
//!
//! Everything here is offline on purpose. The plugin protocol already lets a
//! source hand ketch an asset it fetched itself (`docs/PLUGINS.md`), so a
//! twenty-line shell script is enough to play the part of a release host — no
//! network, no fixtures checked into the tree, and no test that depends on
//! somebody else's tag still existing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A ketch root, applications directory and plugin, all inside one temp dir
/// that is removed when the test ends.
pub struct Sandbox {
    tmp: tempfile::TempDir,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let tmp = tempfile::tempdir().expect("temp dir");
        let sandbox = Sandbox { tmp };
        for dir in [
            sandbox.root(),
            sandbox.apps(),
            sandbox.plugin_dir(),
            sandbox.assets(),
        ] {
            std::fs::create_dir_all(dir).expect("create sandbox dir");
        }
        sandbox.write_plugin();
        sandbox
    }

    pub fn root(&self) -> PathBuf {
        self.tmp.path().join("root")
    }

    pub fn apps(&self) -> PathBuf {
        self.tmp.path().join("Applications")
    }

    pub fn bin(&self) -> PathBuf {
        self.root().join("bin")
    }

    pub fn store(&self) -> PathBuf {
        self.root().join("store")
    }

    fn plugin_dir(&self) -> PathBuf {
        self.root().join("plugins")
    }

    /// Where fixture assets and the JSON the plugin serves both live.
    fn assets(&self) -> PathBuf {
        self.tmp.path().join("assets")
    }

    /// Run ketch against this sandbox. The environment is set per-invocation
    /// rather than process-wide, so tests stay safe to run in parallel.
    pub fn ketch(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ketch"))
            .args(args)
            .env("KETCH_ROOT", self.root())
            .env("KETCH_APPS_DIR", self.apps())
            .env("NO_COLOR", "1")
            // The bin dir on PATH is the configuration ketch is installed
            // into; `doctor` is right to fail without it.
            .env("PATH", self.path_with_bin())
            // A token in the ambient environment (CI always has one) must not
            // reach a test: nothing here is allowed to touch the network.
            .env_remove("KETCH_GITHUB_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_TOKEN")
            .output()
            .expect("run ketch")
    }

    /// `PATH` with the sandbox bin dir in front of the inherited one.
    fn path_with_bin(&self) -> std::ffi::OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![self.bin()];
        dirs.extend(std::env::split_paths(&inherited));
        std::env::join_paths(dirs).expect("join PATH")
    }

    /// Run ketch and fail the test with its full output if it did not succeed.
    pub fn ok(&self, args: &[&str]) -> String {
        let out = self.ketch(args);
        assert!(
            out.status.success(),
            "`ketch {}` failed with {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    /// Run ketch expecting failure, returning stderr.
    pub fn fails(&self, args: &[&str]) -> String {
        let out = self.ketch(args);
        assert!(
            !out.status.success(),
            "`ketch {}` was expected to fail but succeeded\n--- stdout ---\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
        );
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    /// Publish the releases the test plugin will serve for `id`.
    pub fn publish(&self, id: &str, releases: &[Release]) {
        let json: Vec<String> = releases.iter().map(Release::to_json).collect();
        std::fs::write(
            self.assets().join(format!("{id}.releases.json")),
            format!("[{}]", json.join(",")),
        )
        .expect("write releases");
    }

    /// Build a release asset on disk and describe it for the plugin.
    pub fn asset(&self, name: &str, archive: Archive) -> Asset {
        let path = self.assets().join(name);
        archive.write_to(&path);
        let sha256 = sha256_file(&path);
        Asset {
            name: name.to_string(),
            path,
            digest: Some(sha256),
        }
    }

    fn write_plugin(&self) {
        let script = format!(
            "#!/bin/sh\n\
             set -eu\n\
             DB={db}\n\
             case \"$1\" in\n\
             capabilities) printf '%s' '{caps}' ;;\n\
             describe) printf 'null' ;;\n\
             releases) cat \"$DB/$2.releases.json\" ;;\n\
             search) printf '[]' ;;\n\
             download) cp \"$2\" \"$3\" ;;\n\
             *) echo \"unsupported subcommand: $1\" >&2; exit 1 ;;\n\
             esac\n",
            db = shell_quote(&self.assets()),
            caps = r#"{"protocol":1,"scheme":"test","download":true,"search":false}"#,
        );
        let path = self.plugin_dir().join("ketch-source-test");
        std::fs::write(&path, script).expect("write plugin");
        make_executable(&path);
    }
}

/// One release the plugin will report.
pub struct Release {
    version: String,
    assets: Vec<Asset>,
}

impl Release {
    pub fn new(version: &str, assets: Vec<Asset>) -> Release {
        Release {
            version: version.to_string(),
            assets,
        }
    }

    fn to_json(&self) -> String {
        let assets: Vec<String> = self.assets.iter().map(Asset::to_json).collect();
        format!(
            r#"{{"version":"{v}","tag":"v{v}","prerelease":false,"draft":false,"assets":[{a}]}}"#,
            v = self.version,
            a = assets.join(",")
        )
    }
}

/// A fixture asset: a real file on disk, plus the digest the plugin publishes.
#[derive(Clone)]
pub struct Asset {
    name: String,
    path: PathBuf,
    digest: Option<String>,
}

impl Asset {
    /// Publish a digest that does not match the bytes, so the install is
    /// rejected the way a tampered download would be.
    pub fn with_wrong_digest(mut self) -> Asset {
        self.digest = Some("0".repeat(64));
        self
    }

    fn to_json(&self) -> String {
        let digest = match &self.digest {
            Some(hex) => format!(r#","digest":{{"algo":"sha256","hex":"{hex}"}}"#),
            None => String::new(),
        };
        // The plugin downloads, so `url` is just the path it copies from.
        format!(
            r#"{{"name":"{}","url":"{}"{}}}"#,
            self.name,
            self.path.display(),
            digest
        )
    }
}

/// One file inside a fixture archive.
pub struct Entry {
    path: String,
    body: Vec<u8>,
    mode: u32,
}

impl Entry {
    pub fn file(path: &str, body: &str) -> Entry {
        Entry {
            path: path.to_string(),
            body: body.as_bytes().to_vec(),
            mode: 0o644,
        }
    }

    /// An executable that prints `says` when run, so a test can prove the thing
    /// on PATH is the thing that was installed.
    pub fn program(path: &str, says: &str) -> Entry {
        Entry {
            path: path.to_string(),
            body: format!("#!/bin/sh\necho '{says}'\n").into_bytes(),
            mode: 0o755,
        }
    }
}

/// A fixture archive, in one of the formats ketch sniffs for.
pub enum Archive {
    TarGz(Vec<Entry>),
    Zip(Vec<Entry>),
}

impl Archive {
    fn write_to(self, dest: &Path) {
        match self {
            Archive::TarGz(entries) => write_tar_gz(dest, &entries),
            Archive::Zip(entries) => write_zip(dest, &entries),
        }
    }
}

fn write_tar_gz(dest: &Path, entries: &[Entry]) {
    let file = std::fs::File::create(dest).expect("create tarball");
    let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut tar = tar::Builder::new(gz);
    for entry in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(entry.body.len() as u64);
        header.set_mode(entry.mode);
        header.set_cksum();
        tar.append_data(&mut header, &entry.path, entry.body.as_slice())
            .expect("append");
    }
    tar.into_inner()
        .expect("finish tar")
        .finish()
        .expect("gzip");
}

fn write_zip(dest: &Path, entries: &[Entry]) {
    let file = std::fs::File::create(dest).expect("create zip");
    let mut zip = zip::ZipWriter::new(file);
    for entry in entries {
        let options = zip::write::SimpleFileOptions::default().unix_permissions(entry.mode);
        zip.start_file(&entry.path, options).expect("start file");
        zip.write_all(&entry.body).expect("write file");
    }
    zip.finish().expect("finish zip");
}

fn sha256_file(path: &Path) -> String {
    use sha2::Digest;
    let bytes = std::fs::read(path).expect("read asset");
    hex::encode(sha2::Sha256::digest(bytes))
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// Single-quote a path for the plugin script. Temp dirs contain no quotes, but
/// the script is generated rather than hand-written, so it should not assume so.
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// The architecture token naming an asset this machine runs natively.
pub fn host_arch() -> &'static str {
    std::env::consts::ARCH
}
