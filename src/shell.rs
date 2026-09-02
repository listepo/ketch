//! Putting the ketch bin directory on PATH, in the user's own shell.
//!
//! Separate from `platform/` because what has to be edited is a shell's
//! startup file, not an operating system's: bash, zsh and fish read the same
//! files wherever they run, so a Linux or Windows backend inherits all of this
//! unchanged.
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
    if configured.is_empty() {
        return DoctorCheck::fail(
            "PATH",
            format!("{bin} is not on PATH"),
            "Run `ketch path install`, or `ketch doctor --fix`.",
        );
    }
    let files: Vec<String> = configured.iter().map(|p| p.display().to_string()).collect();
    DoctorCheck::warn(
        "PATH",
        format!(
            "{bin} is set up in {} but not in this shell",
            files.join(", ")
        ),
        "Open a new shell.",
    )
}

/// The line to add by hand, for a shell ketch does not know.
pub fn manual_line(cfg: &Config) -> Result<String> {
    Ok(Shell::Bash.export(bin_dir_str(cfg)?))
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
/// or ketch's own markers, does not read as configured.
fn mentions(text: &str, bin_dir: &str) -> bool {
    text.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .any(|line| line.contains(bin_dir))
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
}
