// Copyright (C) 2026 AbuKaram01
// SPDX-License-Identifier: GPL-3.0-or-later

//! Run statistics and dual console/file logging.
//!
//! [`emit`] is the single output call site: every message goes to
//! stdout, and — if a log file is open — a plain-text (ANSI-stripped)
//! copy is appended to it, so the two never drift out of sync.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Tally of what happened during a [`crate::processor::process`] run.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Number of entries whose names violated an exFAT/Windows naming rule.
    pub found: usize,
    /// Number of entries actually renamed (fix mode only).
    pub fixed: usize,
    /// Number of entries skipped (symlinks, special files, non-UTF-8 names).
    pub skipped: usize,
    /// Number of entries that failed to process.
    pub errors: usize,
}

/// Strips ANSI escape sequences from `s`, returning a plain-text copy.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&ch) {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Prints `msg` to stdout and, if `log` is `Some`, appends a plain-text
/// copy to the log file.
pub fn emit(msg: &str, log: &mut Option<fs::File>) {
    println!("{}", msg);
    if let Some(ref mut f) = log {
        let _ = writeln!(f, "{}", strip_ansi(msg));
    }
}

/// Opens (creating or truncating) the log file at `path`.
///
/// On Unix, created with mode `0600` (owner-only), since the log can
/// contain full filesystem paths.
#[cfg(unix)]
pub fn open_log_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

/// Opens (creating or truncating) the log file at `path`.
///
/// Non-Unix fallback: no special permission bits are set.
#[cfg(not(unix))]
pub fn open_log_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::tempdir;

    #[test]
    fn strip_ansi_removes_color_codes() {
        let input = "\x1b[32m[OK]\x1b[0m clean";
        assert_eq!(strip_ansi(input), "[OK] clean");
    }

    #[test]
    fn strip_ansi_leaves_plain_text_untouched() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn emit_writes_stripped_text_to_log_file() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("run.log");
        let mut log = Some(open_log_file(&log_path).unwrap());

        emit("\x1b[31m[ERROR]\x1b[0m broken", &mut log);
        drop(log); // ensure the writer is flushed/closed before reading back

        let mut contents = String::new();
        fs::File::open(&log_path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();
        assert_eq!(contents.trim(), "[ERROR] broken");
    }

    #[test]
    fn emit_without_a_log_file_does_not_panic() {
        let mut log: Option<fs::File> = None;
        emit("just stdout, no file", &mut log);
    }

    #[test]
    fn stats_default_is_all_zero() {
        assert_eq!(
            Stats::default(),
            Stats {
                found: 0,
                fixed: 0,
                skipped: 0,
                errors: 0
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn open_log_file_sets_owner_only_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let log_path = dir.path().join("secret.log");
        let file = open_log_file(&log_path).unwrap();
        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
