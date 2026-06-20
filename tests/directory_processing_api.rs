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

//! Black-box test of the full directory-processing API
//! ([`exfat_sanitize::processor::process`]) — the entry point a GUI would
//! call after letting the user pick a folder and a mode (scan / dry-run /
//! fix). This is the closest thing to an "integration test" in the crate:
//! it exercises traversal, sanitization, collision-avoidance, and backup
//! all together against a real (temporary) directory.

use std::fs;

use exfat_sanitize::cli::Args;
use exfat_sanitize::logger::Stats;
use exfat_sanitize::processor::process;
use tempfile::tempdir;

/// Builds an [`Args`] value the way a GUI would: directly from user
/// choices, without going through `clap`'s command-line parser.
fn gui_args(path: std::path::PathBuf, fix: bool) -> Args {
    Args {
        path,
        scan: !fix,
        fix,
        dry_run: false,
        replace: '_',
        verbose: false,
        log: None,
        backup: true,
        no_symlinks: false,
    }
}

#[test]
fn end_to_end_scan_then_fix() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("good.txt"), b"a").unwrap();
    fs::write(dir.path().join("bad?name.txt"), b"b").unwrap();

    // Step 1: scan — a GUI would render this as a preview list for the user.
    let scan_args = gui_args(dir.path().to_path_buf(), false);
    let mut scan_stats = Stats::default();
    process(&scan_args, &mut scan_stats, &mut None);
    assert_eq!(scan_stats.found, 1);
    assert!(
        dir.path().join("bad?name.txt").exists(),
        "scan must not rename"
    );

    // Step 2: the user clicks "Fix" — apply for real.
    let fix_args = gui_args(dir.path().to_path_buf(), true);
    let mut fix_stats = Stats::default();
    process(&fix_args, &mut fix_stats, &mut None);
    assert_eq!(fix_stats.fixed, 1);
    assert!(!dir.path().join("bad?name.txt").exists());
    assert!(dir.path().join("bad_name.txt").exists());
    assert!(
        dir.path().join("bad?name.txt.bak").exists(),
        "backup should be kept"
    );
}

#[test]
fn nested_directories_are_processed_innermost_first() {
    let dir = tempdir().unwrap();
    let bad_dir = dir.path().join("bad:dir");
    fs::create_dir(&bad_dir).unwrap();
    fs::write(bad_dir.join("bad*file.txt"), b"x").unwrap();

    let args = gui_args(dir.path().to_path_buf(), true);
    let mut stats = Stats::default();
    process(&args, &mut stats, &mut None);

    assert_eq!(stats.found, 2); // the directory itself, and the file inside it
    assert_eq!(stats.fixed, 2);
    assert!(dir.path().join("bad_dir").is_dir());
    assert!(dir.path().join("bad_dir").join("bad_file.txt").exists());
}

#[test]
fn colliding_sanitized_names_do_not_overwrite_each_other() {
    let dir = tempdir().unwrap();
    // Both of these sanitize to "report_.txt" with replace = '_'.
    fs::write(dir.path().join("report*.txt"), b"first").unwrap();
    fs::write(dir.path().join("report?.txt"), b"second").unwrap();

    let args = gui_args(dir.path().to_path_buf(), true);
    let mut stats = Stats::default();
    process(&args, &mut stats, &mut None);

    assert_eq!(stats.fixed, 2);
    // Neither original file's content was lost to a silent overwrite.
    let mut contents: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "txt")
                .unwrap_or(false)
        })
        .map(|e| fs::read_to_string(e.path()).unwrap())
        .collect();
    contents.sort();
    assert_eq!(contents, vec!["first".to_string(), "second".to_string()]);
}

/// End-to-end version of the headline bug fix from this audit pass: two
/// files that are each individually exFAT-legal — neither has an illegal
/// character, a reserved name, or a bad length — still collide once
/// copied to a real exFAT volume, because exFAT is case-insensitive.
/// A GUI relying solely on `needs_fix()` per name would miss this
/// entirely; `process()` must catch it too.
#[test]
fn scan_then_fix_catches_a_collision_invisible_to_per_name_checks() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Vacation.JPG"), b"first").unwrap();
    fs::write(dir.path().join("vacation.jpg"), b"second").unwrap();

    let scan_args = gui_args(dir.path().to_path_buf(), false);
    let mut scan_stats = Stats::default();
    process(&scan_args, &mut scan_stats, &mut None);
    assert_eq!(
        scan_stats.found, 1,
        "scan should flag exactly one side of the pair"
    );

    let fix_args = gui_args(dir.path().to_path_buf(), true);
    let mut fix_stats = Stats::default();
    process(&fix_args, &mut fix_stats, &mut None);
    assert_eq!(fix_stats.fixed, 1);

    // 'V' (0x56) sorts before 'v' (0x76): "Vacation.JPG" is the keeper,
    // "vacation.jpg" is the one disambiguated to "vacation-1.jpg". Note
    // gui_args() also sets backup: true, so the rename additionally
    // leaves "vacation.jpg.bak" behind — three files on disk afterward,
    // not two. (A test bug caught here on the first real run: this
    // assertion originally expected only two files and didn't account
    // for the backup copy, which made a perfectly correct run look like
    // a content-loss bug.)
    assert!(
        dir.path().join("Vacation.JPG").exists(),
        "the keeper must be untouched"
    );
    assert!(dir.path().join("vacation-1.jpg").exists());
    assert!(dir.path().join("vacation.jpg.bak").exists());

    assert_eq!(
        fs::read_to_string(dir.path().join("Vacation.JPG")).unwrap(),
        "first"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("vacation-1.jpg")).unwrap(),
        "second"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("vacation.jpg.bak")).unwrap(),
        "second"
    );
}
