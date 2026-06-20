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

//! Read-only inspection of filenames against exFAT's naming rules.
//!
//! Functions in this module never modify anything — they only answer the
//! question "is this name a problem?". Once [`needs_fix`] says yes, hand
//! the name to [`crate::sanitizer::sanitize`] to actually produce a fixed
//! version.

use crate::constants::{ILLEGAL_CHARS, MAX_NAME_UTF16, RESERVED_NAMES};

/// Returns the length of `s` in UTF-16 code units.
///
/// exFAT (like NTFS and the Win32 API in general) measures filename length
/// in UTF-16 code units, not bytes and not `char`s. A character outside
/// the Basic Multilingual Plane (most emoji, for example) is stored as a
/// surrogate *pair* and therefore counts as 2 units — different from both
/// `str::len()` (UTF-8 bytes) and `str::chars().count()` (1 per `char`).
///
/// # Examples
///
/// ```
/// use exfatify::checker::utf16_len;
///
/// assert_eq!(utf16_len("hello"), 5);
/// assert_eq!(utf16_len("😀"), 2); // outside the BMP -> surrogate pair
/// ```
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Returns `true` if `name` violates any exFAT naming rule and therefore
/// needs to be sanitized before the entry can be safely copied to an
/// exFAT volume.
///
/// Checks performed:
/// - Length over 255 UTF-16 code units ([`crate::constants::MAX_NAME_UTF16`]).
/// - An illegal character ([`crate::constants::ILLEGAL_CHARS`]) or a
///   control character (`U+0000`–`U+001F`).
/// - A [reserved device name](is_reserved).
/// - A leading space, or a trailing space or period.
///
/// The leading/trailing-space and trailing-period rule comes from the
/// FAT/exFAT long-name specification itself ("leading and trailing
/// spaces in a long name are ignored... trailing periods are ignored"),
/// not just from Windows Explorer's UI — Microsoft's own exFAT
/// documentation and direct testing against the Win32 `CopyFile` API
/// confirm trailing spaces/periods get silently trimmed on write.
/// (Leading periods are explicitly **not** part of this rule and are left
/// alone — dotfile-style names like `.bashrc` are completely normal on
/// exFAT.)
///
/// # Examples
///
/// ```
/// use exfatify::checker::needs_fix;
///
/// assert!(needs_fix("report*.txt"));      // illegal char
/// assert!(needs_fix("notes "));           // trailing space
/// assert!(needs_fix(" notes"));           // leading space
/// assert!(needs_fix("NUL.txt"));          // reserved name
/// assert!(!needs_fix(".bashrc"));         // leading period is fine
/// assert!(!needs_fix("normal_file.txt")); // already fine
/// ```
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

/// Returns `true` if `name`'s stem (everything before the first `.`)
/// matches a Windows-reserved device name, case-insensitively, after
/// trimming surrounding spaces and periods.
///
/// The check applies regardless of extension: `NUL.tar.gz` is just as
/// reserved as plain `NUL`, since Windows reserves the name at the device
/// level rather than the file level.
///
/// # Examples
///
/// ```
/// use exfatify::checker::is_reserved;
///
/// assert!(is_reserved("NUL"));
/// assert!(is_reserved("nul.txt"));
/// assert!(is_reserved("COM1.tar.gz"));
/// assert!(!is_reserved("NULL")); // not an exact match, so it's safe
/// ```
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

    /// Per the FAT/exFAT long-name spec, leading spaces are ignored the
    /// same way trailing ones are — but leading *periods* are explicitly
    /// allowed (dotfile-style names are normal on exFAT), so only the
    /// space case should be flagged here.
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
