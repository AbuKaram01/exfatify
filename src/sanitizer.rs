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

//! Turns a problematic filename into one that's safe on exFAT.
//!
//! [`case_insensitive_match_exists`], [`is_case_insensitive_duplicate`],
//! and [`unique_name`] read the filesystem to detect collisions; nothing
//! in this module writes.

use std::fs;
use std::path::Path;

use crate::checker::{is_reserved, utf16_len};
use crate::constants::{ILLEGAL_CHARS, MAX_NAME_UTF16};

/// Produces an exFAT-safe version of `name`:
///
/// 1. Replace each illegal/control character with `replace`.
/// 2. Trim a leading space and trailing spaces/periods (leading periods,
///    as in `.bashrc`, are left alone).
/// 3. Prefix with `_` if the result is a reserved device name.
/// 4. Fall back to `"unnamed_file"` if the result is empty.
/// 5. Truncate to 255 UTF-16 units, preserving the extension when there's
///    room for one.
///
/// Does not check for collisions with sibling entries — pair with
/// [`unique_name`] for that.
///
/// # Examples
///
/// ```
/// use crate::sanitizer::sanitize;
///
/// assert_eq!(sanitize("report*.txt", '-'), "report-.txt");
/// assert_eq!(sanitize(".bashrc", '-'), ".bashrc");
/// assert_eq!(sanitize("NUL", '-'), "_NUL");
/// assert_eq!(sanitize("...", '-'), "unnamed_file");
/// ```
pub fn sanitize(name: &str, replace: char) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if ILLEGAL_CHARS.contains(&c) || (c as u32) <= 0x1F {
                replace
            } else {
                c
            }
        })
        .collect();

    result = result
        .trim_start_matches(' ')
        .trim_end_matches([' ', '.'])
        .to_string();

    if is_reserved(&result) {
        result = format!("_{}", result);
    }

    if result.is_empty() {
        result = "unnamed_file".to_string();
    }

    if utf16_len(&result) > MAX_NAME_UTF16 {
        result = match result.rfind('.') {
            // Only preserve the tail as an extension if it leaves room for
            // a base name; otherwise fall back to a flat truncation.
            Some(dot_pos) => {
                let ext = &result[dot_pos..];
                let ext_units = utf16_len(ext);
                if ext_units < MAX_NAME_UTF16 {
                    let base = &result[..dot_pos];
                    let allowed_units = MAX_NAME_UTF16 - ext_units;
                    let truncated_base = truncate_to_utf16(base, allowed_units);
                    format!("{}{}", truncated_base, ext)
                } else {
                    truncate_to_utf16(&result, MAX_NAME_UTF16)
                }
            }
            None => truncate_to_utf16(&result, MAX_NAME_UTF16),
        };

        result = result.trim_end_matches([' ', '.']).to_string();
        if result.is_empty() {
            result = "unnamed_file".to_string();
        }
    }

    result
}

/// Truncates `s` to at most `max_utf16` UTF-16 code units without
/// splitting a surrogate pair.
///
/// Crate-internal — external callers should go through [`sanitize`].
pub(crate) fn truncate_to_utf16(s: &str, max_utf16: usize) -> String {
    let mut count = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        let units = ch.len_utf16();
        if count + units > max_utf16 {
            break;
        }
        out.push(ch);
        count += units;
    }
    out
}

/// `true` if `dir` contains an entry — other than `exclude`, if given —
/// whose name matches `name` case-*insensitively*.
///
/// exFAT is case-insensitive but case-preserving, so `"Report.txt"` and
/// `"report.txt"` collide there even though most source filesystems treat
/// them as distinct; a plain `Path::exists()` would miss that.
///
/// Uses `str::to_uppercase` rather than exFAT's bundled Up-case Table, so
/// a handful of length-changing case mappings (e.g. German `ß` → `"SS"`)
/// can disagree with the real table.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use crate::sanitizer::case_insensitive_match_exists;
///
/// let collides = case_insensitive_match_exists(Path::new("/some/directory"), "report.txt", None);
/// ```
pub fn case_insensitive_match_exists(dir: &Path, name: &str, exclude: Option<&Path>) -> bool {
    let target = name.to_uppercase();
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };

    entries.flatten().any(|entry| {
        if exclude.is_some_and(|skip| entry.path() == skip) {
            return false;
        }
        entry
            .file_name()
            .to_str()
            .map(|n| n.to_uppercase() == target)
            .unwrap_or(false)
    })
}

/// `true` if `name` is "the duplicate" within its case-insensitive group
/// in `dir` — some other entry matches it case-insensitively **and**
/// sorts before it lexicographically.
///
/// Unlike [`case_insensitive_match_exists`] (symmetric — both sides of a
/// colliding pair answer yes), this breaks the tie deterministically:
/// within any case-insensitive group, exactly one entry (the
/// lexicographically smallest) is the keeper, and every other member is
/// "the duplicate". That keeps scan and fix mode in agreement regardless
/// of directory-walk order.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use crate::sanitizer::is_case_insensitive_duplicate;
///
/// let dir = Path::new("/some/directory");
/// assert!(!is_case_insensitive_duplicate(dir, "Report.txt", None));
/// assert!(is_case_insensitive_duplicate(dir, "report.txt", None));
/// ```
pub fn is_case_insensitive_duplicate(dir: &Path, name: &str, exclude: Option<&Path>) -> bool {
    let target = name.to_uppercase();
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };

    entries.flatten().any(|entry| {
        if exclude.is_some_and(|skip| entry.path() == skip) {
            return false;
        }
        match entry.file_name().to_str() {
            Some(sibling) => sibling.to_uppercase() == target && sibling < name,
            None => false,
        }
    })
}

/// Returns a name guaranteed not to collide, case-insensitively, with any
/// other entry in `dir`, appending `-1`, `-2`, … before the extension as
/// needed.
///
/// `exclude`, if given, is the entry being renamed, so it doesn't collide
/// with its own not-yet-renamed self.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use crate::sanitizer::unique_name;
///
/// let name = unique_name(Path::new("/some/directory"), "photo.jpg", None);
/// ```
pub fn unique_name(dir: &Path, name: &str, exclude: Option<&Path>) -> String {
    if !case_insensitive_match_exists(dir, name, exclude) {
        return name.to_owned();
    }

    // Same extension-preserving-if-there's-room logic as `sanitize`.
    let (base, ext) = match name.rfind('.') {
        Some(dot) if utf16_len(&name[dot..]) < MAX_NAME_UTF16 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };

    let ext_units = utf16_len(ext);

    (1_u64..)
        .map(|i| {
            let suffix = format!("-{}", i);
            let suffix_units = utf16_len(&suffix);
            let allowed = MAX_NAME_UTF16
                .saturating_sub(ext_units)
                .saturating_sub(suffix_units);
            let safe_base = truncate_to_utf16(base, allowed);
            format!("{}{}{}", safe_base, suffix, ext)
        })
        .find(|candidate| !case_insensitive_match_exists(dir, candidate, exclude))
        .unwrap()
}

/// Builds a `.bak` filename for `name`, truncating if necessary to stay
/// within [`MAX_NAME_UTF16`].
///
/// # Examples
///
/// ```
/// use crate::sanitizer::backup_name;
///
/// assert_eq!(backup_name("photo.jpg"), "photo.jpg.bak");
/// ```
pub fn backup_name(name: &str) -> String {
    const BAK_SUFFIX: &str = ".bak";
    let suffix_units = utf16_len(BAK_SUFFIX);

    if utf16_len(name) + suffix_units <= MAX_NAME_UTF16 {
        return format!("{}{}", name, BAK_SUFFIX);
    }

    let allowed = MAX_NAME_UTF16.saturating_sub(suffix_units);
    format!("{}{}", truncate_to_utf16(name, allowed), BAK_SUFFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn sanitize_replaces_illegal_chars() {
        assert_eq!(sanitize("a:b*c?.txt", '_'), "a_b_c_.txt");
    }

    #[test]
    fn sanitize_trims_trailing_space_and_period() {
        assert_eq!(sanitize("name ", '-'), "name");
        assert_eq!(sanitize("name.", '-'), "name");
        assert_eq!(sanitize("name. . ", '-'), "name");
    }

    #[test]
    fn sanitize_trims_leading_space() {
        assert_eq!(sanitize(" name", '-'), "name");
        assert_eq!(sanitize("   name", '-'), "name");
        assert_eq!(sanitize(" name ", '-'), "name");
    }

    #[test]
    fn sanitize_leaves_leading_period_alone() {
        assert_eq!(sanitize(".bashrc", '-'), ".bashrc");
        assert_eq!(sanitize(".hidden.txt", '-'), ".hidden.txt");
    }

    #[test]
    fn sanitize_prefixes_reserved_names() {
        assert_eq!(sanitize("CON", '-'), "_CON");
        assert_eq!(sanitize("nul.txt", '-'), "_nul.txt");
    }

    #[test]
    fn sanitize_falls_back_when_result_is_empty() {
        assert_eq!(sanitize("...", '-'), "unnamed_file");
        assert_eq!(sanitize("   ", '-'), "unnamed_file");
        assert_eq!(sanitize("", '-'), "unnamed_file");
    }

    /// Replaced illegal chars must not collapse to empty — they become a
    /// same-length string of the replacement char instead.
    #[test]
    fn sanitize_does_not_treat_replaced_illegal_chars_as_empty() {
        assert_eq!(sanitize("***", '-'), "---");
    }

    #[test]
    fn sanitize_truncates_overlong_names_preserving_extension() {
        let long_name = format!("{}.txt", "a".repeat(300));
        let result = sanitize(&long_name, '-');
        assert!(utf16_len(&result) <= MAX_NAME_UTF16);
        assert!(result.ends_with(".txt"));
    }

    #[test]
    fn sanitize_truncates_overlong_names_without_extension() {
        let long_name = "a".repeat(300);
        let result = sanitize(&long_name, '-');
        assert_eq!(utf16_len(&result), MAX_NAME_UTF16);
    }

    #[test]
    fn sanitize_truncates_long_dotfiles_with_no_real_extension() {
        let long_dotfile = format!(".{}", "a".repeat(400));
        let result = sanitize(&long_dotfile, '-');
        assert!(
            utf16_len(&result) <= MAX_NAME_UTF16,
            "expected truncation, got {} UTF-16 units",
            utf16_len(&result)
        );
    }

    #[test]
    fn sanitize_truncates_names_with_an_oversized_extension() {
        let pathological = format!("file.{}", "x".repeat(400));
        let result = sanitize(&pathological, '-');
        assert!(utf16_len(&result) <= MAX_NAME_UTF16);
    }

    #[test]
    fn truncate_to_utf16_never_splits_a_surrogate_pair() {
        assert_eq!(truncate_to_utf16("😀", 1), "");
        assert_eq!(truncate_to_utf16("😀", 2), "😀");
    }

    #[test]
    fn case_insensitive_match_exists_finds_a_different_case_sibling() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Report.txt"), b"x").unwrap();
        assert!(case_insensitive_match_exists(
            dir.path(),
            "report.TXT",
            None
        ));
        assert!(!case_insensitive_match_exists(
            dir.path(),
            "other.txt",
            None
        ));
    }

    #[test]
    fn case_insensitive_match_exists_respects_exclude() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Report.txt");
        fs::write(&path, b"x").unwrap();

        assert!(case_insensitive_match_exists(
            dir.path(),
            "report.txt",
            None
        ));
        assert!(!case_insensitive_match_exists(
            dir.path(),
            "report.txt",
            Some(&path)
        ));
    }

    #[test]
    fn is_case_insensitive_duplicate_picks_the_lexicographically_smaller_name_as_keeper() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Report.txt"), b"x").unwrap();
        fs::write(dir.path().join("report.txt"), b"y").unwrap();

        assert!(!is_case_insensitive_duplicate(
            dir.path(),
            "Report.txt",
            None
        ));
        assert!(is_case_insensitive_duplicate(
            dir.path(),
            "report.txt",
            None
        ));
    }

    #[test]
    fn is_case_insensitive_duplicate_agrees_regardless_of_which_one_is_checked_first() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("photo.JPG"), b"x").unwrap();
        fs::write(dir.path().join("PHOTO.jpg"), b"y").unwrap();

        let lower_is_dup = is_case_insensitive_duplicate(dir.path(), "photo.JPG", None);
        let upper_is_dup = is_case_insensitive_duplicate(dir.path(), "PHOTO.jpg", None);
        assert_ne!(lower_is_dup, upper_is_dup);
    }

    #[test]
    fn is_case_insensitive_duplicate_is_false_with_no_matching_sibling() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("unique_name.txt"), b"x").unwrap();
        assert!(!is_case_insensitive_duplicate(
            dir.path(),
            "unique_name.txt",
            None
        ));
    }

    #[test]
    fn unique_name_returns_input_when_no_collision() {
        let dir = tempdir().unwrap();
        assert_eq!(unique_name(dir.path(), "photo.jpg", None), "photo.jpg");
    }

    #[test]
    fn unique_name_appends_suffix_on_collision() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("photo.jpg"), b"x").unwrap();
        assert_eq!(unique_name(dir.path(), "photo.jpg", None), "photo-1.jpg");
    }

    #[test]
    fn unique_name_increments_past_multiple_collisions() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("photo.jpg"), b"x").unwrap();
        fs::write(dir.path().join("photo-1.jpg"), b"x").unwrap();
        fs::write(dir.path().join("photo-2.jpg"), b"x").unwrap();
        assert_eq!(unique_name(dir.path(), "photo.jpg", None), "photo-3.jpg");
    }

    #[test]
    fn unique_name_treats_different_case_as_a_collision() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Report.txt"), b"x").unwrap();
        assert_eq!(unique_name(dir.path(), "report.txt", None), "report-1.txt");
    }

    #[test]
    fn unique_name_excludes_the_entry_being_renamed_from_its_own_collision_check() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("Report.txt");
        fs::write(&path, b"x").unwrap();

        assert_eq!(
            unique_name(dir.path(), "Report.txt", Some(&path)),
            "Report.txt"
        );
    }

    #[test]
    fn unique_name_handles_suffixing_when_the_base_name_has_no_room_left() {
        let dir = tempdir().unwrap();
        let maxed_out = "a".repeat(MAX_NAME_UTF16);
        fs::write(dir.path().join(&maxed_out), b"x").unwrap();

        let result = unique_name(dir.path(), &maxed_out, None);
        assert!(
            utf16_len(&result) <= MAX_NAME_UTF16,
            "suffixed name overflowed the limit"
        );
        assert_ne!(result, maxed_out, "should have been disambiguated");
    }

    #[test]
    fn backup_name_appends_bak_suffix() {
        assert_eq!(backup_name("photo.jpg"), "photo.jpg.bak");
    }

    #[test]
    fn backup_name_truncates_when_needed() {
        let long_name = "a".repeat(300);
        let result = backup_name(&long_name);
        assert!(utf16_len(&result) <= MAX_NAME_UTF16);
        assert!(result.ends_with(".bak"));
    }

    #[test]
    fn preview_workflow_flags_only_names_that_need_fixing() {
        use crate::checker::needs_fix;

        let candidate_names = [
            "vacation_photo.jpg",
            "invoice<final>.pdf",
            "notes ",
            "NUL.txt",
        ];

        let preview: Vec<(&str, Option<String>)> = candidate_names
            .iter()
            .map(|&name| {
                if needs_fix(name) {
                    (name, Some(sanitize(name, '-')))
                } else {
                    (name, None)
                }
            })
            .collect();

        assert_eq!(preview[0], ("vacation_photo.jpg", None));
        assert_eq!(preview[1].1.as_deref(), Some("invoice-final-.pdf"));
        assert_eq!(preview[2].1.as_deref(), Some("notes"));
        assert_eq!(preview[3].1.as_deref(), Some("_NUL.txt"));
    }

    /// Property check across a batch of pathological inputs at once.
    #[test]
    fn sanitize_output_never_needs_fixing_again_for_various_inputs() {
        use crate::checker::needs_fix;

        let long_dotfile = format!(".{}", "a".repeat(400));
        let oversized_extension = format!("file.{}", "x".repeat(400));
        let inputs = [
            "weird:name*.txt",
            "trailing.",
            " leading space.txt",
            "CON",
            &"x".repeat(400),
            "***",
            long_dotfile.as_str(),
            oversized_extension.as_str(),
        ];

        for input in inputs {
            let cleaned = sanitize(input, '-');
            assert!(
                !needs_fix(&cleaned),
                "sanitize({input:?}) produced {cleaned:?}, which still needs fixing"
            );
            assert!(
                utf16_len(&cleaned) <= MAX_NAME_UTF16,
                "sanitize({input:?}) produced {cleaned:?}, which is over the 255-unit limit"
            );
        }
    }
}
