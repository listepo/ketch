//! Plain text helpers the core and its renderers share.
//!
//! Kept apart from any renderer because the core needs them to build its own
//! messages — a download's size, a server's error snippet — and a front end
//! needs the same spelling so its output matches what the core said.

/// Human-readable byte count.
pub fn bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Shorten to `width`, ending with `…` when it does not fit.
pub fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut s: String = text.chars().take(keep).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_byte_counts() {
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate("abcdef", 10), "abcdef");
        assert_eq!(truncate("abcdef", 4), "abc…");
        // Multi-byte input must not panic or split a character.
        assert_eq!(truncate("ünïcödé-package", 6), "ünïcö…");
    }
}
