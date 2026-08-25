// Copyright (C) 2026  AbuKaram01
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Read-only checks: does a filename violate an exFAT naming rule?
//!
//! Nothing here modifies a name — pair [`needs_fix`] with
//! [`crate::sanitizer::sanitize`] to actually fix one.

use crate::constants::{ILLEGAL_CHARS, MAX_NAME_UTF16, RESERVED_NAMES};

/// Length of `s` in UTF-16 code units — exFAT's unit of measurement, not
/// bytes and not `char`s. A surrogate pair (most emoji) counts as 2, e.g.
/// `"😀"` is 2 units, not 1.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `true` if `name` breaks an exFAT naming rule: over 255 UTF-16 units, an
/// illegal or control character, a [reserved name](is_reserved), or a
/// leading space / trailing space / trailing period. Leading periods are
/// fine — dotfiles like `.bashrc` are untouched.
pub fn needs_fix(name: &str) -> bool {
    utf16_len(name) > MAX_NAME_UTF16
        || name
            .chars()
            .any(|c| ILLEGAL_CHARS.contains(&c) || (c as u32) <= 0x1F)
        || is_reserved(name)
        || name.starts_with(' ')
        || name.ends_with(' ')
        || name.ends_with('.')
}

/// `true` if `name`'s stem (before the first `.`) matches a
/// Windows-reserved device name, case-insensitively, regardless of
/// extension (`NUL.tar.gz` counts).
pub fn is_reserved(name: &str) -> bool {
    let stem = name.find('.').map(|i| &name[..i]).unwrap_or(name);
    let normalized = stem.trim_matches(|c: char| c == ' ' || c == '.');
    let upper = normalized.to_uppercase();
    RESERVED_NAMES.contains(&upper.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_len_counts_surrogate_pairs_not_chars() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("😀"), 2);
        assert_eq!(utf16_len(""), 0);
    }

    #[test]
    fn needs_fix_detects_every_illegal_char() {
        for &c in crate::constants::ILLEGAL_CHARS {
            let name = format!("file{c}name.txt");
            assert!(needs_fix(&name), "expected '{c}' to trigger needs_fix");
        }
    }

    #[test]
    fn needs_fix_detects_control_chars() {
        let name = "file\u{0007}name.txt"; // BEL
        assert!(needs_fix(name));
    }

    #[test]
    fn needs_fix_detects_trailing_space_or_period() {
        assert!(needs_fix("trailing "));
        assert!(needs_fix("trailing."));
        assert!(!needs_fix("trailing"));
    }

    #[test]
    fn needs_fix_detects_leading_space_but_not_leading_period() {
        assert!(needs_fix(" leading"));
        assert!(!needs_fix(".leading"));
    }

    #[test]
    fn needs_fix_detects_overlong_names() {
        let long_name = "a".repeat(300);
        assert!(needs_fix(&long_name));
    }

    #[test]
    fn needs_fix_accepts_clean_names_including_non_ascii() {
        assert!(!needs_fix("normal-file_name (1).txt"));
        assert!(!needs_fix("日本語のファイル名.txt"));
    }

    #[test]
    fn is_reserved_matches_case_insensitively_with_any_extension() {
        assert!(is_reserved("con"));
        assert!(is_reserved("CON"));
        assert!(is_reserved("Con.txt"));
        assert!(is_reserved("lpt9.tar.gz"));
    }

    #[test]
    fn is_reserved_rejects_near_misses() {
        assert!(!is_reserved("CONFIG"));
        assert!(!is_reserved("console.txt"));
        assert!(!is_reserved("COM10"));
    }

    #[test]
    fn is_reserved_trims_stray_padding_around_the_stem() {
        assert!(is_reserved(" NUL .txt"));
    }
}
