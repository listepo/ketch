//! macOS container formats.
//!
//! Both shell out to the system tools, because reimplementing HFS+/APFS image
//! reading or Apple's flat-package format would be a large amount of code with
//! no upside — `hdiutil` and `pkgutil` ship on every Mac.

use super::Extractor;
use crate::error::{Error, Result};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// `.dmg` — attached read-only with no browse and no auto-open, contents
/// copied out, then detached even if the copy failed.
pub struct DmgExtractor;

/// `.pkg` / `.mpkg` — expanded and unpacked into the store rather than
/// installed system-wide, so ketch stays the only thing that owns the files and
/// `ketch uninstall` can actually undo it.
pub struct PkgExtractor;

/// Run a system tool and return its stdout.
///
/// stdin is closed so a tool that decides to prompt (a DMG with a licence
/// agreement, say) fails fast instead of hanging the install forever.
pub fn run_tool(program: &str, args: &[&OsStr]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::io(Path::new(program), e))?;
    if !output.status.success() {
        return Err(Error::Command {
            cmd: format!("{program} {}", render_args(args)),
            status: output
                .status
                .code()
                .map(|c| format!("exit {c}"))
                .unwrap_or_else(|| "killed by signal".to_string()),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Same, but a non-zero exit is reported rather than fatal. Used for the
/// best-effort cleanup paths where failing would lose the real error.
pub fn try_tool(program: &str, args: &[&OsStr]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn render_args(args: &[&OsStr]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `ditto` rather than a hand-rolled copy: it preserves extended attributes,
/// symlinks and resource forks, and a `.app` whose code signature is broken by
/// a naive copy will refuse to launch.
pub fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    run_tool("/usr/bin/ditto", &[src.as_os_str(), dest.as_os_str()]).map(|_| ())
}

/// The 512-byte `koly` trailer that closes every Apple disk image.
fn has_koly_trailer(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::End(-512)).is_err() {
        return false;
    }
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic).is_ok() && &magic == b"koly"
}

fn has_extension(path: &Path, want: &[&str]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| want.contains(&e.as_str()))
}

impl Extractor for DmgExtractor {
    fn id(&self) -> &str {
        "dmg"
    }

    fn detect(&self, path: &Path, _head: &[u8]) -> bool {
        has_koly_trailer(path) || has_extension(path, &["dmg"])
    }

    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let mount_root = tempfile::tempdir().map_err(|e| Error::io(dest, e))?;
        let listing = run_tool(
            "/usr/bin/hdiutil",
            &[
                OsStr::new("attach"),
                src.as_os_str(),
                OsStr::new("-nobrowse"),
                OsStr::new("-noautoopen"),
                OsStr::new("-readonly"),
                OsStr::new("-noverify"),
                OsStr::new("-mountrandom"),
                mount_root.path().as_os_str(),
            ],
        )?;

        let Some(mount) = find_mount_point(&listing) else {
            // The image is attached even though nothing mounted, and the mount
            // point is the handle we would normally detach by. The device node
            // is the only one left; without it the image stays attached for the
            // rest of the session with nothing pointing at it.
            if let Some(device) = find_device(&listing) {
                try_tool(
                    "/usr/bin/hdiutil",
                    &[OsStr::new("detach"), &device, OsStr::new("-force")],
                );
            }
            return Err(Error::msg(format!(
                "hdiutil attached {} but reported no mount point",
                src.display()
            )));
        };

        // Everything below must run whether or not the copy worked, or the
        // image stays attached for the rest of the session.
        let copied = copy_volume(&mount, dest);
        try_tool(
            "/usr/bin/hdiutil",
            &[
                OsStr::new("detach"),
                mount.as_os_str(),
                OsStr::new("-force"),
            ],
        );
        copied
    }
}

/// Pull the mount point out of `hdiutil attach` output.
///
/// Lines are tab-separated `device \t type \t mountpoint`, and only some
/// partitions are mounted at all, so the mount point is the last field of the
/// last line that has one.
fn find_mount_point(listing: &str) -> Option<PathBuf> {
    listing
        .lines()
        .filter_map(|line| line.split('\t').next_back())
        .map(str::trim)
        .filter(|field| field.starts_with('/'))
        .map(PathBuf::from)
        .find(|path| path.is_dir())
}

/// The device node `hdiutil attach` created, from the first line that names one.
///
/// Detaching by device takes the whole image down, partitions included, which
/// is exactly what is wanted when none of them mounted.
fn find_device(listing: &str) -> Option<std::ffi::OsString> {
    listing
        .lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::trim)
        .find(|field| field.starts_with("/dev/"))
        .map(std::ffi::OsString::from)
}

fn copy_volume(mount: &Path, dest: &Path) -> Result<()> {
    let entries = std::fs::read_dir(mount).map_err(|e| Error::io(mount, e))?;
    let mut copied_any = false;

    for entry in entries {
        let entry = entry.map_err(|e| Error::io(mount, e))?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Volume bookkeeping and the customary `/Applications` drop-target.
        if name_str.starts_with('.') {
            continue;
        }
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
            continue;
        }
        copy_tree(&entry.path(), &dest.join(name.as_os_str()))?;
        copied_any = true;
    }

    if !copied_any {
        return Err(Error::EmptyPayload(mount.to_path_buf()));
    }
    Ok(())
}

impl Extractor for PkgExtractor {
    fn id(&self) -> &str {
        "pkg"
    }

    fn detect(&self, path: &Path, head: &[u8]) -> bool {
        head.starts_with(b"xar!") || has_extension(path, &["pkg", "mpkg"])
    }

    fn extract(&self, src: &Path, dest: &Path) -> Result<()> {
        let work = tempfile::tempdir().map_err(|e| Error::io(dest, e))?;
        // `pkgutil` insists the destination not exist yet.
        let expanded = work.path().join("expanded");
        run_tool(
            "/usr/sbin/pkgutil",
            &[
                OsStr::new("--expand-full"),
                src.as_os_str(),
                expanded.as_os_str(),
            ],
        )?;

        let payloads = find_payload_roots(&expanded);
        if payloads.is_empty() {
            return Err(Error::msg(format!(
                "{} expanded but contained no payload",
                src.display()
            )));
        }
        for payload in payloads {
            for entry in std::fs::read_dir(&payload).map_err(|e| Error::io(&payload, e))? {
                let entry = entry.map_err(|e| Error::io(&payload, e))?;
                copy_tree(&entry.path(), &dest.join(entry.file_name()))?;
            }
        }
        Ok(())
    }
}

/// Find the directories holding the actual files.
///
/// `pkgutil --expand-full` writes one `<component>.pkg/Payload/` per component;
/// older layouts call it `Root`. Everything else in the expansion is install
/// metadata that has no meaning outside the system installer.
fn find_payload_roots(expanded: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(expanded)
        .max_depth(4)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .filter(|e| matches!(e.file_name().to_str(), Some("Payload") | Some("Root")))
        .map(|e| e.into_path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_mount_point_out_of_hdiutil_output() {
        let root = tempfile::tempdir().unwrap();
        let mount = root.path().join("dmg.T0hgqZ");
        std::fs::create_dir_all(&mount).unwrap();

        let listing = format!(
            "/dev/disk4          \tGUID_partition_scheme\t\n\
             /dev/disk4s1        \tApple_HFS            \t{}\n",
            mount.display()
        );
        assert_eq!(find_mount_point(&listing), Some(mount));
    }

    #[test]
    fn ignores_partitions_with_no_mount_point() {
        assert_eq!(
            find_mount_point("/dev/disk4\tGUID_partition_scheme\t\n"),
            None
        );
    }

    #[test]
    fn an_image_that_mounts_nothing_can_still_be_detached() {
        // Nothing mounted, so the device node is the only handle left for
        // detaching an image that is nonetheless attached.
        let listing = "/dev/disk4\tGUID_partition_scheme\t\n\
                       /dev/disk4s1\tApple_HFS\t\n";
        assert_eq!(find_mount_point(listing), None);
        assert_eq!(find_device(listing).unwrap(), "/dev/disk4");
    }
}
