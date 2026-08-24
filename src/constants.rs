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

//! Shared exFAT naming-rule constants, used by [`crate::checker`] and
//! [`crate::sanitizer`].

/// Maximum filename length allowed by exFAT, in UTF-16 code units — not
/// bytes, not `char`s. See [`crate::checker::utf16_len`].
pub const MAX_NAME_UTF16: usize = 255;

/// Characters forbidden anywhere in an exFAT filename.
///
/// Control characters (`U+0000`–`U+001F`) are checked separately via a
/// numeric range instead of being listed here.
pub const ILLEGAL_CHARS: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];

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
