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

//! Directory tree traversal: applies [`crate::checker`] and
//! [`crate::sanitizer`] to every entry under a root path.
//!
//! The only module that performs bulk filesystem writes.

use std::fs;

use colored::*;
use walkdir::WalkDir;

use crate::checker::needs_fix;
use crate::cli::Args;
use crate::logger::{emit, Stats};
use crate::sanitizer::{backup_name, is_case_insensitive_duplicate, sanitize, unique_name};

/// Walks `args.path` and, for every entry whose name either violates an
/// exFAT/Windows naming rule or is the "loser" of a case-insensitive collision
/// with a sibling, reports it (scan/dry-run) or renames it (fix),
/// updating `stats` and writing through [`emit`] as it goes.
///
/// Traversal is `contents_first`: children are renamed before their
/// parent directory, so a renamed parent never invalidates paths still
/// queued beneath it.
///
/// # Behavior by mode
///
/// - **Scan** (default) / **dry-run**: reports only, renames nothing.
/// - **Fix**: renames on disk; backs up regular files first if
///   `args.backup` is set.
///
/// Symlinks are renamed (never their target) by default; pass
/// `--no-symlinks` to skip them. Symlinks are never backed up, even with
/// `--backup` — `fs::copy` follows symlinks, which would either copy the
/// target's full contents or fail outright on a dangling link.
///
/// See this module's own `#[cfg(test)]` block for end-to-end examples.
pub fn process(args: &Args, stats: &mut Stats, log: &mut Option<fs::File>) {
    let readonly = args.is_readonly();

    let walker = WalkDir::new(&args.path)
        .contents_first(true)
        .follow_links(false);

    for entry_result in walker {
        let entry = match entry_result {
            Ok(e) => e,
            Err(err) => {
                emit(&format!("  {} {}", "[ERROR]".red().bold(), err), log);
                stats.errors += 1;
                continue;
            }
        };

        let path = entry.path();
        if path == args.path {
            continue;
        }

        let file_type = entry.file_type();
        if !file_type.is_file() && !file_type.is_dir() && !entry.path_is_symlink() {
            if args.verbose {
                emit(
                    &format!(
                        "  {} {} {}",
                        "[SKIP]".yellow(),
                        path.display(),
                        "(special file — device/socket/pipe)".dimmed()
                    ),
                    log,
                );
            }
            stats.skipped += 1;
            continue;
        }

        let is_symlink = entry.path_is_symlink();

        if is_symlink {
            if args.no_symlinks {
                emit(
                    &format!(
                        "  {} {} {}",
                        "[SKIP]".yellow(),
                        path.display(),
                        "(symlink)".dimmed()
                    ),
                    log,
                );
                stats.skipped += 1;
                continue;
            }

            if args.fix && !args.dry_run {
                emit(
                    &format!(
                        "  {} {} {}",
                        "[WARN]".yellow().bold(),
                        path.display(),
                        "(renaming symlink — may break references to the old name)".yellow()
                    ),
                    log,
                );
            } else if args.verbose {
                emit(
                    &format!(
                        "  {} {} {}",
                        "[NOTE]".cyan(),
                        path.display(),
                        "(symlink — will rename the link, not its target)".dimmed()
                    ),
                    log,
                );
            }
        }

        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => {
                emit(
                    &format!(
                        "  {} {} {}",
                        "[SKIP]".yellow(),
                        path.display(),
                        "(non-UTF-8 name)".dimmed()
                    ),
                    log,
                );
                stats.skipped += 1;
                continue;
            }
        };

        let dir = match path.parent() {
            Some(d) => d,
            None => {
                emit(
                    &format!(
                        "  {} no parent dir: {}",
                        "[ERROR]".red().bold(),
                        path.display()
                    ),
                    log,
                );
                stats.errors += 1;
                continue;
            }
        };

        // A name can be a problem two ways: the string itself violates a
        // rule (`needs_fix`), or it's the "loser" of a case-insensitive
        // collision with a sibling (see `is_case_insensitive_duplicate`).
        let is_collision_duplicate = is_case_insensitive_duplicate(dir, name, Some(path));

        if !needs_fix(name) && !is_collision_duplicate {
            if args.verbose {
                emit(&format!("  {} {}", "[OK]".green(), path.display()), log);
            }
            continue;
        }

        stats.found += 1;

        let clean = sanitize(name, args.replace);
        let final_name = unique_name(dir, &clean, Some(path));
        let new_path = dir.join(&final_name);

        if readonly {
            let label = if args.dry_run {
                "[DRY-RUN]".blue().bold().to_string()
            } else {
                "[PROBLEM]".yellow().bold().to_string()
            };
            emit(&format!("  {} {}", label, path.display()), log);
            emit(
                &format!("            -> would become: {}", final_name.bold()),
                log,
            );
            continue;
        }

        let is_dir = file_type.is_dir();

        // Only back up regular files: directories have nothing to copy,
        // and symlinks are skipped deliberately (see doc comment above).
        if args.backup && !is_dir && !is_symlink {
            let bak_name = backup_name(name);
            let bak_path = dir.join(&bak_name);
            if let Err(e) = fs::copy(path, &bak_path) {
                emit(
                    &format!(
                        "  {} backup failed for {}: {}",
                        "[ERROR]".red().bold(),
                        path.display(),
                        e
                    ),
                    log,
                );
                stats.errors += 1;
                continue;
            }
        }

        match fs::rename(path, &new_path) {
            Ok(_) => {
                stats.fixed += 1;
                emit(&format!("  {} {}", "[FIXED]".green().bold(), name), log);
                emit(&format!("            -> {}", final_name.bold()), log);
            }
            Err(e) => {
                stats.errors += 1;
                emit(
                    &format!("  {} {}: {}", "[ERROR]".red().bold(), path.display(), e),
                    log,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    /// Builds an [`Args`] value directly, without going through `clap`'s
    /// command-line parser.
    fn args_for(path: PathBuf, fix: bool, dry_run: bool) -> Args {
        Args {
            path,
            scan: !fix && !dry_run,
            fix,
            dry_run,
            replace: '-',
            verbose: false,
            log: None,
            backup: false,
            no_symlinks: false,
        }
    }

    #[test]
    fn scan_mode_reports_but_does_not_rename() {
        let dir = tempdir().unwrap();
        let bad_path = dir.path().join("bad*name.txt");
        fs::write(&bad_path, b"x").unwrap();

        let args = args_for(dir.path().to_path_buf(), false, false);
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.found, 1);
        assert_eq!(stats.fixed, 0);
        assert!(bad_path.exists(), "scan mode must not rename anything");
    }

    #[test]
    fn dry_run_reports_but_does_not_rename() {
        let dir = tempdir().unwrap();
        let bad_path = dir.path().join("bad*name.txt");
        fs::write(&bad_path, b"x").unwrap();

        let args = args_for(dir.path().to_path_buf(), true, true);
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.found, 1);
        assert_eq!(stats.fixed, 0);
        assert!(bad_path.exists(), "dry-run must not rename anything");
    }

    #[test]
    fn fix_mode_renames_bad_files() {
        let dir = tempdir().unwrap();
        let bad_path = dir.path().join("bad*name.txt");
        fs::write(&bad_path, b"x").unwrap();

        let args = args_for(dir.path().to_path_buf(), true, false);
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.fixed, 1);
        assert!(!bad_path.exists());
        assert!(dir.path().join("bad-name.txt").exists());
    }

    #[test]
    fn fix_with_backup_preserves_original_content() {
        let dir = tempdir().unwrap();
        let bad_path = dir.path().join("bad*name.txt");
        fs::write(&bad_path, b"original content").unwrap();

        let mut args = args_for(dir.path().to_path_buf(), true, false);
        args.backup = true;
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        let backup_path = dir.path().join("bad*name.txt.bak");
        assert_eq!(fs::read(&backup_path).unwrap(), b"original content");
    }

    #[test]
    fn directories_are_never_backed_up() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("bad:dir")).unwrap();

        let mut args = args_for(dir.path().to_path_buf(), true, false);
        args.backup = true;
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.fixed, 1);
        assert!(!dir.path().join("bad:dir.bak").exists());
    }

    #[test]
    fn clean_names_are_left_untouched() {
        let dir = tempdir().unwrap();
        let clean_path = dir.path().join("clean_name.txt");
        fs::write(&clean_path, b"x").unwrap();

        let args = args_for(dir.path().to_path_buf(), true, false);
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.found, 0);
        assert!(clean_path.exists());
    }

    #[test]
    fn nested_bad_names_are_fixed_innermost_first() {
        let dir = tempdir().unwrap();
        let bad_dir = dir.path().join("bad:dir");
        fs::create_dir(&bad_dir).unwrap();
        fs::write(bad_dir.join("bad*file.txt"), b"x").unwrap();

        let args = args_for(dir.path().to_path_buf(), true, false);
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.found, 2);
        assert_eq!(stats.fixed, 2);
        assert!(dir.path().join("bad-dir").join("bad-file.txt").exists());
    }

    #[test]
    fn fix_mode_separates_case_insensitive_collisions_between_clean_names() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Report.txt"), b"first").unwrap();
        fs::write(dir.path().join("report.txt"), b"second").unwrap();

        let args = args_for(dir.path().to_path_buf(), true, false);
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(
            stats.found, 1,
            "exactly one of the pair needed disambiguating"
        );
        assert_eq!(stats.fixed, 1);

        // 'R' (0x52) sorts before 'r' (0x72): "Report.txt" is the keeper.
        assert!(
            dir.path().join("Report.txt").exists(),
            "the keeper must be untouched"
        );
        assert!(!dir.path().join("report.txt").exists());
        assert!(dir.path().join("report-1.txt").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("Report.txt")).unwrap(),
            "first"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("report-1.txt")).unwrap(),
            "second"
        );
    }

    #[test]
    fn scan_mode_agrees_with_fix_mode_on_case_insensitive_collision_count() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("Report.txt"), b"first").unwrap();
        fs::write(dir.path().join("report.txt"), b"second").unwrap();

        let scan_args = args_for(dir.path().to_path_buf(), false, false);
        let mut scan_stats = Stats::default();
        process(&scan_args, &mut scan_stats, &mut None);

        assert_eq!(scan_stats.found, 1);
        assert_eq!(scan_stats.fixed, 0);
        assert!(dir.path().join("Report.txt").exists());
        assert!(dir.path().join("report.txt").exists());
    }

    /// Two originally-different bad names that both sanitize to the same
    /// clean name must not overwrite each other.
    #[test]
    fn colliding_sanitized_names_do_not_overwrite_each_other() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("report*.txt"), b"first").unwrap();
        fs::write(dir.path().join("report?.txt"), b"second").unwrap();

        let mut args = args_for(dir.path().to_path_buf(), true, false);
        args.replace = '_';
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(stats.fixed, 2);
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

    /// A dangling symlink must not make `--backup` fail — `fs::copy`
    /// follows symlinks, so backing one up would otherwise error out.
    #[cfg(unix)]
    #[test]
    fn fix_with_backup_does_not_choke_on_a_dangling_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let link_path = dir.path().join("broken*link");
        symlink(dir.path().join("does-not-exist"), &link_path).unwrap();

        let mut args = args_for(dir.path().to_path_buf(), true, false);
        args.backup = true;
        let mut stats = Stats::default();
        process(&args, &mut stats, &mut None);

        assert_eq!(
            stats.errors, 0,
            "a dangling symlink must not produce a backup error"
        );
        assert_eq!(stats.fixed, 1);
        assert!(dir.path().join("broken-link").is_symlink());
        assert!(
            !dir.path().join("broken*link.bak").exists(),
            "symlinks are never backed up"
        );
    }
}
