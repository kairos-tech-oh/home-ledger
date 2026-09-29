//! Text and id cleaning, ported from the helper's `plain` and `valid_id`.

use uuid::Uuid;

pub const NAME_MAX: usize = 120;
pub const TICKER_MAX: usize = 16;
pub const NOTES_MAX: usize = 2000;
pub const LINE_NOTES_MAX: usize = 500;

/// Strip control characters, collapse runs of whitespace, and cut to a limit.
/// Counts characters rather than bytes so a multi-byte name is not cut mid-glyph.
pub fn plain(value: &str, limit: usize) -> String {
    let swapped: String = value
        .chars()
        .map(|c| {
            if (c as u32) < 32 || c as u32 == 127 {
                ' '
            } else {
                c
            }
        })
        .collect();
    swapped
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

/// A ticker is uppercase and drawn from a fixed alphabet, or it is nothing.
/// Anything without one is never sent to a quote API.
pub fn ticker(value: &str) -> String {
    let upper = plain(value, TICKER_MAX).to_ascii_uppercase();
    let ok = !upper.is_empty()
        && upper
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && upper
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-'));
    if ok { upper } else { String::new() }
}

/// Ids index the store and come back out of a writable file, so they are held
/// to a fixed alphabet even though they are spliced into nothing.
pub fn valid_id(value: &str) -> String {
    let ok = value.len() == 32
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    if ok { value.to_string() } else { String::new() }
}

pub fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Pick `value` when it is one of `allowed`, else `fallback`.
pub fn one_of(value: &str, allowed: &[&str], fallback: &str) -> String {
    if allowed.contains(&value) {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

/// A `yyyy-mm-dd` date, or empty. Validates the parts, so `2026-13-45` is empty.
pub fn iso_date(value: &str) -> String {
    let text = plain(value, 10);
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return String::new();
    }
    let (Ok(_y), Ok(m), Ok(d)) = (
        parts[0].parse::<u16>(),
        parts[1].parse::<u8>(),
        parts[2].parse::<u8>(),
    ) else {
        return String::new();
    };
    if (1..=12).contains(&m) && (1..=31).contains(&d) {
        text
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_become_spaces_and_runs_collapse() {
        assert_eq!(plain("a\u{0}\u{1}b   c\n\nd", 100), "a b c d");
    }

    #[test]
    fn a_name_is_cut_at_characters_not_bytes() {
        let cut = plain("ééééé", 3);
        assert_eq!(cut.chars().count(), 3);
    }

    #[test]
    fn a_ticker_is_uppercased_and_filtered() {
        assert_eq!(ticker("brk.b"), "BRK.B");
        assert_eq!(ticker("../etc/passwd"), "");
        assert_eq!(ticker("has space"), "");
    }

    #[test]
    fn only_a_32_char_lowercase_hex_id_is_valid() {
        let good = new_id();
        assert_eq!(valid_id(&good), good);
        assert_eq!(valid_id("short"), "");
        assert_eq!(valid_id(&"A".repeat(32)), "");
    }

    #[test]
    fn an_impossible_date_is_dropped() {
        assert_eq!(iso_date("2026-09-21"), "2026-09-21");
        assert_eq!(iso_date("2026-13-01"), "");
        assert_eq!(iso_date("2026-09-45"), "");
        assert_eq!(iso_date("not a date"), "");
    }
}
