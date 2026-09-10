//! Line diffs between two texts, as `ketch registry push` shows them.
//!
//! A diff of two package files is small and needs no library for: the whole
//! point is a readable unified rendering of what changed between the
//! registry's copy and the local one, not a patch anyone applies. The
//! algorithm is a longest-common-subsequence table over lines, walked
//! forward into an edit script. On inputs of a few dozen lines the
//! quadratic table costs a few kilobytes, and the code stays short enough
//! to audit in one read — which is why no diff crate was added, and why a
//! linear-space Myers, whose only virtue is surviving files orders of
//! magnitude larger than a package file, would buy nothing here.
//!
//! Lines are compared as `str::lines` splits them, so `"a"` and `"a\n"`
//! diff as equal and no `\ No newline at end of file` marker is ever shown.
//! A CRLF-versus-LF difference likewise does not appear, which suits a diff
//! that a person reads and nothing ever applies.

/// Context lines shown on each side of a change.
const CONTEXT: usize = 3;

/// One step of the walk from the old text to the new one.
#[derive(Clone, Copy)]
enum Op<'a> {
    /// A line present in both texts.
    Context(&'a str),
    /// A line only the old text had.
    Delete(&'a str),
    /// A line only the new text has.
    Insert(&'a str),
}

/// A unified diff between `old` and `new`, with three lines of context and no
/// `---`/`+++` header. Empty when the texts are equal.
///
/// Every `@@` header prints both counts even when one is 1 — GNU diff drops
/// a `,1`, but the exact shape here is a rendering contract with the caller.
/// Two changes share a hunk while at most six unchanged lines (twice the
/// context) run between them, so no side of a hunk ever shows more than
/// three context lines.
pub fn unified(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let ops = script(&old_lines, &new_lines);
    if ops.iter().all(|op| matches!(op, Op::Context(_))) {
        return String::new();
    }
    render(&ops)
}

/// The edit script turning `old` into `new`.
fn script<'a>(old: &'a [&'a str], new: &'a [&'a str]) -> Vec<Op<'a>> {
    // table[i][j] is the length of the longest common subsequence of
    // old[i..] and new[j..]; building it over suffixes is what lets the
    // walk below run forward and emit the diff in reading order.
    let mut table = vec![vec![0u32; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut ops = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        // A matching pair of next lines is always on some optimal path, so
        // taking it never costs a shorter diff later.
        if i < old.len() && j < new.len() && old[i] == new[j] {
            ops.push(Op::Context(old[i]));
            i += 1;
            j += 1;
        } else if i < old.len() && (j == new.len() || table[i + 1][j] >= table[i][j + 1]) {
            ops.push(Op::Delete(old[i]));
            i += 1;
        } else {
            // The old text is exhausted, or keeping its next line matches
            // strictly less of the new one; either way new[j] exists here.
            ops.push(Op::Insert(new[j]));
            j += 1;
        }
    }
    deletions_first(ops)
}

/// Lays each changed run out as its deletions followed by its insertions,
/// the order every diff tool shows; which of the two the walk emits first
/// depends on where in the texts the change sits.
fn deletions_first(ops: Vec<Op<'_>>) -> Vec<Op<'_>> {
    let mut ordered = Vec::with_capacity(ops.len());
    let mut i = 0;
    while i < ops.len() {
        if matches!(ops[i], Op::Context(_)) {
            ordered.push(ops[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < ops.len() && !matches!(ops[i], Op::Context(_)) {
            i += 1;
        }
        for op in &ops[start..i] {
            if matches!(op, Op::Delete(_)) {
                ordered.push(*op);
            }
        }
        for op in &ops[start..i] {
            if matches!(op, Op::Insert(_)) {
                ordered.push(*op);
            }
        }
    }
    ordered
}

/// Groups the script into hunks and renders them.
fn render(ops: &[Op<'_>]) -> String {
    let mut out = String::new();
    let mut scanned = 0;
    while let Some(first_change) = (scanned..ops.len()).find(|k| is_change(&ops[*k])) {
        let lo = first_change.saturating_sub(CONTEXT);
        // A later change joins this hunk only while the unchanged run in
        // front of it is short enough that the two hunks' context lines
        // would meet or overlap.
        let mut last_change = first_change;
        for (k, op) in ops.iter().enumerate().skip(first_change + 1) {
            if !is_change(op) {
                continue;
            }
            if k - last_change - 1 > 2 * CONTEXT {
                break;
            }
            last_change = k;
        }
        let hi = (last_change + 1 + CONTEXT).min(ops.len());
        emit_hunk(&mut out, &ops[lo..hi], &ops[..lo]);
        scanned = hi;
    }
    out
}

/// Whether an op changes the text rather than carrying it over.
fn is_change(op: &Op<'_>) -> bool {
    matches!(op, Op::Delete(_) | Op::Insert(_))
}

/// Appends one hunk to `out`: its `@@` header, then its body lines.
fn emit_hunk(out: &mut String, hunk: &[Op<'_>], before: &[Op<'_>]) {
    // A line of old is anything but an insertion and a line of new anything
    // but a deletion: context lines count towards both.
    let old_count = hunk
        .iter()
        .filter(|op| !matches!(op, Op::Insert(_)))
        .count();
    let new_count = hunk
        .iter()
        .filter(|op| !matches!(op, Op::Delete(_)))
        .count();
    let old_before = before
        .iter()
        .filter(|op| !matches!(op, Op::Insert(_)))
        .count();
    let new_before = before
        .iter()
        .filter(|op| !matches!(op, Op::Delete(_)))
        .count();
    // An empty range starts at the line before it, 0 at the top of the file
    // — how a unified diff says "inserted after that line" without pointing
    // at a line the range does not contain.
    let old_start = if old_count == 0 {
        old_before
    } else {
        old_before + 1
    };
    let new_start = if new_count == 0 {
        new_before
    } else {
        new_before + 1
    };
    out.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        old_start, old_count, new_start, new_count
    ));
    for op in hunk {
        let (prefix, line) = match op {
            Op::Context(line) => (' ', line),
            Op::Delete(line) => ('-', line),
            Op::Insert(line) => ('+', line),
        };
        out.push(prefix);
        out.push_str(line);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// `line01` through `lineNN`, newline-joined, for gap-size tests.
    fn numbered(count: usize) -> String {
        (1..=count)
            .map(|n| format!("line{n:02}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn identical_texts_diff_to_an_empty_string() {
        let text = "[package]\nname = \"ripgrep\"\n";
        assert_eq!(unified(text, text), "");
    }

    #[test]
    fn empty_texts_diff_to_an_empty_string() {
        assert_eq!(unified("", ""), "");
    }

    #[test]
    fn a_missing_trailing_newline_is_not_a_change() {
        assert_eq!(
            unified("[package]\nname = \"rg\"", "[package]\nname = \"rg\"\n"),
            ""
        );
    }

    #[test]
    fn an_empty_old_text_is_all_insertions() {
        assert_eq!(
            unified("", "alpha\nbeta\n"),
            "@@ -0,0 +1,2 @@\n+alpha\n+beta\n"
        );
    }

    #[test]
    fn an_empty_new_text_is_all_deletions() {
        assert_eq!(
            unified("alpha\nbeta\n", ""),
            "@@ -1,2 +0,0 @@\n-alpha\n-beta\n"
        );
    }

    #[test]
    fn a_pure_insertion_shows_as_one_added_line() {
        assert_eq!(
            unified("alpha\nbeta\ngamma\n", "alpha\nbeta\nNEW\ngamma\n"),
            "@@ -1,3 +1,4 @@\n alpha\n beta\n+NEW\n gamma\n"
        );
    }

    #[test]
    fn a_pure_deletion_shows_as_one_removed_line() {
        assert_eq!(
            unified("alpha\nbeta\nOLD\ngamma\n", "alpha\nbeta\ngamma\n"),
            "@@ -1,4 +1,3 @@\n alpha\n beta\n-OLD\n gamma\n"
        );
    }

    #[test]
    fn a_replacement_shows_the_deletion_before_the_insertion() {
        assert_eq!(
            unified("alpha\nbeta\ngamma\n", "alpha\nBETA\ngamma\n"),
            "@@ -1,3 +1,3 @@\n alpha\n-beta\n+BETA\n gamma\n"
        );
    }

    #[test]
    fn nearby_changes_share_one_hunk() {
        let old = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
        let new = "one\nTWO\nthree\nfour\nFIVE\nsix\nseven\neight\nnine\nten\n";
        assert_eq!(
            unified(old, new),
            concat!(
                "@@ -1,8 +1,8 @@\n",
                " one\n",
                "-two\n",
                "+TWO\n",
                " three\n",
                " four\n",
                "-five\n",
                "+FIVE\n",
                " six\n",
                " seven\n",
                " eight\n"
            )
        );
    }

    #[test]
    fn distant_changes_produce_two_hunks_with_their_own_counts() {
        let old = numbered(16);
        let new = old.replace("line01", "LINE01").replace("line16", "LINE16");
        assert_eq!(
            unified(&old, &new),
            concat!(
                "@@ -1,4 +1,4 @@\n",
                "-line01\n",
                "+LINE01\n",
                " line02\n",
                " line03\n",
                " line04\n",
                "@@ -13,4 +13,4 @@\n",
                " line13\n",
                " line14\n",
                " line15\n",
                "-line16\n",
                "+LINE16\n"
            )
        );
    }

    #[test]
    fn context_never_exceeds_three_lines_per_side() {
        let old = numbered(12);
        let mut lines: Vec<&str> = old.lines().collect();
        lines.insert(6, "INSERTED");
        let new = lines.join("\n");
        assert_eq!(
            unified(&old, &new),
            concat!(
                "@@ -4,6 +4,7 @@\n",
                " line04\n",
                " line05\n",
                " line06\n",
                "+INSERTED\n",
                " line07\n",
                " line08\n",
                " line09\n"
            )
        );
    }
}
