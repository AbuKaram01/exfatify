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

//! Pure functions that transform a problematic filename into one that's
//! safe to write to an exFAT volume.
//!
//! Most of this module is side-effect free. The exceptions —
//! [`case_insensitive_match_exists`], [`is_case_insensitive_duplicate`],
//! and [`unique_name`] — have to check the filesystem to detect or avoid
//! name collisions, but only ever *read* (`fs::read_dir`); nothing here
//! writes.

use std::fs;
use std::path::Path;

use crate::checker::{is_reserved, utf16_len};
use crate::constants::{ILLEGAL_CHARS, MAX_NAME_UTF16};

/// Produces an exFAT-safe version of `name`.
///
/// Steps applied, in order:
/// 1. Replace every illegal character (and control character) with `replace`.
/// 2. Trim a leading space and trailing spaces/periods (leading *periods*
///    are left alone — dotfile-style names like `.bashrc` are normal on
///    exFAT; only leading *spaces* are part of this rule).
/// 3. Prefix with `_` if the result collides with a reserved device name.
/// 4. Fall back to `"unnamed_file"` if the result is empty.
/// 5. Truncate to 255 UTF-16 code units — preserving the file extension
///    only when the extension itself is short enough to leave room for a
///    base name — then re-trim the trailing case and re-check the empty
///    case. (Truncation only ever removes from the *end*, so it can't
///    reintroduce a leading space that step 2 already removed.)
///
/// This function does **not** check for collisions with other entries in
/// a directory (including case-insensitive ones — exFAT is case
/// *insensitive* but case *preserving*); pair it with [`unique_name`] for
/// that.
///
/// # Examples
///
/// ```
/// use exfat_sanitize::sanitizer::sanitize;
///
/// assert_eq!(sanitize("report*.txt", '-'), "report-.txt");
/// assert_eq!(sanitize("trailing space ", '-'), "trailing space");
/// assert_eq!(sanitize(" leading space", '-'), "leading space");
/// assert_eq!(sanitize(".bashrc", '-'), ".bashrc"); // leading period: untouched
/// assert_eq!(sanitize("NUL", '-'), "_NUL");
/// // Illegal characters are *replaced*, not stripped, so a name made
/// // entirely of them does not collapse to empty:
/// assert_eq!(sanitize("***", '-'), "---");
/// // Only an input that's nothing but trimmable leading/trailing
/// // space or trailing period disappears completely and falls back
/// // to a placeholder:
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
            // Only treat the tail after the last '.' as a preservable
            // extension if it actually leaves room for a base name. Without
            // this guard, a name whose only '.' is at position 0 (a long
            // dotfile, e.g. "." + 400 chars) or whose tail is itself huge
            // would compute `ext_units >= MAX_NAME_UTF16`, saturate the
            // allowed base length to 0, and return the ORIGINAL — still
            // too long — string unchanged. That's a real bug a naive
            // extension-preserving truncation falls into; this guard
            // closes it by falling back to a flat truncation instead.
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

/// Truncates `s` to at most `max_utf16` UTF-16 code units without ever
/// splitting a surrogate pair (which would produce invalid text).
///
/// Crate-internal: external callers should go through [`sanitize`], which
/// already wraps this with extension-preserving truncation logic.
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

/// Returns `true` if `dir` contains an entry — other than `exclude`, if
/// given — whose filename matches `name` case-*insensitively*.
///
/// exFAT is case insensitive but case preserving (confirmed by
/// [Microsoft's exFAT specification](https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification),
/// via its mandatory Up-case Table): `"Report.txt"` and `"report.txt"`
/// are the *same file* once written to an exFAT volume, even though most
/// source filesystems (ext4, a case-sensitive APFS volume, …) treat them
/// as distinct. A collision check that only does a literal/case-sensitive
/// lookup — e.g. a plain `Path::exists()` — will miss this and let two
/// distinct source files silently collapse into one on the target drive.
///
/// `exclude` lets a caller renaming a specific entry skip matching
/// against that entry's own (pre-rename) directory listing — without it,
/// a name that hasn't changed case would always "collide with itself".
///
/// # Known limitation
///
/// This uses Rust's standard Unicode uppercasing (`str::to_uppercase`)
/// rather than exFAT's literal, bundled Up-case Table. The two agree for
/// the vast majority of real-world names (all ASCII and almost all
/// Unicode case pairs), but can disagree for a handful of characters with
/// length-changing uppercase mappings (e.g. German `ß` → `"SS"`), which
/// the real Up-case Table — a strict one-to-one code point mapping —
/// would not fold the same way.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use exfat_sanitize::sanitizer::case_insensitive_match_exists;
///
/// // True if some other entry named e.g. "REPORT.TXT" or "Report.txt"
/// // already lives in this directory.
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

/// Returns `true` if `name` should be treated as "the duplicate" within
/// its case-insensitive group in `dir` — i.e. some *other* entry case-
/// insensitively matches `name` **and** sorts before it lexicographically.
///
/// [`case_insensitive_match_exists`] alone answers "does this collide with
/// something?", which is symmetric: in a colliding pair, *both* sides
/// answer yes. That symmetry is exactly right for [`unique_name`]'s job
/// (finding a slot that's free of every existing name), but it's the
/// wrong question for [`crate::processor::process`]'s "does this specific
/// entry need fixing?" decision — using it there would flag *both* members
/// of every pair during a scan (nothing renamed, so each keeps seeing the
/// other throughout), while a fix run would only ever end up renaming
/// *one* of them (the first one processed claims the original name on
/// disk, which then resolves the second one's check on the fly). Same
/// input directory, two different counts — a confusing mismatch between
/// what a preview reports and what a fix run actually does.
///
/// This function breaks that symmetry on purpose, by leaning on the fact
/// that two different-case variants of a name are never byte-identical,
/// so ordinary string comparison gives a total, unambiguous order: within
/// any case-insensitive group, exactly one entry — the lexicographically
/// smallest — is never anyone's duplicate (the "keeper"), and every other
/// member of the group is. That answer no longer depends on which order
/// the directory walk visits entries in, or on whether earlier renames in
/// the same run have already mutated the directory — so scan/dry-run and
/// fix mode always agree on the count, and fix mode always ends up
/// renaming the exact entry a preceding scan said it would.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use exfat_sanitize::sanitizer::is_case_insensitive_duplicate;
///
/// // If "Report.txt" and "report.txt" are both in this directory,
/// // "Report.txt" (sorts first) is the keeper and "report.txt" is the
/// // duplicate that needs disambiguating.
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

/// Given a desired `name`, returns a name guaranteed not to collide —
/// case-insensitively, matching exFAT's own semantics — with any other
/// entry in `dir`, appending `-1`, `-2`, … before the extension as needed.
///
/// `exclude`, if given, is the original path of the entry being renamed,
/// so it doesn't count as colliding with its own (not-yet-renamed) entry.
/// Pass `None` only when `name` is guaranteed not to be any existing
/// entry's current literal name (e.g. a purely hypothetical preview).
///
/// This is the only function in the crate that touches the filesystem
/// without being explicitly told to apply changes; it only ever *reads*
/// (via [`case_insensitive_match_exists`]), never writes.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
/// use exfat_sanitize::sanitizer::unique_name;
///
/// // If "photo.jpg" (in any case) already exists in this directory, this
/// // returns "photo-1.jpg" (or "-2", etc., as needed).
/// let name = unique_name(Path::new("/some/directory"), "photo.jpg", None);
/// ```
pub fn unique_name(dir: &Path, name: &str, exclude: Option<&Path>) -> String {
    if !case_insensitive_match_exists(dir, name, exclude) {
        return name.to_owned();
    }

    // Only preserve the tail after the last '.' as an "extension" if doing
    // so leaves room for a base name — same reasoning as in `sanitize`.
    // `name` here is already ≤ MAX_NAME_UTF16 (it came out of `sanitize`),
    // but it can still be a long dotfile-shaped name with no real
    // extension to speak of, and forcing a `-1` suffix in on top of an
    // already-maxed-out name needs the same guard to avoid overflowing
    // past the 255-unit cap.
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

/// Builds a `.bak` backup filename for `name`, truncating if necessary so
/// the result still fits within [`MAX_NAME_UTF16`] UTF-16 code units.
///
/// # Examples
///
/// ```
/// use exfat_sanitize::sanitizer::backup_name;
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

    /// Regression test for a real gap found before shipping: the FAT/exFAT
    /// long-name spec ignores leading spaces the same way it ignores
    /// trailing ones, but the original implementation only trimmed the
    /// trailing side.
    #[test]
    fn sanitize_trims_leading_space() {
        assert_eq!(sanitize(" name", '-'), "name");
        assert_eq!(sanitize("   name", '-'), "name");
        assert_eq!(sanitize(" name ", '-'), "name");
    }

    /// Leading *periods* are explicitly excluded from that same rule —
    /// dotfile-style names are completely normal on exFAT, so sanitize()
    /// must not touch them.
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
        // Trailing space/period get *trimmed*, so an input made entirely
        // of them disappears completely and falls back to "unnamed_file".
        assert_eq!(sanitize("...", '-'), "unnamed_file");
        assert_eq!(sanitize("   ", '-'), "unnamed_file");
        assert_eq!(sanitize("", '-'), "unnamed_file");
    }

    /// Illegal characters get *replaced*, not stripped, so a name made
    /// entirely of them does NOT collapse to empty/"unnamed_file" — it
    /// becomes a same-length string of the replacement character. This is
    /// the counterpart to the test above and was the source of a wrong
    /// assumption (and a wrong test assertion) caught in code review.
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

    /// Regression test for a real bug found before shipping: a long
    /// dotfile (the only '.' is at index 0) made the naive
    /// extension-preserving truncation treat the *entire* name as an
    /// "extension" worth preserving whole, so the result came back
    /// completely untruncated and still over the limit.
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

    /// Same bug, different shape: a normal-looking extension that is
    /// itself longer than the entire allowed name.
    #[test]
    fn sanitize_truncates_names_with_an_oversized_extension() {
        let pathological = format!("file.{}", "x".repeat(400));
        let result = sanitize(&pathological, '-');
        assert!(utf16_len(&result) <= MAX_NAME_UTF16);
    }

    #[test]
    fn truncate_to_utf16_never_splits_a_surrogate_pair() {
        // "😀" is 2 UTF-16 units; allowing only 1 must drop it entirely
        // rather than emit half a surrogate pair.
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

        // Without exclude, the file matches itself.
        assert!(case_insensitive_match_exists(
            dir.path(),
            "report.txt",
            None
        ));
        // With exclude pointing at the same entry, it no longer counts.
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

        // 'R' (0x52) sorts before 'r' (0x72), so "Report.txt" is the keeper.
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
        // The whole point of this function: the answer must not depend on
        // iteration/processing order, only on the (fixed) name strings.
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("photo.JPG"), b"x").unwrap();
        fs::write(dir.path().join("PHOTO.jpg"), b"y").unwrap();

        let lower_is_dup = is_case_insensitive_duplicate(dir.path(), "photo.JPG", None);
        let upper_is_dup = is_case_insensitive_duplicate(dir.path(), "PHOTO.jpg", None);
        // Exactly one of the two must be the duplicate, never both, never neither.
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

    /// The bug this whole pass was about: a sibling that only differs by
    /// case must still be treated as a collision, because exFAT would
    /// merge them.
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

        // "Report.txt" should not collide with itself when excluded, even
        // though it's still sitting on disk under its own (pre-rename) name.
        assert_eq!(
            unique_name(dir.path(), "Report.txt", Some(&path)),
            "Report.txt"
        );
    }

    #[test]
    fn unique_name_handles_suffixing_when_the_base_name_has_no_room_left() {
        let dir = tempdir().unwrap();
        // A 255-unit name with no real extension to preserve.
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
}
