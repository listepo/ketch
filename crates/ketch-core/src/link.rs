// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-or-later
// Licensed under GPL-3.0 or later; see https://www.gnu.org/licenses/gpl-3.0.html

//! `ketch://` links: the one place a URL some web page or registry listing
//! handed a front end becomes a package name.
//!
//! A link can only open a package's page. It never names an install, an
//! upgrade or an uninstall, even to ask for confirmation (creator decision,
//! docs/research-desktop-platforms.md, open decision 10), so the grammar has
//! exactly one shape — `ketch://package/<name>` — and everything else is
//! refused rather than interpreted. Every byte of the link is untrusted: it
//! came from outside the app and may be shown to the user and used to look up
//! a path, so the name passes the same guard a manifest's package name does.

use crate::error::{Error, Result};
use crate::model::{normalize_name, usable_file_name};

/// The URL scheme front ends register with the operating system.
pub const SCHEME: &str = "ketch";

/// The only action a link may name.
const ACTION: &str = "package";

/// Longer than any real package link by a wide margin; bounding it keeps a
/// hostile link from being echoed into an error dialog at any length.
const MAX_LEN: usize = 512;

/// Package names are short; the registry's longest is far below this.
const MAX_NAME: usize = 100;

/// The package a `ketch://package/<name>` link names, ready to look up.
///
/// This only validates the shape and the name. It does not check that the
/// package exists: that is the caller's lookup, which keeps this free of the
/// network and of the install tree.
pub fn package_name(raw: &str) -> Result<String> {
    if raw.len() > MAX_LEN {
        return Err(Error::msg(format!(
            "ketch link is longer than {MAX_LEN} bytes"
        )));
    }
    // ASCII graphic only: whitespace and control characters hide text, and
    // non-ASCII letters would let a lookalike name pass for another package.
    if !raw.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(refuse(raw, "contains characters a link may not carry"));
    }
    let Some((scheme, rest)) = raw.split_once("://") else {
        return Err(refuse(raw, "is not a URL"));
    };
    if !scheme.eq_ignore_ascii_case(SCHEME) {
        return Err(refuse(raw, "is not a ketch link"));
    }
    // A query or fragment would be a second channel for instructions, and
    // userinfo, ports, escapes and backslashes are how a URL says one thing
    // to a parser and another to a person.
    if rest.contains(['?', '#', '@', ':', '%', '\\']) {
        return Err(refuse(raw, "carries more than a package name"));
    }
    let mut parts = rest.split('/');
    let (Some(action), Some(name)) = (parts.next(), parts.next()) else {
        return Err(refuse(raw, "names no package"));
    };
    if !action.eq_ignore_ascii_case(ACTION) {
        return Err(Error::msg(format!(
            "ketch links open a package page only; `{}` is not an action they support",
            action.escape_debug()
        )));
    }
    // One optional trailing slash, nothing deeper: `package/a/b` is not a
    // package called `a`.
    let tail = parts.next();
    if tail.is_some_and(|t| !t.is_empty()) || parts.next().is_some() {
        return Err(refuse(raw, "names more than one package"));
    }
    usable_name(name)
}

fn usable_name(name: &str) -> Result<String> {
    let shaped = name.len() <= MAX_NAME
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+'));
    if !shaped {
        return Err(Error::msg(format!(
            "`{}` is not a package name",
            name.escape_debug()
        )));
    }
    usable_file_name("package name", name)?;
    Ok(normalize_name(name))
}

fn refuse(raw: &str, why: &str) -> Error {
    // Escaped so a hostile link cannot rewrite the line it is reported on.
    Error::msg(format!(
        "ketch link `{}` {why}",
        raw.chars().take(80).collect::<String>().escape_debug()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("ketch://package/ripgrep", "ripgrep")]
    #[case("ketch://package/ripgrep/", "ripgrep")]
    #[case("KETCH://Package/RipGrep", "ripgrep")]
    #[case("ketch://package/foo.bar_baz-1+x", "foo.bar_baz-1+x")]
    fn a_link_to_a_package_page_yields_its_name(#[case] link: &str, #[case] name: &str) {
        assert_eq!(package_name(link).ok().as_deref(), Some(name));
    }

    #[rstest]
    #[case::install("ketch://install/ripgrep")]
    #[case::upgrade("ketch://upgrade/ripgrep")]
    #[case::uninstall("ketch://uninstall/ripgrep")]
    #[case::install_by_query("ketch://package/ripgrep?install=1")]
    #[case::install_by_fragment("ketch://package/ripgrep#install")]
    fn a_link_that_names_an_action_other_than_opening_a_page_is_refused(#[case] link: &str) {
        assert!(package_name(link).is_err());
    }

    #[rstest]
    #[case::empty("")]
    #[case::other_scheme("https://package/ripgrep")]
    #[case::no_scheme("package/ripgrep")]
    #[case::no_authority("ketch:package/ripgrep")]
    #[case::no_name("ketch://package")]
    #[case::empty_name("ketch://package/")]
    #[case::two_names("ketch://package/a/b")]
    #[case::double_slash("ketch://package//a")]
    #[case::traversal("ketch://package/..")]
    #[case::traversal_nested("ketch://package/../evil")]
    #[case::dot("ketch://package/.")]
    #[case::leading_dot("ketch://package/.hidden")]
    #[case::leading_dash("ketch://package/-rf")]
    #[case::trailing_dot("ketch://package/evil.")]
    #[case::backslash("ketch://package/..\\evil")]
    #[case::percent_escape("ketch://package/%2e%2e")]
    #[case::percent_slash("ketch://package/a%2fb")]
    #[case::userinfo("ketch://user@package/ripgrep")]
    #[case::port("ketch://package:8080/ripgrep")]
    #[case::space("ketch://package/rip grep")]
    #[case::newline("ketch://package/rip\ngrep")]
    #[case::nul("ketch://package/rip\0grep")]
    #[case::escape_sequence("ketch://package/\u{1b}[31mred")]
    #[case::bidi_override("ketch://package/safe\u{202e}txt")]
    #[case::lookalike("ketch://package/r\u{0131}pgrep")]
    #[case::shell_metacharacters("ketch://package/a;rm")]
    #[case::repo_form("ketch://package/owner/repo")]
    fn a_malformed_or_hostile_link_is_refused(#[case] link: &str) {
        assert!(package_name(link).is_err(), "{link:?} was accepted");
    }

    #[test]
    fn an_oversized_link_is_refused_without_echoing_it() {
        let link = format!("ketch://package/{}", "a".repeat(MAX_LEN));
        let message = package_name(&link).unwrap_or_else(|e| e.to_string());
        assert!(message.len() < 200, "{message}");
    }

    #[test]
    fn an_oversized_name_is_refused() {
        let link = format!("ketch://package/{}", "a".repeat(MAX_NAME + 1));
        assert!(package_name(&link).is_err());
    }

    #[test]
    fn a_refusal_escapes_what_it_quotes() {
        let message = package_name("ketch://package/a\u{1b}[2Jb").unwrap_or_else(|e| e.to_string());
        assert!(!message.contains('\u{1b}'), "{message:?}");
    }
}
