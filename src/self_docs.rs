// Copyright (c) 2026 Ivan Tugay
// SPDX-License-Identifier: GPL-3.0-only
// Licensed under GPL-3.0 only; see https://www.gnu.org/licenses/gpl-3.0.html

//! The renderers for ketch's own man pages and completion scripts.
//!
//! `ketch self install` places them beside the binary it installs. Writing and
//! linking them is the core's job, but both are rendered from the command
//! line, which only this crate has (`man.rs`, `complete.rs`), so it hands the
//! renderers over as [`SELF_DOCS`].

use ketch_core::extra::SelfDocs;

/// The renderers every `ketch self` path that places ketch's docs is given.
pub const SELF_DOCS: SelfDocs = SelfDocs {
    man_pages: crate::man::write_to,
    completion: crate::complete::script,
};

#[cfg(test)]
mod tests {
    use super::*;
    use ketch_core::extra::{classify, write_ketch_docs};
    use ketch_core::model::ExtraKind;

    #[test]
    fn generated_ketch_docs_classify_and_stay_in_the_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path().join("store/ketch/1.0.0");
        let extras = write_ketch_docs(&prefix, SELF_DOCS).unwrap();
        assert!(prefix.join("share/man/man1/ketch.1").is_file());
        assert!(prefix.join("share/ketch/completions/ketch").is_file());
        for entry in &extras {
            classify(entry).unwrap_or_else(|e| panic!("{}: {e}", entry.as_rel_path()));
        }
        let man = std::fs::read_to_string(prefix.join("share/man/man1/ketch.1")).unwrap();
        assert!(man.contains(".TH KETCH 1"), "{man}");
        assert!(man.contains(".SH SUBCOMMANDS"), "{man}");
    }

    #[test]
    fn every_generated_man_page_is_recorded_as_a_man_extra() {
        let tmp = tempfile::tempdir().unwrap();
        let prefix = tmp.path().join("store/ketch/1.0.0");
        let extras = write_ketch_docs(&prefix, SELF_DOCS).unwrap();
        let recorded: Vec<String> = extras
            .iter()
            .map(|e| classify(e).unwrap())
            .filter(|c| c.kind == ExtraKind::Man)
            .map(|c| c.rel_path)
            .collect();
        let expected: Vec<String> = crate::man::pages()
            .unwrap()
            .into_iter()
            .map(|p| format!("share/man/man1/{}", p.file_name))
            .collect();
        assert_eq!(recorded, expected);
        assert!(recorded.iter().all(|rel| prefix.join(rel).is_file()));
    }
}
