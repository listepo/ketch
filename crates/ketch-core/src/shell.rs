// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

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
//!
//! On Windows it also switches completion on, which `self install` and
//! `ketch completions --install` ask for: a block in the PowerShell profiles
//! that dot-sources the completion script, and doskey macros for cmd.exe
//! loaded through `Command Processor\AutoRun`. Both are taken back out by
//! `ketch self uninstall`.

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
    /// ignored, because `$SHELL` and `argv[0]` disagree about both. A Windows
    /// `bash.exe` (and a `C:\…\bash.exe` path) must still resolve: Git for
    /// Windows puts `.exe` on `$SHELL` / `argv[0]`, and `Path` on Unix would
    /// treat a backslash path as one component.
    pub fn from_program(program: &str) -> Option<Shell> {
        let base = program.rsplit(['/', '\\']).next().unwrap_or(program);
        let stem = base.trim_start_matches('-');
        // Windows preserves arbitrary `.Exe` / `.eXe` casing from PATH / argv[0];
        // only stripping `.exe`/`.EXE` left Git Bash etc. undetected.
        let stem = if stem.len() >= 4 && stem[stem.len() - 4..].eq_ignore_ascii_case(".exe") {
            &stem[..stem.len() - 4]
        } else {
            stem
        };
        if stem.eq_ignore_ascii_case("bash") {
            Some(Shell::Bash)
        } else if stem.eq_ignore_ascii_case("zsh") {
            Some(Shell::Zsh)
        } else if stem.eq_ignore_ascii_case("fish") {
            Some(Shell::Fish)
        } else {
            None
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

/// A value ketch writes into the Windows registry.
///
/// `self uninstall` removes every entry [`registry_entries`] finds, so a new
/// value ketch starts writing is added here rather than as one more step in
/// the uninstall — one that a later change could forget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryEntry {
    /// The bin dir in `HKCU\Environment\Path`, written by `install.ps1` and
    /// `ketch path install`.
    UserPath,
    /// The command in `HKCU\Software\Microsoft\Command Processor\AutoRun`
    /// that loads ketch's doskey macros into every cmd.
    CmdMacros,
}

impl RegistryEntry {
    /// How the entry is named when the user is asked to remove it.
    pub fn describe(self) -> &'static str {
        match self {
            Self::UserPath => "the user PATH",
            Self::CmdMacros => {
                r"the cmd macros in HKCU\Software\Microsoft\Command Processor\AutoRun"
            }
        }
    }

    /// True when the packages ketch installed rely on the entry, so
    /// `self uninstall --keep-packages` leaves it; false when it only loads
    /// ketch itself.
    pub fn serves_packages(self) -> bool {
        match self {
            Self::UserPath => true,
            Self::CmdMacros => false,
        }
    }
}

/// The registry values ketch wrote that are present now. Always empty off
/// Windows.
pub fn registry_entries(cfg: &Config) -> Vec<RegistryEntry> {
    let mut entries = Vec::new();
    if user_path_configured(cfg) {
        entries.push(RegistryEntry::UserPath);
    }
    if cmd_macros_configured(cfg) {
        entries.push(RegistryEntry::CmdMacros);
    }
    entries
}

/// A `ketch doctor` warning for user PATH entries that point into a ketch
/// root that is gone — left by an uninstall that could not reach the
/// registry, or by a root deleted by hand. `None` when there are none, and
/// always off Windows.
pub fn stale_registry_check(cfg: &Config) -> Option<DoctorCheck> {
    #[cfg(windows)]
    {
        let path = read_user_path().ok()?;
        let stale = stale_path_entries(&path, &cfg.bin_dir);
        (!stale.is_empty()).then(|| {
            DoctorCheck::warn(
                "user PATH",
                format!(
                    "names ketch folders that no longer exist: {}",
                    stale.join(", ")
                ),
                "Remove them from the user PATH in System Properties → Environment Variables.",
            )
        })
    }
    #[cfg(not(windows))]
    {
        let _ = cfg;
        None
    }
}

/// Entries of a Windows PATH that are a ketch bin dir whose folder is gone:
/// this root's own, or the `bin` of any `.ketch` root, the name every
/// installer defaults to.
#[cfg_attr(not(windows), allow(dead_code))]
fn stale_path_entries<'a>(path: &'a str, bin_dir: &Path) -> Vec<&'a str> {
    windows_path_entries(path)
        .filter(|entry| {
            let key = windows_path_key(Path::new(entry));
            let ketch_bin = key == windows_path_key(bin_dir) || key.ends_with(r"\.ketch\bin");
            ketch_bin && !Path::new(entry.trim().trim_matches('"')).exists()
        })
        .collect()
}

/// Remove one registry value ketch wrote.
#[cfg(windows)]
pub fn remove_registry_entry(cfg: &Config, entry: RegistryEntry) -> Result<()> {
    match entry {
        RegistryEntry::UserPath => uninstall_user(cfg, false).map(|_| ()),
        RegistryEntry::CmdMacros => uninstall_cmd_macros(cfg).map(|_| ()),
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

/// Split a Windows PATH on `;`, keeping `;` inside quotes as part of one entry.
///
/// Registry PATH values are sometimes quoted. A folder name may contain `;`
/// (`C:\weird;name`), so a naive `split(';')` would shatter `"C:\weird;name"`
/// into two entries and miss the bin dir on doctor/install.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_path_entries(path: &str) -> impl Iterator<Item = &str> {
    windows_path_entry_list(path).into_iter()
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_path_entry_list(path: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    for (i, c) in path.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ';' if !in_quotes => {
                let entry = &path[start..i];
                if !entry.is_empty() {
                    entries.push(entry);
                }
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if start < path.len() && !path[start..].is_empty() {
        entries.push(&path[start..]);
    }
    entries
}

#[cfg_attr(not(windows), allow(dead_code))]
fn windows_path_eq(entry: &str, dir: &Path) -> bool {
    if windows_path_key(Path::new(entry)) == windows_path_key(dir) {
        return true;
    }
    // The same folder spelled another way: `install.ps1` writes the path
    // `Resolve-Path` gives, while the root ketch was started with may be an
    // 8.3 short name (`RUNNER~1`) or go through a link. Folding text cannot
    // see through either; resolving both can, while the folder still exists.
    let entry = entry.trim().trim_matches('"').trim_matches('\'');
    match (dunce::canonicalize(entry), dunce::canonicalize(dir)) {
        (Ok(a), Ok(b)) => windows_path_key(&a) == windows_path_key(&b),
        _ => false,
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub fn windows_path_key(p: &Path) -> String {
    // Registry PATH values are sometimes quoted (`"C:\\Program Files\\…"`).
    // Strip the quotes before folding so doctor/install see the same entry as
    // an unquoted bin dir and do not prepend a duplicate.
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.trim().trim_matches('"').trim_matches('\'');
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

/// The Windows PowerShell editions, each with its own profile directory.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerShell {
    /// PowerShell 7 (`pwsh`), profile in `Documents\PowerShell`.
    Core,
    /// Windows PowerShell 5.1 (`powershell`), profile in
    /// `Documents\WindowsPowerShell`.
    Desktop,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl PowerShell {
    /// Both editions, the current one first.
    pub const ALL: [PowerShell; 2] = [PowerShell::Core, PowerShell::Desktop];

    /// What the user calls it.
    pub fn label(self) -> &'static str {
        match self {
            PowerShell::Core => "PowerShell 7",
            PowerShell::Desktop => "Windows PowerShell",
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    fn exe(self) -> &'static str {
        match self {
            PowerShell::Core => "pwsh",
            PowerShell::Desktop => "powershell",
        }
    }

    /// The CurrentUserAllHosts profile under `documents`: what `$PROFILE.
    /// CurrentUserAllHosts` names, read by the console and every editor host.
    pub fn profile(self, documents: &Path) -> PathBuf {
        let dir = match self {
            PowerShell::Core => "PowerShell",
            PowerShell::Desktop => "WindowsPowerShell",
        };
        documents.join(dir).join("profile.ps1")
    }
}

/// What happened to one PowerShell profile.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone)]
pub struct ProfileChange {
    pub shell: PowerShell,
    pub file: PathBuf,
    /// `Err` carries why the profile was left alone; it is not a failure.
    pub outcome: std::result::Result<Outcome, String>,
}

/// The profile block: dot-source `script` when it is there.
///
/// `Test-Path` first, because the script is a link `self uninstall` removes
/// and a profile that errors on every start is worse than no completion.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn powershell_block(script: &str) -> String {
    let path = quote_powershell(script);
    format!("{BEGIN}\nif (Test-Path -LiteralPath {path}) {{ . {path} }}\n{END}\n")
}

/// Single-quote for PowerShell. It treats the typographic single quotes as
/// quote characters too, so each of those is doubled like `'` itself.
#[cfg_attr(not(windows), allow(dead_code))]
fn quote_powershell(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for c in text.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// `text` with the profile block for `script` in it, or `None` when it
/// already says exactly that.
///
/// Windows PowerShell 5.1 reads a profile without a byte order mark in the
/// ANSI code page, so a new file whose block names a non-ASCII path starts
/// with one. An existing file keeps whatever encoding its owner chose.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn splice_profile(text: &str, script: &str) -> Option<String> {
    let block = powershell_block(script);
    if text.is_empty() && !block.is_ascii() {
        return Some(format!("\u{FEFF}{block}"));
    }
    splice(text, &block)
}

/// Take the block out of a profile. `Some(None)` means nothing but the block
/// (and a byte order mark ketch may have written) was in it, so the file can
/// go: an empty profile still trips an execution policy that forbids scripts.
pub(crate) fn unsplice_profile(text: &str) -> Option<Option<String>> {
    let next = unsplice(text)?;
    if next.trim_start_matches('\u{FEFF}').is_empty() {
        Some(None)
    } else {
        Some(Some(next))
    }
}

/// Add the block that dot-sources `script` to both editions' profiles.
///
/// A profile that does not exist yet is created only when that edition is
/// installed and its execution policy runs local scripts. Windows PowerShell
/// ships with `Restricted` on client editions: a new profile there would put
/// an error on every start and complete nothing.
#[cfg(windows)]
pub fn install_powershell_profiles(script: &Path) -> Result<Vec<ProfileChange>> {
    let script = script.to_str().ok_or_else(|| {
        Error::msg(format!(
            "{} is not valid UTF-8, so no profile can name it",
            script.display()
        ))
    })?;
    if script.contains(['\n', '\r']) {
        return Err(Error::msg(format!("{script} contains a newline")));
    }
    let documents = documents_dir()?;
    let mut changes = Vec::new();
    for shell in PowerShell::ALL {
        let file = shell.profile(&documents);
        let outcome = if file.is_file() {
            Ok(())
        } else {
            match execution_policy(shell) {
                None => Err(format!("{} is not installed", shell.exe())),
                Some(policy) if runs_local_scripts(&policy) => Ok(()),
                Some(policy) => Err(format!(
                    "its execution policy is {policy}; run `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` there and install completions again"
                )),
            }
        };
        let outcome = match outcome {
            Err(why) => Err(why),
            Ok(()) => {
                let text = read(&file)?;
                let had_block = block_span(&text).is_some();
                match splice_profile(&text, script) {
                    None => Ok(Outcome::Unchanged),
                    Some(next) => {
                        write(&file, &next)?;
                        Ok(if had_block {
                            Outcome::Updated
                        } else {
                            Outcome::Added
                        })
                    }
                }
            }
        };
        changes.push(ProfileChange {
            shell,
            file,
            outcome,
        });
    }
    Ok(changes)
}

/// Every PowerShell profile holding a ketch block. Empty off Windows, and
/// when Documents cannot be found.
pub fn powershell_profiles_with_block() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let Ok(documents) = documents_dir() else {
            return Vec::new();
        };
        PowerShell::ALL
            .into_iter()
            .map(|shell| shell.profile(&documents))
            .filter(|p| {
                std::fs::read_to_string(p)
                    .map(|t| block_span(&t).is_some())
                    .unwrap_or(false)
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

/// Take the ketch block out of one PowerShell profile. True when it changed.
/// A profile left with nothing in it is removed, since ketch created it.
pub fn uninstall_powershell_profile(file: &Path) -> Result<bool> {
    match unsplice_profile(&read(file)?) {
        None => Ok(false),
        Some(Some(next)) => {
            write(file, &next)?;
            Ok(true)
        }
        Some(None) => {
            let target = dunce::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
            std::fs::remove_file(&target).map_err(|e| Error::io(&target, e))?;
            Ok(true)
        }
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
fn runs_local_scripts(policy: &str) -> bool {
    ["Unrestricted", "RemoteSigned", "Bypass"]
        .iter()
        .any(|p| policy.trim().eq_ignore_ascii_case(p))
}

/// The effective execution policy of one edition, or `None` when it does not
/// run.
#[cfg(windows)]
fn execution_policy(shell: PowerShell) -> Option<String> {
    powershell(shell.exe(), "Get-ExecutionPolicy", &[])
        .ok()
        .map(|p| p.trim().to_string())
}

/// The Documents folder, as PowerShell itself resolves it.
///
/// Asked of the shell rather than built from the home directory: OneDrive
/// and group policy move Documents, and the profile PowerShell reads is under
/// wherever it went. `DoNotVerify`, because without it Windows PowerShell
/// answers an empty string for a Documents folder not created yet.
#[cfg(windows)]
fn documents_dir() -> Result<PathBuf> {
    let asked = powershell(
        PowerShell::Desktop.exe(),
        "[Console]::Out.Write([Environment]::GetFolderPath('MyDocuments', 'DoNotVerify'))",
        &[],
    )
    .ok()
    .filter(|p| !p.trim().is_empty())
    .map(|p| PathBuf::from(p.trim()));
    asked
        .or_else(dirs::document_dir)
        .ok_or_else(|| Error::msg("could not find the Documents folder"))
}

/// Run a PowerShell command and return its standard output.
///
/// Anything variable goes in through `env`, never into `script`: a value
/// holding a quote or `$` cannot then break out of the command. Output is
/// forced to UTF-8 so a non-ASCII path survives the console code page.
#[cfg(windows)]
fn powershell(exe: &str, script: &str, env: &[(&str, &str)]) -> Result<String> {
    // Without a console to change, the assignment throws; the default
    // encoding is then the only one there is.
    let script = format!(
        "try {{ [Console]::OutputEncoding = [Text.Encoding]::UTF8 }} catch {{ }}; {script}"
    );
    let out = std::process::Command::new(exe)
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .envs(env.iter().copied())
        .output()
        .map_err(|e| Error::msg(format!("could not run {exe}: {e}")))?;
    if !out.status.success() {
        return Err(Error::msg(format!(
            "{exe} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The macros cmd.exe gets. cmd has no programmable completion, so short
/// names for the common commands are what it can have.
#[cfg_attr(not(windows), allow(dead_code))]
pub const DOSKEY_MACROS: &str =
    "ki=ketch install $*\r\nku=ketch upgrade $*\r\nkl=ketch list $*\r\nkun=ketch uninstall $*\r\n";

/// Where the macro file lives: in the root, not the versioned store prefix,
/// so the AutoRun line that names it survives every `self upgrade`.
pub fn doskey_file(cfg: &Config) -> PathBuf {
    cfg.root.join("share").join("ketch").join("ketch.doskey")
}

/// The command ketch adds to AutoRun to load `file`.
///
/// cmd expands `%…%` in AutoRun and has no escape for `"` inside quotes, so a
/// path holding either is refused rather than written and hoped for.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn autorun_command(file: &Path) -> Result<String> {
    let path = file.to_str().ok_or_else(|| {
        Error::msg(format!(
            "{} is not valid UTF-8, so cmd cannot be told about it",
            file.display()
        ))
    })?;
    if path.contains(['"', '%', '\n', '\r']) {
        return Err(Error::msg(format!(
            "{path} holds a character cmd's AutoRun cannot quote"
        )));
    }
    Ok(format!("doskey /macrofile=\"{path}\""))
}

/// What to do with the AutoRun value.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AutoRunEdit {
    /// Leave it as it is.
    Unchanged,
    /// Write this value.
    Set(String),
    /// Delete the value: nothing but ketch's command was in it.
    Delete,
}

#[cfg_attr(not(windows), allow(dead_code))]
fn autorun_has(current: &str, ours: &str) -> bool {
    current == ours
        || current.ends_with(&format!(" & {ours}"))
        || current.starts_with(&format!("{ours} & "))
        || current.contains(&format!(" & {ours} & "))
}

/// Add `ours` to an AutoRun value, after whatever the user already runs
/// there. Appending exactly ` & ours` is what lets [`autorun_remove`] give
/// the earlier value back byte for byte.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn autorun_add(current: Option<&str>, ours: &str) -> AutoRunEdit {
    match current {
        Some(current) if autorun_has(current, ours) => AutoRunEdit::Unchanged,
        // A blank value runs nothing; `  & cmd` would be a syntax error.
        Some(current) if !current.trim().is_empty() => {
            AutoRunEdit::Set(format!("{current} & {ours}"))
        }
        _ => AutoRunEdit::Set(ours.to_string()),
    }
}

/// Take `ours` back out of an AutoRun value, leaving the rest as it was.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn autorun_remove(current: &str, ours: &str) -> AutoRunEdit {
    if current == ours {
        return AutoRunEdit::Delete;
    }
    if let Some(head) = current.strip_suffix(&format!(" & {ours}")) {
        return AutoRunEdit::Set(head.to_string());
    }
    let middle = format!(" & {ours} & ");
    if let Some(at) = current.find(&middle) {
        return AutoRunEdit::Set(format!(
            "{} & {}",
            &current[..at],
            &current[at + middle.len()..]
        ));
    }
    if let Some(tail) = current.strip_prefix(&format!("{ours} & ")) {
        return AutoRunEdit::Set(tail.to_string());
    }
    AutoRunEdit::Unchanged
}

/// The value AutoRun lives in.
#[cfg_attr(not(windows), allow(dead_code))]
const AUTORUN_KEY: &str = r"Software\Microsoft\Command Processor";

/// Write the doskey macro file and load it from cmd's AutoRun, after
/// whatever AutoRun already runs.
#[cfg(windows)]
pub fn install_cmd_macros(cfg: &Config) -> Result<Outcome> {
    let file = doskey_file(cfg);
    let ours = autorun_command(&file)?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    std::fs::write(&file, DOSKEY_MACROS).map_err(|e| Error::io(&file, e))?;
    let current = read_autorun()?;
    match autorun_add(current.as_ref().map(|(_, v)| v.as_str()), &ours) {
        AutoRunEdit::Set(next) => {
            // An `ExpandString` stays one: its `%VAR%` must keep expanding.
            let kind = current.as_ref().map_or("String", |(k, _)| k.as_str());
            write_autorun(kind, &next)?;
            Ok(Outcome::Added)
        }
        AutoRunEdit::Unchanged | AutoRunEdit::Delete => Ok(Outcome::Unchanged),
    }
}

/// True when cmd's AutoRun loads this root's macro file.
pub fn cmd_macros_configured(cfg: &Config) -> bool {
    #[cfg(windows)]
    {
        let Ok(ours) = autorun_command(&doskey_file(cfg)) else {
            return false;
        };
        read_autorun()
            .ok()
            .flatten()
            .is_some_and(|(_, value)| autorun_has(&value, &ours))
    }
    #[cfg(not(windows))]
    {
        let _ = cfg;
        false
    }
}

/// Take this root's command back out of AutoRun — the earlier value comes
/// back as it was, or the value goes when it held only ketch's — and delete
/// the macro file. True when AutoRun changed.
pub fn uninstall_cmd_macros(cfg: &Config) -> Result<bool> {
    let file = doskey_file(cfg);
    #[cfg(windows)]
    let changed = {
        let ours = autorun_command(&file)?;
        match read_autorun()? {
            None => false,
            Some((kind, value)) => match autorun_remove(&value, &ours) {
                AutoRunEdit::Unchanged => false,
                AutoRunEdit::Set(next) => {
                    write_autorun(&kind, &next)?;
                    true
                }
                AutoRunEdit::Delete => {
                    delete_autorun()?;
                    true
                }
            },
        }
    };
    #[cfg(not(windows))]
    let changed = false;
    match std::fs::remove_file(&file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::io(&file, e)),
    }
    // Only if empty: `share` is a name a user could have put there too.
    let mut dir = file.parent();
    while let Some(d) = dir.filter(|d| *d != cfg.root) {
        if std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
    Ok(changed)
}

/// AutoRun's kind and raw value, `%VAR%` unexpanded; `None` when it is unset.
#[cfg(windows)]
fn read_autorun() -> Result<Option<(String, String)>> {
    let out = powershell(
        PowerShell::Desktop.exe(),
        "$k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($env:KETCH_AUTORUN_KEY); \
         if ($k -and ($k.GetValueNames() -contains 'AutoRun')) { \
         [Console]::Out.Write($k.GetValueKind('AutoRun').ToString() + [char]10 + \
         [string]$k.GetValue('AutoRun', '', 'DoNotExpandEnvironmentNames')) }",
        &[("KETCH_AUTORUN_KEY", AUTORUN_KEY)],
    )
    .map_err(|e| Error::msg(format!("could not read cmd's AutoRun: {e}")))?;
    Ok(out
        .split_once('\n')
        .map(|(kind, value)| (kind.trim().to_string(), value.to_string())))
}

#[cfg(windows)]
fn write_autorun(kind: &str, value: &str) -> Result<()> {
    powershell(
        PowerShell::Desktop.exe(),
        "$k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($env:KETCH_AUTORUN_KEY); \
         $k.SetValue('AutoRun', $env:KETCH_AUTORUN_VALUE, \
         [Microsoft.Win32.RegistryValueKind]$env:KETCH_AUTORUN_KIND); $k.Close()",
        &[
            ("KETCH_AUTORUN_KEY", AUTORUN_KEY),
            ("KETCH_AUTORUN_VALUE", value),
            ("KETCH_AUTORUN_KIND", kind),
        ],
    )
    .map(|_| ())
    .map_err(|e| Error::msg(format!("could not write cmd's AutoRun: {e}")))
}

#[cfg(windows)]
fn delete_autorun() -> Result<()> {
    powershell(
        PowerShell::Desktop.exe(),
        "$k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($env:KETCH_AUTORUN_KEY, $true); \
         if ($k) { $k.DeleteValue('AutoRun', $false); $k.Close() }",
        &[("KETCH_AUTORUN_KEY", AUTORUN_KEY)],
    )
    .map(|_| ())
    .map_err(|e| Error::msg(format!("could not delete cmd's AutoRun: {e}")))
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
            // A byte order mark (a PowerShell profile may open with one)
            // is not part of the first line.
            let starts_line = i == 0 || text.as_bytes()[i - 1] == b'\n' || &text[..i] == "\u{FEFF}";
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
    let target = dunce::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
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
    use rstest::rstest;

    const BIN: &str = "/home/u/.ketch/bin";

    /// One real PATH entry per shell counts; a backup, a longer path, a
    /// comment or nothing at all does not.
    #[rstest]
    #[case("export PATH=\"/home/u/.ketch/bin:$PATH\"", true)]
    #[case("set -gx PATH /home/u/.ketch/bin $PATH", true)]
    #[case("export PATH=/home/u/.ketch/bin", true)]
    #[case("export PATH=\"/home/u/.ketch/bin:$PATH\"\n", true)]
    #[case("export PATH=\"/home/u/.ketch/bin.bak:$PATH\"", false)]
    #[case("export PATH=\"/home/u/.ketch/bin/tools:$PATH\"", false)]
    #[case("  # export PATH=\"/home/u/.ketch/bin:$PATH\"\n", false)]
    #[case("", false)]
    fn only_a_real_path_entry_counts_as_configured(#[case] line: &str, #[case] expected: bool) {
        assert_eq!(mentions(line, BIN), expected);
    }

    fn zsh_block() -> String {
        Shell::Zsh.block(BIN)
    }

    #[rstest]
    #[case("-zsh", Some(Shell::Zsh))]
    #[case("/bin/bash", Some(Shell::Bash))]
    #[case("/opt/homebrew/bin/fish", Some(Shell::Fish))]
    #[case("/usr/bin/tcsh", None)]
    #[case("", None)]
    #[case("bash.exe", Some(Shell::Bash))]
    #[case(r"C:\Program Files\Git\bin\bash.exe", Some(Shell::Bash))]
    #[case(r"C:/Program Files/Git/usr/bin/zsh.exe", Some(Shell::Zsh))]
    #[case("FISH.EXE", Some(Shell::Fish))]
    #[case("bash.Exe", Some(Shell::Bash))]
    #[case(r"C:\Git\usr\bin\zsh.eXe", Some(Shell::Zsh))]
    fn a_program_path_identifies_the_shell(#[case] program: &str, #[case] expected: Option<Shell>) {
        assert_eq!(Shell::from_program(program), expected);
    }

    /// A quote or `$` in the path must not end the quoting or expand.
    #[rstest]
    #[case(
        Shell::Bash,
        "/home/o'brien/.ketch/bin",
        "export PATH='/home/o'\\''brien/.ketch/bin':\"$PATH\""
    )]
    #[case(
        Shell::Zsh,
        "/home/o'brien/.ketch/bin",
        "export PATH='/home/o'\\''brien/.ketch/bin':\"$PATH\""
    )]
    #[case(
        Shell::Bash,
        "/home/$USER/bin",
        "export PATH='/home/$USER/bin':\"$PATH\""
    )]
    #[case(
        Shell::Zsh,
        "/home/$USER/bin",
        "export PATH='/home/$USER/bin':\"$PATH\""
    )]
    fn a_hostile_path_cannot_break_posix_quoting(
        #[case] shell: Shell,
        #[case] dir: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(shell.export(dir), expected);
    }

    /// The two quoters agree on what they share and differ where the shells do.
    #[rstest]
    #[case("a\\b", "'a\\b'", "'a\\\\b'")]
    #[case("o'brien", "'o'\\''brien'", "'o\\'brien'")]
    fn quoting_keeps_a_backslash_or_quote_literal(
        #[case] dir: &str,
        #[case] posix: &str,
        #[case] fish: &str,
    ) {
        assert_eq!(quote_posix(dir), posix);
        assert_eq!(quote_fish(dir), fish);
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

    /// Every shell gets a line it can actually run, quoted for its own grammar.
    #[rstest]
    #[case(Shell::Bash, "export PATH='/home/u/.ketch/bin':\"$PATH\"")]
    #[case(Shell::Zsh, "export PATH='/home/u/.ketch/bin':\"$PATH\"")]
    #[case(Shell::Fish, "set -gx PATH '/home/u/.ketch/bin' $PATH")]
    fn each_shell_gets_the_syntax_it_can_actually_run(
        #[case] shell: Shell,
        #[case] expected: &str,
    ) {
        assert_eq!(shell.export(BIN), expected);
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

    const DOSKEY: &str = r#"doskey /macrofile="C:\Users\u\.ketch\share\ketch\ketch.doskey""#;

    #[rstest]
    #[case(None, AutoRunEdit::Set(DOSKEY.to_string()))]
    #[case(Some(String::new()), AutoRunEdit::Set(DOSKEY.to_string()))]
    #[case(Some("  ".to_string()), AutoRunEdit::Set(DOSKEY.to_string()))]
    #[case(Some("@echo off".to_string()), AutoRunEdit::Set(format!("@echo off & {DOSKEY}")))]
    #[case(Some(DOSKEY.to_string()), AutoRunEdit::Unchanged)]
    #[case(Some(format!("@echo off & {DOSKEY}")), AutoRunEdit::Unchanged)]
    #[case(Some(format!("{DOSKEY} & cls")), AutoRunEdit::Unchanged)]
    #[case(Some(format!("a & {DOSKEY} & b")), AutoRunEdit::Unchanged)]
    fn autorun_add_appends_once_after_what_is_there(
        #[case] current: Option<String>,
        #[case] expected: AutoRunEdit,
    ) {
        assert_eq!(autorun_add(current.as_deref(), DOSKEY), expected);
    }

    #[rstest]
    #[case(DOSKEY, AutoRunEdit::Delete)]
    #[case(&format!("@echo off & {DOSKEY}"), AutoRunEdit::Set("@echo off".to_string()))]
    #[case(&format!("{DOSKEY} & cls"), AutoRunEdit::Set("cls".to_string()))]
    #[case(&format!("a & {DOSKEY} & b"), AutoRunEdit::Set("a & b".to_string()))]
    #[case("@echo off", AutoRunEdit::Unchanged)]
    #[case("", AutoRunEdit::Unchanged)]
    // Another root's line is not this one's to take.
    #[case(
        r#"doskey /macrofile="D:\other\share\ketch\ketch.doskey""#,
        AutoRunEdit::Unchanged
    )]
    fn autorun_remove_takes_exactly_ketch_part(
        #[case] current: &str,
        #[case] expected: AutoRunEdit,
    ) {
        assert_eq!(autorun_remove(current, DOSKEY), expected);
    }

    /// Add then remove hands back the value that was there, byte for byte.
    #[rstest]
    #[case("@echo off")]
    #[case("set X=1 & prompt $g ")]
    #[case("%USERPROFILE%\\init.cmd")]
    fn autorun_add_then_remove_restores_the_earlier_value(#[case] earlier: &str) {
        let AutoRunEdit::Set(with) = autorun_add(Some(earlier), DOSKEY) else {
            panic!("nothing added to {earlier:?}");
        };
        assert_eq!(
            autorun_remove(&with, DOSKEY),
            AutoRunEdit::Set(earlier.to_string())
        );
    }

    #[test]
    fn autorun_command_quotes_the_path_and_refuses_what_cmd_cannot_hold() {
        let file = Path::new(r"C:\Users\u\.ketch\share\ketch\ketch.doskey");
        assert_eq!(autorun_command(file).expect("command"), DOSKEY);
        assert!(autorun_command(Path::new(r"C:\a%PATH%\ketch.doskey")).is_err());
        assert!(autorun_command(Path::new("C:\\a\"b\\ketch.doskey")).is_err());
    }

    #[test]
    fn the_doskey_file_holds_the_four_macros() {
        let lines: Vec<&str> = DOSKEY_MACROS.lines().map(str::trim_end).collect();
        assert_eq!(
            lines,
            [
                "ki=ketch install $*",
                "ku=ketch upgrade $*",
                "kl=ketch list $*",
                "kun=ketch uninstall $*"
            ]
        );
        assert!(DOSKEY_MACROS.ends_with("\r\n"));
    }

    #[test]
    fn the_profile_block_dot_sources_the_script_only_when_it_exists() {
        let block = powershell_block(r"C:\Users\u\Documents\PowerShell\Completions\ketch.ps1");
        assert_eq!(
            block,
            format!(
                "{BEGIN}\nif (Test-Path -LiteralPath 'C:\\Users\\u\\Documents\\PowerShell\\Completions\\ketch.ps1') {{ . 'C:\\Users\\u\\Documents\\PowerShell\\Completions\\ketch.ps1' }}\n{END}\n"
            )
        );
    }

    #[rstest]
    #[case("o'brien", "'o''brien'")]
    #[case("o\u{2019}brien", "'o\u{2019}\u{2019}brien'")]
    #[case("$env:X", "'$env:X'")]
    fn powershell_quoting_keeps_quotes_and_dollars_literal(
        #[case] text: &str,
        #[case] quoted: &str,
    ) {
        assert_eq!(quote_powershell(text), quoted);
    }

    #[test]
    fn a_profile_gets_the_block_once_and_loses_it_byte_for_byte() {
        let original = "Set-PSReadLineOption -EditMode Emacs\r\n";
        let script = r"C:\d\PowerShell\Completions\ketch.ps1";
        let with = splice_profile(original, script).expect("added");
        assert_eq!(splice_profile(&with, script), None);
        assert_eq!(unsplice_profile(&with), Some(Some(original.to_string())));
    }

    #[test]
    fn a_profile_ketch_created_goes_away_whole() {
        let with = splice_profile("", r"C:\d\ketch.ps1").expect("added");
        assert!(with.starts_with(BEGIN));
        assert_eq!(unsplice_profile(&with), Some(None));
    }

    /// Windows PowerShell reads a BOM-less profile in the ANSI code page.
    #[test]
    fn a_new_profile_naming_a_non_ascii_path_starts_with_a_bom() {
        let with = splice_profile("", r"C:\Users\Иван\Documents\ketch.ps1").expect("added");
        assert!(with.starts_with('\u{FEFF}'));
        assert_eq!(unsplice_profile(&with), Some(None));
    }

    #[test]
    fn profiles_are_the_all_hosts_ones_for_both_editions() {
        let docs = Path::new("D");
        assert_eq!(
            PowerShell::Core.profile(docs),
            docs.join("PowerShell").join("profile.ps1")
        );
        assert_eq!(
            PowerShell::Desktop.profile(docs),
            docs.join("WindowsPowerShell").join("profile.ps1")
        );
    }

    #[rstest]
    #[case("RemoteSigned\r\n", true)]
    #[case("Unrestricted", true)]
    #[case("Bypass", true)]
    #[case("Restricted", false)]
    #[case("AllSigned", false)]
    fn only_a_policy_that_runs_local_scripts_gets_a_new_profile(
        #[case] policy: &str,
        #[case] runs: bool,
    ) {
        assert_eq!(runs_local_scripts(policy), runs);
    }

    #[test]
    fn uninstalling_cmd_macros_removes_the_file_and_empty_dirs_only() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let cfg = Config::load(
            Some(tmp.path().to_path_buf()),
            &crate::report::Report::silent(),
        )
        .expect("config");
        let file = doskey_file(&cfg);
        std::fs::create_dir_all(file.parent().expect("parent")).expect("dirs");
        std::fs::write(&file, DOSKEY_MACROS).expect("write");
        let mine = cfg.root.join("share").join("mine.txt");
        std::fs::write(&mine, "keep").expect("write");
        uninstall_cmd_macros(&cfg).expect("uninstall");
        assert!(!file.exists());
        assert!(!file.parent().expect("parent").exists());
        assert!(mine.exists(), "a file ketch did not write was removed");
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
    fn windows_path_has_matches_a_quoted_registry_entry() {
        let dir = Path::new(r"C:\Users\u\.ketch\bin");
        let path = r#"C:\Windows\System32;"C:\Users\u\.ketch\bin";C:\Windows"#;
        assert!(windows_path_has(path, dir));
        assert!(windows_path_prepend(path, dir).is_none());
        assert_eq!(
            windows_path_remove(path, dir).as_deref(),
            Some(r"C:\Windows\System32;C:\Windows")
        );
    }

    /// A folder name may contain `;`. Quotes keep it one PATH entry; splitting on
    /// every `;` used to shatter it and miss the match (and corrupt remove).
    #[test]
    fn windows_path_keeps_semicolon_inside_quotes() {
        let dir = Path::new(r"C:\weird;name");
        let path = r#"C:\Windows;"C:\weird;name";C:\Other"#;
        assert_eq!(
            windows_path_entry_list(path),
            vec![r"C:\Windows", r#""C:\weird;name""#, r"C:\Other"]
        );
        assert!(windows_path_has(path, dir));
        assert!(windows_path_prepend(path, dir).is_none());
        assert_eq!(
            windows_path_remove(path, dir).as_deref(),
            Some(r"C:\Windows;C:\Other")
        );
    }

    #[rstest]
    #[case(
        r"C:\Windows\System32;C:\Users\u\.ketch\bin;C:\Windows",
        Some(r"C:\Windows\System32;C:\Windows")
    )]
    #[case(r"C:\Windows\System32", None)]
    fn windows_path_remove_drops_only_the_named_entry(
        #[case] path: &str,
        #[case] expected: Option<&str>,
    ) {
        let dir = Path::new(r"C:\Users\u\.ketch\bin");
        assert_eq!(windows_path_remove(path, dir).as_deref(), expected);
    }

    // The Windows case is an 8.3 short name; a link is how the same folder
    // gets a second spelling on a unix test host.
    #[cfg(unix)]
    #[test]
    fn a_path_entry_that_resolves_to_the_bin_dir_is_the_bin_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bin = tmp.path().join("bin");
        std::fs::create_dir(&bin).expect("bin");
        let link = tmp.path().join("alias");
        std::os::unix::fs::symlink(&bin, &link).expect("link");
        let path = format!("/usr/bin;{}", link.display());
        assert!(windows_path_has(&path, &bin));
        assert_eq!(
            windows_path_remove(&path, &bin).as_deref(),
            Some("/usr/bin")
        );
    }

    #[test]
    fn a_path_entry_for_a_missing_folder_matches_only_by_spelling() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gone = tmp.path().join("gone");
        assert!(!windows_path_has(&tmp.path().display().to_string(), &gone));
        assert!(windows_path_has(&gone.display().to_string(), &gone));
    }

    #[test]
    fn only_ketch_bin_dirs_that_are_gone_are_stale() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let live = tmp.path().join(".ketch").join("bin");
        std::fs::create_dir_all(&live).expect("bin");
        let gone_default = r"C:\Users\u\.ketch\bin";
        let gone_custom = tmp.path().join("custom").join("bin");
        let path = format!(
            r"C:\Windows;{};{gone_default};{};C:\gone\tools\bin",
            live.display(),
            gone_custom.display()
        );
        let custom = gone_custom.display().to_string();
        assert_eq!(
            stale_path_entries(&path, &gone_custom),
            vec![gone_default, custom.as_str()]
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn no_registry_entries_are_found_off_windows() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg = Config::load(
            Some(tmp.path().join("root")),
            &crate::report::Report::silent(),
        )
        .expect("config");
        assert!(registry_entries(&cfg).is_empty());
    }
}
