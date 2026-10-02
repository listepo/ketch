// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! Arbitrary argv through the real clap tree, `Cli::try_parse_from`, plus the
//! error a failed parse renders. Most tokens are drawn from the tree's own
//! subcommands, flags and values so the fuzzer gets past the first word.
#![no_main]

use std::ffi::OsString;
use std::sync::OnceLock;

use arbitrary::Arbitrary;
use ketch::fuzzing;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Tok {
    /// A word of the clap tree.
    Known(u16),
    /// `--long=value` / `-svalue`.
    Glued(u16, String),
    Free(String),
    Num(i64),
    /// Non-UTF-8 argv, which a Unix shell can pass.
    Bytes(Vec<u8>),
}

fn vocab() -> &'static [String] {
    static WORDS: OnceLock<Vec<String>> = OnceLock::new();
    WORDS.get_or_init(fuzzing::cli_vocab)
}

fn to_os(tok: Tok, words: &[String]) -> OsString {
    let word = |i: u16| words[usize::from(i) % words.len()].clone();
    match tok {
        Tok::Known(i) => word(i).into(),
        Tok::Glued(i, v) => {
            let w = word(i);
            if w.starts_with("--") {
                format!("{w}={v}")
            } else {
                format!("{w}{v}")
            }
            .into()
        }
        Tok::Free(s) => s.into(),
        Tok::Num(n) => n.to_string().into(),
        #[cfg(unix)]
        Tok::Bytes(b) => std::os::unix::ffi::OsStringExt::from_vec(b),
        #[cfg(not(unix))]
        Tok::Bytes(b) => String::from_utf8_lossy(&b).into_owned().into(),
    }
}

fuzz_target!(|toks: Vec<Tok>| {
    let words = vocab();
    fuzzing::cli_argv(toks.into_iter().take(32).map(|t| to_os(t, words)).collect());
});
