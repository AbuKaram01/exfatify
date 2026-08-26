// Copyright (C) 2026 AbuKaram01
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared exFAT/Windows naming-rule constants, used by [`crate::checker`]
//! and [`crate::sanitizer`].
//!
//! exFAT's own on-disk format shares this rule set with Windows/NTFS —
//! Microsoft's own file-naming docs describe illegal characters, the
//! trailing space/period rule, and reserved device names as Windows
//! naming rules that apply "regardless of the file system", not as
//! something unique to exFAT.

/// Maximum filename length allowed by exFAT, in UTF-16 code units — not
/// bytes, not `char`s. See [`crate::checker::utf16_len`].
pub const MAX_NAME_UTF16: usize = 255;

/// Characters forbidden anywhere in an exFAT/Windows filename.
///
/// Control characters (`U+0000`–`U+001F`) are checked separately via a
/// numeric range instead of being listed here.
pub const ILLEGAL_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Windows device names reserved regardless of extension (e.g.
/// `NUL.tar.gz` is still reserved).
pub const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn illegal_chars_are_all_ascii_punctuation() {
        for &c in ILLEGAL_CHARS {
            assert!(c.is_ascii(), "expected ASCII char, got {:?}", c);
        }
    }

    #[test]
    fn illegal_chars_matches_the_documented_windows_exfat_set() {
        // Per Microsoft's own file-naming docs and the exFAT spec: the 9
        // characters below, plus control chars 0x00-0x1F (checked
        // separately in checker::needs_fix).
        let expected = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
        assert_eq!(ILLEGAL_CHARS.len(), expected.len());
        for c in expected {
            assert!(ILLEGAL_CHARS.contains(&c), "missing illegal char {c:?}");
        }
    }

    #[test]
    fn reserved_names_are_already_uppercase() {
        for name in RESERVED_NAMES {
            assert_eq!(*name, name.to_uppercase());
        }
    }

    #[test]
    fn reserved_names_has_one_entry_per_com_and_lpt_port() {
        let com_count = RESERVED_NAMES
            .iter()
            .filter(|n| n.starts_with("COM"))
            .count();
        let lpt_count = RESERVED_NAMES
            .iter()
            .filter(|n| n.starts_with("LPT"))
            .count();
        assert_eq!(com_count, 9);
        assert_eq!(lpt_count, 9);
    }
}
