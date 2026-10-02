// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! ketch's own man pages, rendered from the clap definition.
//!
//! One page per visible command, `ketch.1` plus `ketch-<cmd>[-<sub>…].1`, so
//! `man ketch-self-uninstall` shows that command's options, defaults and
//! environment variables. Kept apart from `extra.rs`, which classifies man
//! pages a client app ships: this module only produces the host's own.

use crate::cli::Cli;
use crate::error::{Error, Result};
use clap::CommandFactory;
use std::path::{Path, PathBuf};

/// One rendered man page: its file name (`ketch-install.1`) and roff source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// File name under `man1/`.
    pub file_name: String,
    /// The roff source.
    pub roff: Vec<u8>,
}

/// Render every page, depth first, parents before their subcommands.
pub fn pages() -> Result<Vec<Page>> {
    let mut cmd = Cli::command();
    // Building fills in each subcommand's display name (`ketch-config-create`)
    // and bin name (`ketch config create`), which the page file name and its
    // synopsis are taken from.
    cmd.build();
    let date = page_date();
    let mut out = Vec::new();
    render(&cmd, &date, &mut out)?;
    Ok(out)
}

fn render(cmd: &clap::Command, date: &str, out: &mut Vec<Page>) -> Result<()> {
    let title = cmd
        .get_display_name()
        .unwrap_or_else(|| cmd.get_name())
        .to_ascii_uppercase();
    let man = clap_mangen::Man::new(cmd.clone())
        .title(title)
        .date(date)
        .source(format!("ketch {}", env!("CARGO_PKG_VERSION")))
        .manual("ketch manual");
    let file_name = man.get_filename();
    let mut roff = Vec::new();
    man.render(&mut roff)
        .map_err(|e| Error::msg(format!("rendering man page {file_name}: {e}")))?;
    out.push(Page {
        file_name,
        roff: tidy(&roff),
    });
    for sub in visible_subcommands(cmd) {
        render(sub, date, out)?;
    }
    Ok(())
}

/// clap_mangen puts "Possible values" after a break, a blank line and another
/// break (or, with no help text, a blank line and a break). A blank line is
/// already a paragraph space, so mandoc drops the breaks around it with a
/// warning; one `.sp` is the spacing either sequence was after.
fn tidy(roff: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(roff)
        .replace("\n.br\n\n.br\n", "\n.sp\n")
        .replace("\n\n.br\n", "\n.sp\n")
        .into_bytes()
}

/// The `.TH` date: `SOURCE_DATE_EPOCH` when a packager sets it, so the same
/// release renders the same bytes, and today otherwise.
fn page_date() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });
    let stamp = crate::log::timestamp(secs);
    stamp.get(..10).unwrap_or(&stamp).to_string()
}

/// Subcommands that get a page: not hidden, and not clap's generated `help`,
/// which says nothing `--help` does not.
pub fn visible_subcommands(cmd: &clap::Command) -> impl Iterator<Item = &clap::Command> {
    cmd.get_subcommands()
        .filter(|sub| !sub.is_hide_set() && sub.get_name() != "help")
}

/// Write every page into `dir`, creating it, and return the paths written.
pub fn write_to(dir: &Path) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
    let mut written = Vec::new();
    for page in pages()? {
        let path = dir.join(&page.file_name);
        std::fs::write(&path, &page.roff).map_err(|e| Error::io(&path, e))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn expected_names(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
        out.push(format!("{prefix}.1"));
        for sub in visible_subcommands(cmd) {
            expected_names(sub, &format!("{prefix}-{}", sub.get_name()), out);
        }
    }

    #[test]
    fn one_page_per_visible_command() {
        let mut cmd = Cli::command();
        cmd.build();
        let mut expected = Vec::new();
        expected_names(&cmd, "ketch", &mut expected);
        let actual: Vec<String> = pages().unwrap().into_iter().map(|p| p.file_name).collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn nested_commands_get_their_own_page() {
        let names: Vec<String> = pages().unwrap().into_iter().map(|p| p.file_name).collect();
        for name in ["ketch.1", "ketch-install.1", "ketch-self-uninstall.1"] {
            assert!(names.iter().any(|n| n == name), "missing {name}: {names:?}");
        }
    }

    #[test]
    fn hidden_commands_and_help_get_no_page() {
        let names: Vec<String> = pages().unwrap().into_iter().map(|p| p.file_name).collect();
        assert!(!names.iter().any(|n| n == "ketch-man.1"), "{names:?}");
        assert!(!names.iter().any(|n| n.ends_with("-help.1")), "{names:?}");
    }

    #[test]
    fn a_subcommand_page_names_its_full_invocation_and_options() {
        let page = pages()
            .unwrap()
            .into_iter()
            .find(|p| p.file_name == "ketch-install.1")
            .unwrap();
        let roff = String::from_utf8(page.roff).unwrap();
        assert!(roff.contains(".TH KETCH-INSTALL 1"), "{roff}");
        assert!(roff.contains("ketch install"), "{roff}");
        assert!(roff.contains("\\-\\-force"), "{roff}");
    }

    #[test]
    fn tidy_turns_a_break_beside_a_blank_line_into_one_paragraph_space() {
        let raw = b"help\n.br\n\n.br\n\\fIPossible values:\\fR\n";
        assert_eq!(
            String::from_utf8(tidy(raw)).unwrap(),
            "help\n.sp\n\\fIPossible values:\\fR\n"
        );
        assert_eq!(
            String::from_utf8(tidy(b"<SHELL>\n\n.br\nvalues\n")).unwrap(),
            "<SHELL>\n.sp\nvalues\n"
        );
        for page in &pages().unwrap() {
            let roff = String::from_utf8_lossy(&page.roff);
            assert!(!roff.contains("\n\n.br\n"), "{}", page.file_name);
        }
    }

    #[test]
    fn write_to_creates_the_directory_and_every_page() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("out/man1");
        let written = write_to(&dir).unwrap();
        assert_eq!(written.len(), pages().unwrap().len());
        assert!(written.iter().all(|p| p.is_file()));
    }
}
