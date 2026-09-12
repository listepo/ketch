//! Putting the ketch bin directory on PATH.
//!
//! On Unix that means a block in bash, zsh or fish startup files. On Windows
//! it means the user environment (`HKCU\\Environment\\Path`); the same shell
//! edits still exist for Git Bash.
//!
//! This is the only code in ketch that writes outside the ketch root, and it
//! runs only when the user asks for it — `ketch path install`, or
//! `ketch doctor --fix`. Everything it adds sits between two markers so it can
//! be found again, rewritten in place, and taken back out without guessing.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::platform::DoctorCheck;
use std::path::{Path, PathBuf};

/// Opens the block ketch owns. Must begin a line and end one.
const BEGIN: &str = "# >>> ketch >>>";
/// Closes the block ketch owns.
const END: &str = "# <<< ketch <<<";

/// A shell whose PATH ketch knows how to set up.
///
/// Anything else is handled by printing the line to add by hand: a shell whose
/// quoting rules are not implemented here would be edited wrongly, and a
/// broken startup file costs the user more than a manual paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// Every shell ketch can configure.
    pub const ALL: [Shell; 3] = [Shell::Bash, Shell::Zsh, Shell::Fish];

    /// The name the user types, and the one `$SHELL` ends with.
    pub fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
        }
    }

    /// The shell a program path refers to, or `None` for one ketch cannot set
    /// up.
    ///
    /// Both the directory and the leading `-` that marks a login shell are
    /// ignored, because `$SHELL` and `argv[0]` disagree about both.
    pub fn from_program(program: &str) -> Option<Shell> {
        let base = program.rsplit('/').next().unwrap_or(program);
        match base.trim_start_matches('-') {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            _ => None,
        }
    }

    /// Files this shell may already be configured in, most preferred first.
    ///
    /// bash is the awkward one: a terminal on macOS starts a login shell,
    /// which reads `.bash_profile` and never `.bashrc`, while most Linux
    /// terminals do the reverse. Ordering by what the host actually starts is
    /// what stops ketch writing into a file nothing reads.
    fn candidates(self, home: &Path) -> Vec<PathBuf> {
        let bash = if cfg!(target_os = "macos") {
            [".bash_profile", ".bashrc"]
        } else {
            [".bashrc", ".bash_profile"]
        };
        match self {
            Shell::Bash => bash.iter().map(|f| home.join(f)).collect(),
            Shell::Zsh => vec![zdotdir(home).join(".zshrc")],
            Shell::Fish => vec![config_home(home).join("fish").join("config.fish")],
        }
    }

    /// The file to edit: the first candidate that already exists, else the one
    /// this host would create.
    pub fn config_file(self, home: &Path) -> PathBuf {
        let candidates = self.candidates(home);
        candidates
            .iter()
            .find(|p| p.is_file())
            .or_else(|| candidates.first())
            .cloned()
            // Unreachable while `candidates` returns a non-empty list, and a
            // sane answer rather than a panic if it ever stops.
            .unwrap_or_else(|| home.join(".profile"))
    }

    /// The one line that does the work.
    fn export(self, bin_dir: &str) -> String {
        match self {
            // A single-quoted literal joined to "$PATH" keeps every character
            // of the directory: a path holding a space, a `$` or a quote still
            // expands to exactly itself.
            Shell::Bash | Shell::Zsh => format!("export PATH={}:\"$PATH\"", quote_posix(bin_dir)),
            // Deliberately not `fish_add_path`: that writes a universal
            // variable, which outlives the file ketch is editing and would
            // survive `ketch path uninstall`.
            Shell::Fish => format!("set -gx PATH {} $PATH", quote_fish(bin_dir)),
        }
    }

    /// The whole block ketch owns, markers included.
    fn block(self, bin_dir: &str) -> String {
        format!("{BEGIN}\n{}\n{END}\n", self.export(bin_dir))
    }
}

/// What `install` or `uninstall` did to one shell's config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The block was written for the first time.
    Added,
    /// An existing ketch block named a different directory and was rewritten.
    Updated,
    /// The block was taken out again.
    Removed,
    /// Nothing to do: already correct, or the user had set it up by hand.
    Unchanged,
}

/// One shell's config file, and what happened to it.
#[derive(Debug, Clone)]
pub struct Change {
    pub shell: Shell,
    pub file: PathBuf,
    pub outcome: Outcome,
}

/// The login shell, from `$SHELL`.
pub fn current() -> Option<Shell> {
    let shell = std::env::var("SHELL").ok()?;
    Shell::from_program(&shell)
}

/// Shells worth configuring on this machine: the login shell, plus any whose
/// config file the user already keeps.
///
/// A shell that is neither is left alone. Creating a startup file for a shell
/// nobody runs is litter, and `--shell` says so explicitly when it is wanted.
pub fn detect() -> Result<Vec<Shell>> {
    let home = home()?;
    let current = current();
    Ok(Shell::ALL
        .into_iter()
        .filter(|s| Some(*s) == current || s.candidates(&home).iter().any(|p| p.is_file()))
        .collect())
}

/// Add the block to one shell's config file.
///
/// `dry_run` computes the outcome and writes nothing.
pub fn install(cfg: &Config, shell: Shell, dry_run: bool) -> Result<Change> {
    let bin_dir = bin_dir_str(cfg)?;
    let file = shell.config_file(&home()?);
    let text = read(&file)?;
    let had_block = block_span(&text).is_some();

    // A line the user wrote themselves already does the job. A second copy
    // would be both redundant and impossible to tell from theirs later.
    if !had_block && mentions(&text, bin_dir) {
        return Ok(Change {
            shell,
            file,
            outcome: Outcome::Unchanged,
        });
    }

    let outcome = match splice(&text, &shell.block(bin_dir)) {
        None => Outcome::Unchanged,
        Some(next) => {
            if !dry_run {
                write(&file, &next)?;
            }
            if had_block {
                Outcome::Updated
            } else {
                Outcome::Added
            }
        }
    };
    Ok(Change {
        shell,
        file,
        outcome,
    })
}

/// Take the block back out of one shell's config file, leaving everything the
/// user wrote exactly as it was.
pub fn uninstall(shell: Shell, dry_run: bool) -> Result<Change> {
    let file = shell.config_file(&home()?);
    let text = read(&file)?;
    let outcome = match unsplice(&text) {
        None => Outcome::Unchanged,
        Some(next) => {
            if !dry_run {
                write(&file, &next)?;
            }
            Outcome::Removed
        }
    };
    Ok(Change {
        shell,
        file,
        outcome,
    })
}

/// Every shell startup file holding a ketch block, across all three shells and
/// every file each of them reads.
///
/// Deliberately narrower than `configured_in`: that one answers "is the bin dir
/// on PATH", a line the user wrote by hand included. This one finds only blocks
/// ketch wrote, because those are the only ones it may take back out.
pub fn files_with_block() -> Vec<PathBuf> {
    let Ok(home) = home() else {
        return Vec::new();
    };
    Shell::ALL
        .into_iter()
        .flat_map(|s| s.candidates(&home))
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|t| block_span(&t).is_some())
                .unwrap_or(false)
        })
        .collect()
}

/// Take the ketch block out of one file named by path. True when it changed.
///
/// The by-path entry point, for `ketch self uninstall`: it removes every block
/// it can find rather than the file one named shell happens to read now, since
/// a shell the user has since stopped using still has ketch in its startup.
pub fn uninstall_file(file: &Path) -> Result<bool> {
    match unsplice(&read(file)?) {
        None => Ok(false),
        Some(next) => {
            write(file, &next)?;
            Ok(true)
        }
    }
}

/// Config files that already put the bin dir on PATH, whether ketch wrote them
/// or the user did.
///
/// An unreadable file is not configured as far as anyone can tell, so it is
/// skipped rather than reported: this feeds a diagnostic, not a decision.
pub fn configured_in(cfg: &Config) -> Vec<PathBuf> {
    let (Ok(home), Ok(bin_dir)) = (home(), bin_dir_str(cfg)) else {
        return Vec::new();
    };
    Shell::ALL
        .into_iter()
        .flat_map(|s| s.candidates(&home))
        .filter(|p| {
            std::fs::read_to_string(p)
                .map(|t| mentions(&t, bin_dir))
                .unwrap_or(false)
        })
        .collect()
}

/// The PATH line of `ketch doctor`.
///
/// Three states, not two. A bin dir that is written into `.zshrc` but missing
/// from this process's environment is not broken — the shell that started
/// ketch simply predates the edit — and calling that a failure sends the user
/// round the same loop forever.
pub fn path_check(cfg: &Config) -> DoctorCheck {
    let bin = cfg.bin_dir.display().to_string();
    if cfg.bin_dir_on_path() {
        return DoctorCheck::ok("PATH", format!("{bin} is on PATH"));
    }
    let configured = configured_in(cfg);
    let user = user_path_configured(cfg);
    if configured.is_empty() && !user {
        return DoctorCheck::fail(
            "PATH",
            format!("{bin} is not on PATH"),
            "Run `ketch path install`, or `ketch doctor --fix`.",
        );
    }
    let mut places: Vec<String> = configured.iter().map(|p| p.display().to_string()).collect();
    if user {
        places.push("the user PATH".to_string());
    }
    DoctorCheck::warn(
        "PATH",
        format!(
            "{bin} is set up in {} but not in this shell",
            places.join(", ")
        ),
        "Open a new shell.",
    )
}

/// The line to add by hand, for a shell ketch does not know.
pub fn manual_line(cfg: &Config) -> Result<String> {
    let bin = bin_dir_str(cfg)?;
    if cfg!(windows) {
        Ok(format!("Add {bin} to your user PATH."))
    } else {
        Ok(Shell::Bash.export(bin))
    }
}

/// True when the Windows user PATH already names the bin dir.
///
/// Off Windows this is always false: there is no user environment ketch
/// owns. An unreadable registry is treated as not configured, like an
/// unreadable startup file.
pub fn user_path_configured(cfg: &Config) -> bool {
    #[cfg(windows)]
    {
        read_user_path()
            .ok()
            .is_some_and(|path| windows_path_has(&path, &cfg.bin_dir))
    }
    #[cfg(not(windows))]
    {
        let _ = cfg;
        false
    }
}

/// Put the bin dir on the Windows user PATH.
///
/// Uses `[Environment]::SetEnvironmentVariable` so Explorer is notified and a
/// new terminal sees the change without a logoff. `setx` is not used: it
/// truncates PATH at 1024 characters.
#[cfg(windows)]
pub fn install_user(cfg: &Config, dry_run: bool) -> Result<Outcome> {
    let current = read_user_path()?;
    match windows_path_prepend(&current, &cfg.bin_dir) {
        None => Ok(Outcome::Unchanged),
        Some(next) => {
            if !dry_run {
                write_user_path(&next)?;
            }
            Ok(Outcome::Added)
        }
    }
}

/// Take the bin dir back out of the Windows user PATH.
#[cfg(windows)]
pub fn uninstall_user(cfg: &Config, dry_run: bool) -> Result<Outcome> {
    let current = read_user_path()?;
    match windows_path_remove(&current, &cfg.bin_dir) {
        None => Ok(Outcome::Unchanged),
        Some(next) => {
            if !dry_run {
                write_user_path(&next)?;
            }
            Ok(Outcome::Removed)
        }
    }
}

/// Whether `dir` already appears as an entry in a Windows PATH string.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_path_has(path: &str, dir: &Path) -> bool {
    windows_path_entries(path).any(|entry| windows_path_eq(entry, dir))
}

/// Prepend `dir` if it is missing. `None` when it is already present.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_path_prepend(path: &str, dir: &Path) -> Option<String> {
    if windows_path_has(path, dir) {
        return None;
    }
    let dir = dir.to_string_lossy();
    if path.is_empty() {
        Some(dir.into_owned())
    } else {
        Some(format!("{dir};{path}"))
    }
}

/// Drop `dir` if it is present. `None` when it was not there.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_path_remove(path: &str, dir: &Path) -> Option<String> {
    if !windows_path_has(path, dir) {
        return None;
    }
    let kept: Vec<&str> = windows_path_entries(path)
        .filter(|entry| !windows_path_eq(entry, dir))
        .collect();
    Some(kept.join(";"))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_path_entries(path: &str) -> impl Iterator<Item = &str> {
    path.split(';').filter(|s| !s.is_empty())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_path_eq(entry: &str, dir: &Path) -> bool {
    windows_path_key(Path::new(entry)) == windows_path_key(dir)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    s.trim_end_matches('\\').to_ascii_lowercase()
}

#[cfg(windows)]
fn read_user_path() -> Result<String> {
    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','User')",
        ])
        .output()
        .map_err(|e| Error::msg(format!("could not read the user PATH: {e}")))?;
    if !out.status.success() {
        return Err(Error::msg(format!(
            "could not read the user PATH: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim_end_matches(['\r', '\n'])
        .to_string())
}

/// Write the user Path and broadcast `WM_SETTINGCHANGE`.
///
/// The new value travels in an environment variable so a PATH that contains
/// quotes or `$` cannot break out of the PowerShell command.
#[cfg(windows)]
fn write_user_path(value: &str) -> Result<()> {
    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::SetEnvironmentVariable('Path', $env:KETCH_NEW_USER_PATH, 'User')",
        ])
        .env("KETCH_NEW_USER_PATH", value)
        .status()
        .map_err(|e| Error::msg(format!("could not write the user PATH: {e}")))?;
    if !status.success() {
        return Err(Error::msg("could not write the user PATH"));
    }
    Ok(())
}

fn home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| Error::msg("no home directory; set HOME"))
}

/// zsh reads its files from `$ZDOTDIR` when that is set, and only falls back to
/// the home directory when it is not.
fn zdotdir(home: &Path) -> PathBuf {
    std::env::var_os("ZDOTDIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.to_path_buf())
}

fn config_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
}

/// The bin dir as something a shell file can hold.
///
/// Both failures here are ones no amount of quoting fixes, so they are refused
/// rather than written and hoped for.
fn bin_dir_str(cfg: &Config) -> Result<&str> {
    let text = cfg.bin_dir.to_str().ok_or_else(|| {
        Error::msg(format!(
            "{} is not valid UTF-8, so it cannot be written into a shell config",
            cfg.bin_dir.display()
        ))
    })?;
    if text.contains('\n') {
        return Err(Error::msg(format!(
            "{} contains a newline; no shell can express that on one line",
            cfg.bin_dir.display()
        )));
    }
    Ok(text)
}

/// Single-quote for the POSIX family, where the only character that cannot
/// appear inside single quotes is the single quote itself.
fn quote_posix(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Single-quote for fish, which unlike POSIX honours backslash escapes inside
/// single quotes — so a literal backslash has to be doubled.
fn quote_fish(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// True when some line the shell will actually run names this directory.
///
/// Comments are skipped so that a file still carrying a commented-out attempt,
/// or ketch's own markers, does not read as configured. The name has to stand
/// on its own, too: `contains` alone accepts `…/.ketch/bin.bak`, a backup of
/// the file — `path install` would report the directory as already set up and
/// add nothing, and `doctor` would point at a new shell that still lacks it.
fn mentions(text: &str, bin_dir: &str) -> bool {
    // What can sit next to a directory on `PATH`: a separator, a quote, a
    // space. Another path character — the `.` of `.bak`, the `/` of a longer
    // path — means this is a different directory that merely starts the same.
    let boundary = |byte: u8| {
        byte.is_ascii_whitespace() || matches!(byte, b':' | b'"' | b'\'' | b'=' | b'(' | b')')
    };
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .any(|line| {
            let bytes = line.as_bytes();
            line.match_indices(bin_dir).any(|(start, matched)| {
                let end = start + matched.len();
                (start == 0 || boundary(bytes[start - 1]))
                    && (end == bytes.len() || boundary(bytes[end]))
            })
        })
}

/// Byte range of the ketch block, markers and trailing newline included.
fn block_span(text: &str) -> Option<(usize, usize)> {
    let start = marker_at_line_start(text, BEGIN, 0)?;
    let end_line = marker_at_line_start(text, END, start + BEGIN.len())?;
    let mut end = end_line + END.len();
    if text[end..].starts_with('\n') {
        end += 1;
    }
    Some((start, end))
}

/// Offset of `marker` where it occupies a whole line, at or after `from`.
///
/// Whole-line matching is what keeps a marker quoted inside somebody's own
/// script from being mistaken for the block ketch owns.
fn marker_at_line_start(text: &str, marker: &str, from: usize) -> Option<usize> {
    text[from..]
        .match_indices(marker)
        .map(|(offset, _)| from + offset)
        .find(|&i| {
            let starts_line = i == 0 || text.as_bytes()[i - 1] == b'\n';
            let ends_line = text[i + marker.len()..]
                .chars()
                .next()
                .is_none_or(|c| c == '\n');
            starts_line && ends_line
        })
}

/// Put `block` into `text`, replacing any block already there. `None` means
/// the file already says exactly this.
fn splice(text: &str, block: &str) -> Option<String> {
    match block_span(text) {
        Some((start, end)) => {
            let next = format!("{}{block}{}", &text[..start], &text[end..]);
            (next != text).then_some(next)
        }
        None => {
            let mut next = String::from(text);
            if !next.is_empty() {
                if !next.ends_with('\n') {
                    next.push('\n');
                }
                next.push('\n');
            }
            next.push_str(block);
            Some(next)
        }
    }
}

/// Take the block out. `None` means there was none.
fn unsplice(text: &str) -> Option<String> {
    let (start, end) = block_span(text)?;
    let head = &text[..start];
    // The blank line that was inserted ahead of the block goes back out with
    // it, so installing and uninstalling repeatedly cannot grow the file.
    let head = head
        .strip_suffix('\n')
        .filter(|h| h.ends_with('\n'))
        .unwrap_or(head);
    Some(format!("{head}{}", &text[end..]))
}

/// A missing config file reads as empty: it is about to be created.
fn read(file: &Path) -> Result<String> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(Error::io(file, e)),
    }
}

/// Replace the file's contents, atomically and in place.
fn write(file: &Path, text: &str) -> Result<()> {
    // A startup file is very often a symlink into a dotfiles repository.
    // Renaming over the link would replace it with a regular file and quietly
    // detach the user from their own dotfiles, so the write follows it first.
    let target = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    let Some(parent) = target.parent() else {
        return Err(Error::msg(format!(
            "{} has no parent directory",
            target.display()
        )));
    };
    std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;

    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "profile".to_string());
    let tmp = target.with_file_name(format!(".{name}.ketch-tmp"));

    std::fs::write(&tmp, text).map_err(|e| Error::io(&tmp, e))?;
    // A startup file the user made private must not come back world-readable.
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, &target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        Error::io(&target, e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = "/home/u/.ketch/bin";

    #[test]
    fn a_backup_path_that_starts_with_the_bin_dir_is_not_a_path_entry() {
        assert!(!mentions(
            "export PATH=\"/home/u/.ketch/bin.bak:$PATH\"",
            BIN
        ));
        assert!(mentions("export PATH=\"/home/u/.ketch/bin:$PATH\"", BIN));
        assert!(mentions("set -gx PATH /home/u/.ketch/bin $PATH", BIN));
        assert!(mentions("export PATH=/home/u/.ketch/bin", BIN));
        // A longer path is a different directory that merely starts the same.
        assert!(!mentions(
            "export PATH=\"/home/u/.ketch/bin/tools:$PATH\"",
            BIN
        ));
    }

    fn zsh_block() -> String {
        Shell::Zsh.block(BIN)
    }

    #[test]
    fn a_login_shell_argv_name_still_identifies_the_shell() {
        assert_eq!(Shell::from_program("-zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::from_program("/bin/bash"), Some(Shell::Bash));
        assert_eq!(
            Shell::from_program("/opt/homebrew/bin/fish"),
            Some(Shell::Fish)
        );
        assert_eq!(Shell::from_program("/usr/bin/tcsh"), None);
        assert_eq!(Shell::from_program(""), None);
    }

    #[test]
    fn a_quote_in_the_path_cannot_end_the_quoting() {
        let line = Shell::Zsh.export("/home/o'brien/.ketch/bin");
        assert_eq!(line, "export PATH='/home/o'\\''brien/.ketch/bin':\"$PATH\"");
    }

    #[test]
    fn fish_escapes_the_backslash_that_posix_leaves_alone() {
        assert_eq!(quote_posix("a\\b"), "'a\\b'");
        assert_eq!(quote_fish("a\\b"), "'a\\\\b'");
        assert_eq!(quote_fish("o'brien"), "'o\\'brien'");
    }

    #[test]
    fn a_dollar_in_the_path_is_not_expanded() {
        assert!(Shell::Bash
            .export("/home/$USER/bin")
            .contains("'/home/$USER/bin'"));
    }

    #[test]
    fn the_block_is_added_once_and_then_left_alone() {
        let first = splice("# mine\n", &zsh_block()).expect("first write");
        assert!(first.starts_with("# mine\n\n"));
        assert!(first.contains(BIN));
        assert_eq!(splice(&first, &zsh_block()), None);
    }

    #[test]
    fn a_moved_bin_dir_rewrites_the_block_in_place() {
        let before = splice("# mine\n", &zsh_block()).expect("first write");
        let after = splice(&before, &Shell::Zsh.block("/elsewhere/bin")).expect("rewrite");
        assert!(after.contains("/elsewhere/bin"));
        assert!(!after.contains(BIN));
        assert_eq!(after.matches(BEGIN).count(), 1);
    }

    #[test]
    fn removing_the_block_restores_the_file_byte_for_byte() {
        let original = "# mine\nexport EDITOR=vi\n";
        let with = splice(original, &zsh_block()).expect("write");
        assert_eq!(unsplice(&with).as_deref(), Some(original));
    }

    #[test]
    fn removing_a_block_that_was_never_there_changes_nothing() {
        assert_eq!(unsplice("# mine\n"), None);
    }

    #[test]
    fn install_and_uninstall_cannot_grow_the_file() {
        let original = "# mine\n";
        let mut text = original.to_string();
        for _ in 0..3 {
            text = splice(&text, &zsh_block()).unwrap_or(text);
            text = unsplice(&text).unwrap_or(text);
        }
        assert_eq!(text, original);
    }

    #[test]
    fn a_marker_that_is_not_a_whole_line_is_not_the_block() {
        // Somebody's own script that merely prints the marker.
        let text = format!("echo \"{BEGIN} here\"\n{END} trailing\n");
        assert_eq!(block_span(&text), None);
    }

    #[test]
    fn a_block_at_the_very_start_of_a_file_is_found() {
        let text = format!("{}rest\n", zsh_block());
        assert_eq!(unsplice(&text).as_deref(), Some("rest\n"));
    }

    #[test]
    fn a_path_the_user_added_by_hand_counts_as_configured() {
        assert!(mentions("export PATH=\"/home/u/.ketch/bin:$PATH\"\n", BIN));
    }

    #[test]
    fn a_commented_out_line_does_not_count_as_configured() {
        assert!(!mentions(
            "  # export PATH=\"/home/u/.ketch/bin:$PATH\"\n",
            BIN
        ));
        assert!(!mentions("", BIN));
    }

    #[test]
    fn a_file_without_a_trailing_newline_still_gets_a_clean_block() {
        let text = splice("# mine", &zsh_block()).expect("write");
        assert!(text.starts_with("# mine\n\n"));
        assert!(text.ends_with(&format!("{END}\n")));
    }

    #[test]
    fn an_empty_file_gets_the_block_with_no_leading_blank_line() {
        let text = splice("", &zsh_block()).expect("write");
        assert!(text.starts_with(BEGIN));
    }

    #[test]
    fn each_shell_gets_the_syntax_it_can_actually_run() {
        assert!(Shell::Bash.export(BIN).starts_with("export PATH="));
        assert!(Shell::Zsh.export(BIN).starts_with("export PATH="));
        assert_eq!(
            Shell::Fish.export(BIN),
            "set -gx PATH '/home/u/.ketch/bin' $PATH"
        );
    }

    #[test]
    fn bash_prefers_a_file_the_host_actually_reads() {
        let home = std::path::Path::new("/home/u");
        let first = Shell::Bash.candidates(home).remove(0);
        if cfg!(target_os = "macos") {
            assert!(first.ends_with(".bash_profile"));
        } else {
            assert!(first.ends_with(".bashrc"));
        }
    }

    #[test]
    fn windows_path_prepend_is_idempotent_and_slash_insensitive() {
        let dir = Path::new(r"C:\Users\u\.ketch\bin");
        let added = windows_path_prepend(r"C:\Windows\System32", dir).unwrap();
        assert!(added.starts_with(r"C:\Users\u\.ketch\bin;"));
        assert!(windows_path_prepend(&added, dir).is_none());
        assert!(windows_path_has(
            &added,
            Path::new("C:/Users/u/.ketch/bin/")
        ));
    }

    #[test]
    fn windows_path_remove_drops_only_the_named_entry() {
        let dir = Path::new(r"C:\Users\u\.ketch\bin");
        let path = r"C:\Windows\System32;C:\Users\u\.ketch\bin;C:\Windows";
        assert_eq!(
            windows_path_remove(path, dir).as_deref(),
            Some(r"C:\Windows\System32;C:\Windows")
        );
        assert!(windows_path_remove(r"C:\Windows\System32", dir).is_none());
    }
}
