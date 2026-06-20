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

//! exFAT naming-rule constants.
//!
//! These values come directly from the exFAT specification and, for the
//! reserved device names, from the Windows naming conventions that exFAT
//! inherits in practice (most exFAT volumes get plugged into a Windows
//! machine at some point). Nothing in this module performs any logic — it
//! only centralizes the "magic values" so [`crate::checker`] and
//! [`crate::sanitizer`] can't drift out of sync with each other.

/// Maximum filename length allowed by exFAT, measured in UTF-16 code units.
///
/// This is **not** bytes (UTF-8 length) and **not** `char` count (Unicode
/// scalar values) — see [`crate::checker::utf16_len`] for why that
/// distinction matters.
pub const MAX_NAME_UTF16: usize = 255;

/// Characters that exFAT forbids anywhere in a filename.
///
/// Control characters (`U+0000`–`U+001F`) are also forbidden but are
/// checked separately via a numeric comparison rather than listed here,
/// since there are 32 of them and listing them would add noise without
/// adding clarity.
pub const ILLEGAL_CHARS: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];

/// Device names reserved by Windows.
///
/// exFAT itself doesn't reserve these names, but volumes formatted exFAT
/// are routinely shared with or read by Windows, where a file or folder
/// named e.g. `NUL` — or `NUL.tar.gz`, since the restriction applies
/// regardless of extension — causes confusing failures. Sanitizing them
/// proactively avoids that class of bug entirely.
pub const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn illegal_chars_are_all_ascii_punctuation() {
        // Sanity check: every illegal char should be a single ASCII byte,
        // since that's what every exFAT/Windows naming-rule reference lists.
        for &c in ILLEGAL_CHARS {
            assert!(c.is_ascii(), "expected ASCII char, got {:?}", c);
        }
    }

    #[test]
    fn reserved_names_are_already_uppercase() {
        // checker::is_reserved() uppercases before comparing, so this list
        // must itself be uppercase or that comparison would silently fail.
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
