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

//! Black-box tests of the public sanitization API.
//!
//! Files under `tests/` are compiled by Cargo as separate crates that can
//! only see `exfatify`'s *public* surface — exactly the contract an
//! external integrator (e.g. a GUI front-end) is bound by. If something
//! here doesn't compile, it means the public API isn't actually usable
//! the way it was meant to be.

use exfatify::checker::{needs_fix, utf16_len};
use exfatify::sanitizer::{
    case_insensitive_match_exists, is_case_insensitive_duplicate, sanitize, unique_name,
};
use tempfile::tempdir;

/// Typical GUI workflow: list a directory, check each name, and build a
/// preview of "old name -> new name" pairs for names that need fixing —
/// without renaming anything yet.
#[test]
fn typical_gui_preview_workflow() {
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

/// Whatever `sanitize()` produces must itself pass `needs_fix()` as clean
/// — otherwise applying the fix once wouldn't actually be enough, which
/// would be a confusing bug for any UI built on top of this crate.
#[test]
fn sanitized_output_never_needs_fixing_again() {
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
            utf16_len(&cleaned) <= 255,
            "sanitize({input:?}) produced {cleaned:?}, which is over the 255-unit limit"
        );
    }
}

#[test]
fn utf16_length_respects_255_unit_cap_after_sanitizing() {
    let huge_name = format!("{}.txt", "a".repeat(1000));
    let cleaned = sanitize(&huge_name, '-');
    assert!(utf16_len(&cleaned) <= 255);
}

/// A GUI also needs to know about collisions that exFAT's
/// case-insensitivity creates between two otherwise-clean names — not
/// just the character/length/reserved-name problems `needs_fix` covers.
#[test]
fn case_insensitive_collisions_are_visible_through_the_public_api() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("Report.txt"), b"x").unwrap();

    // Neither name is individually a problem...
    assert!(!needs_fix("report.txt"));
    // ...but a GUI checking for siblings would still see the collision,
    // and unique_name() resolves it the same way it resolves any other
    // collision.
    assert!(case_insensitive_match_exists(
        dir.path(),
        "report.txt",
        None
    ));
    assert_eq!(unique_name(dir.path(), "report.txt", None), "report-1.txt");
}

/// A GUI building a live preview (à la `process()`'s scan mode) should
/// flag only one side of a case-insensitive pair, the same deterministic
/// way `process()` itself does — otherwise its preview count won't match
/// what actually happens when the user hits "Fix".
#[test]
fn duplicate_check_picks_exactly_one_side_of_a_pair_deterministically() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("Report.txt"), b"x").unwrap();
    std::fs::write(dir.path().join("report.txt"), b"y").unwrap();

    let first_is_dup = is_case_insensitive_duplicate(dir.path(), "Report.txt", None);
    let second_is_dup = is_case_insensitive_duplicate(dir.path(), "report.txt", None);
    assert_ne!(
        first_is_dup, second_is_dup,
        "exactly one side should be flagged"
    );
}
