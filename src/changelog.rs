//! Reading a package's changelog, from either of the two places one lives.
//!
//! Projects record what changed in one of two ways, and most do both
//! inconsistently: a `CHANGELOG.md` committed to the repository and shipped
//! inside the release, and the notes attached to the release itself. ketch
//! already has the second — every `Release` carries `notes` — and the first is
//! sitting in the store next to the binary that was installed from it.
//!
//! The file is preferred when it exists, because reading it needs no network
//! and it is the version the user actually has. Falling back to release notes
//! covers every project that keeps its history only on the forge.
//!
//! Nothing here trusts the file's structure. A changelog is prose someone else
//! wrote; the section matcher takes what it recognises and says plainly when it
//! recognises nothing, rather than guessing a range and printing the wrong
//! release's history.

use crate::error::Result;
use std::path::{Path, PathBuf};

/// File names worth looking for, most conventional first.
const NAMES: &[&str] = &[
    "CHANGELOG.md",
    "CHANGELOG",
    "CHANGELOG.txt",
    "CHANGES.md",
    "CHANGES",
    "HISTORY.md",
    "NEWS.md",
    "RELEASES.md",
    "RELEASE_NOTES.md",
];

/// Directories a project might tuck it into, relative to the payload root.
const SUBDIRS: &[&str] = &["", "doc", "docs", "share/doc"];

/// Where a changelog came from, so the caller can say so.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A file inside the installed payload.
    File(PathBuf),
    /// Notes published with the release.
    Release,
}

/// One changelog, ready to print.
#[derive(Debug, Clone)]
pub struct Entry {
    pub origin: Origin,
    /// The heading the section was found under, when there was one.
    pub heading: Option<String>,
    pub body: String,
}

/// Find a changelog file inside an installed payload.
///
/// Only looks a fixed set of places rather than walking the tree: a release
/// payload can contain thousands of files, and the ones that ship a changelog
/// put it where everyone else does.
pub fn find_file(prefix: &Path) -> Option<PathBuf> {
    for dir in SUBDIRS {
        let base = if dir.is_empty() {
            prefix.to_path_buf()
        } else {
            prefix.join(dir)
        };
        for name in NAMES {
            let path = base.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
        // Some projects ship `share/doc/<pkg>/CHANGELOG.md`, one level deeper
        // than the directory itself. One level, not a walk.
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten().filter(|e| e.path().is_dir()) {
                for name in NAMES {
                    let path = entry.path().join(name);
                    if path.is_file() {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

/// Read a changelog file and take the section for `version`, or the whole file
/// when no section matches.
pub fn from_file(path: &Path, version: Option<&str>) -> Result<Entry> {
    let raw = std::fs::read_to_string(path).map_err(|e| crate::error::Error::io(path, e))?;
    let text = sanitize(&raw);
    let (heading, body) = match version.and_then(|v| section(&text, v)) {
        Some(found) => found,
        None => (None, text.trim().to_string()),
    };
    Ok(Entry {
        origin: Origin::File(path.to_path_buf()),
        heading,
        body,
    })
}

/// The notes a release published, if it published any.
pub fn from_release(notes: Option<&str>) -> Option<Entry> {
    let body = sanitize(notes?.trim());
    if body.is_empty() {
        return None;
    }
    Some(Entry {
        origin: Origin::Release,
        heading: None,
        body,
    })
}

/// Strip what a changelog has no business containing.
///
/// This is a client app's bytes on their way to a terminal. An escape sequence
/// in one can rewrite lines already on screen or drive the terminal itself, and
/// a bidi override can make a line read as the reverse of what it says. Neither
/// belongs in prose, so both are dropped where the text enters rather than
/// where it is printed, which would leave every caller to remember.
fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|&c| match c {
            '\n' | '\t' => true,
            // `is_control` is C0, DEL and C1 — including U+009B, which some
            // terminals still take as a CSI introducer on its own.
            _ => !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'),
        })
        .collect()
}

/// The block of a Markdown changelog belonging to one version.
///
/// Returns the heading it matched and the lines under it, up to the next
/// heading at the same level or higher. `None` when no heading names this
/// version — a caller that printed the whole file in that case is being
/// honest; one that printed a guessed range would not be.
pub fn section(text: &str, version: &str) -> Option<(Option<String>, String)> {
    let wanted = normalise(version);
    if wanted.is_empty() {
        return None;
    }
    let lines: Vec<&str> = text.lines().collect();
    let heads = headings(&lines);
    let at = heads.iter().position(|h| heading_names(&h.text, &wanted))?;

    let head = &heads[at];
    let end = heads[at + 1..]
        .iter()
        .find(|next| next.level <= head.level)
        .map_or(lines.len(), |next| next.start);

    let body = lines[head.body..end].join("\n").trim().to_string();
    Some((Some(head.text.clone()), body))
}

/// One heading: where its text is, where its body starts, and how deep it sits.
struct Heading {
    start: usize,
    body: usize,
    level: usize,
    text: String,
}

/// Every heading in the file, in order.
fn headings(lines: &[&str]) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(level) = atx_level(lines[i]) {
            out.push(Heading {
                start: i,
                body: i + 1,
                level,
                text: lines[i].trim().to_string(),
            });
        } else if underlined(lines[i], lines.get(i + 1).copied().unwrap_or("")) {
            out.push(Heading {
                start: i,
                body: i + 2,
                level: 1,
                text: lines[i].trim().to_string(),
            });
            i += 1;
        }
        i += 1;
    }
    out
}

/// `##` and friends.
fn atx_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    // `#####hi` is not a heading; `## 1.2.3` is.
    if (1..=6).contains(&hashes) && line.chars().nth(hashes).is_none_or(|c| c == ' ') {
        Some(hashes)
    } else {
        None
    }
}

/// A Setext heading: text with `====` under it, which is how ripgrep and every
/// changelog in its lineage marks a release.
///
/// Only `=`. The other Setext underline is `---`, and a changelog that puts a
/// `---` rule between releases — a common habit — would have every section
/// truncated at the line above the rule. Missing those headings costs a
/// fallback to the release notes; matching them wrongly costs a silently
/// half-printed release.
fn underlined(text: &str, next: &str) -> bool {
    let rule = next.trim_end();
    !text.trim().is_empty()
        && atx_level(text).is_none()
        && rule.len() >= 2
        && rule.chars().all(|c| c == '=')
}

/// True when a heading names this version.
///
/// Changelog headings are written every way there is — `## [1.2.3] - 2024-05-01`,
/// `## v1.2.3`, `## 1.2.3 (2024-05-01)`, `## Release 1.2.3` — so the version is
/// matched as a whole token anywhere in the heading rather than by shape.
fn heading_names(line: &str, wanted: &str) -> bool {
    line.trim_start_matches('#')
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+'))
        .any(|token| normalise(token) == wanted)
}

/// Strip the `v` a tag carries and a release's surrounding punctuation, so
/// `v1.2.3`, `[1.2.3]` and `1.2.3` are one version.
fn normalise(text: &str) -> String {
    let trimmed = text
        .trim()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric());
    trimmed
        .strip_prefix('v')
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEEP_A_CHANGELOG: &str = "\
# Changelog

## [Unreleased]

- something in flight

## [1.2.3] - 2024-05-01

### Added

- the thing everybody wanted

### Fixed

- the thing nobody noticed

## [1.2.2] - 2024-04-01

- an older fix
";

    #[test]
    fn a_keep_a_changelog_section_stops_at_the_next_release() {
        let (heading, body) = section(KEEP_A_CHANGELOG, "1.2.3").expect("section");
        assert_eq!(heading.as_deref(), Some("## [1.2.3] - 2024-05-01"));
        assert!(body.contains("the thing everybody wanted"));
        assert!(body.contains("### Fixed"), "sub-headings stay in: {body}");
        assert!(!body.contains("an older fix"), "ran into 1.2.2: {body}");
        assert!(!body.contains("something in flight"));
    }

    #[test]
    fn a_tag_and_its_version_find_the_same_section() {
        let by_tag = section(KEEP_A_CHANGELOG, "v1.2.3").expect("by tag");
        let by_version = section(KEEP_A_CHANGELOG, "1.2.3").expect("by version");
        assert_eq!(by_tag.0, by_version.0);
    }

    #[test]
    fn the_last_section_in_a_file_runs_to_the_end() {
        let (_, body) = section(KEEP_A_CHANGELOG, "1.2.2").expect("section");
        assert_eq!(body, "- an older fix");
    }

    #[test]
    fn every_way_people_write_a_version_heading_is_recognised() {
        for heading in [
            "## 1.2.3",
            "## v1.2.3",
            "## [1.2.3]",
            "## [1.2.3] - 2024-05-01",
            "## 1.2.3 (2024-05-01)",
            "## Release 1.2.3",
            "### v1.2.3 — codename",
            "# 1.2.3",
        ] {
            let text = format!("{heading}\n\n- a change\n");
            assert!(
                section(&text, "1.2.3").is_some(),
                "did not match `{heading}`"
            );
        }
    }

    #[test]
    fn a_version_that_is_a_prefix_of_another_is_not_matched() {
        let text = "## 1.2.30\n\n- not this one\n";
        assert_eq!(section(text, "1.2.3"), None);
    }

    #[test]
    fn a_version_nobody_wrote_about_has_no_section() {
        assert_eq!(section(KEEP_A_CHANGELOG, "9.9.9"), None);
        assert_eq!(section(KEEP_A_CHANGELOG, ""), None);
    }

    #[test]
    fn a_deeper_heading_does_not_end_a_section_but_a_shallower_one_does() {
        let text = "\
## 1.0.0

### Added
- a

# 0.9.0
- old
";
        let (_, body) = section(text, "1.0.0").expect("section");
        assert!(body.contains("### Added"));
        assert!(!body.contains("old"));
    }

    #[test]
    fn a_run_of_hashes_that_is_not_a_heading_is_not_treated_as_one() {
        assert_eq!(atx_level("#tag"), None);
        assert_eq!(atx_level("####### too deep"), None);
        assert_eq!(atx_level("## fine"), Some(2));
        assert_eq!(atx_level("not a heading"), None);
    }

    #[test]
    fn an_underlined_version_is_a_heading_and_the_underline_is_not_in_the_body() {
        let text = "\
15.2.0 (2026-07-15)
===================
Platform support:

* a thing

15.1.0 (2026-05-01)
===================
older
";
        let (heading, body) = section(text, "15.2.0").expect("section");
        assert_eq!(heading.as_deref(), Some("15.2.0 (2026-07-15)"));
        assert!(body.starts_with("Platform support:"), "{body}");
        assert!(!body.contains("older"), "ran into 15.1.0: {body}");
        assert!(!body.contains("==="), "kept the underline: {body}");
    }

    #[test]
    fn a_rule_between_releases_never_swallows_the_lines_above_it() {
        // `---` is a Setext underline in Markdown, but in a changelog it is
        // nearly always a separator; treating it as a heading would cut every
        // section short at the line before the rule.
        let text = "\
## 1.2.3

- the change

---

## 1.2.2
";
        let (_, body) = section(text, "1.2.3").expect("section");
        assert!(
            body.contains("- the change"),
            "truncated at the rule: {body}"
        );
    }

    #[test]
    fn a_changelog_cannot_drive_the_terminal_it_is_printed_to() {
        let hostile = "\u{1b}[2J\u{1b}]0;pwned\u{7}real\rtext\u{202e}reversed\u{9b}m";
        let entry = from_release(Some(hostile)).expect("entry");
        assert_eq!(entry.body, "[2J]0;pwnedrealtextreversedm");
        assert!(!entry.body.contains('\u{1b}'));
        assert!(!entry.body.contains('\r'));
    }

    #[test]
    fn sanitising_keeps_the_shape_of_the_prose() {
        assert_eq!(
            sanitize("## 1.0\n\n\t- a\r\n- b\n"),
            "## 1.0\n\n\t- a\n- b\n"
        );
    }

    #[test]
    fn a_release_with_no_notes_produces_nothing_rather_than_a_blank_entry() {
        assert!(from_release(None).is_none());
        assert!(from_release(Some("   \n ")).is_none());
        assert!(from_release(Some("real notes")).is_some());
    }

    #[test]
    fn a_changelog_is_found_where_projects_actually_put_it() {
        let tmp = tempfile::tempdir().expect("temp dir");
        let nested = tmp.path().join("share/doc/testtool");
        std::fs::create_dir_all(&nested).expect("mkdir");
        std::fs::write(nested.join("CHANGELOG.md"), "# Changelog\n").expect("write");
        assert_eq!(find_file(tmp.path()), Some(nested.join("CHANGELOG.md")));
    }

    #[test]
    fn a_payload_with_no_changelog_reports_none() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::write(tmp.path().join("README.md"), "# hi\n").expect("write");
        assert_eq!(find_file(tmp.path()), None);
    }

    #[test]
    fn the_root_is_preferred_over_a_copy_buried_in_docs() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(tmp.path().join("docs")).expect("mkdir");
        std::fs::write(tmp.path().join("CHANGELOG.md"), "root\n").expect("write");
        std::fs::write(tmp.path().join("docs/CHANGELOG.md"), "docs\n").expect("write");
        assert_eq!(find_file(tmp.path()), Some(tmp.path().join("CHANGELOG.md")));
    }
}
