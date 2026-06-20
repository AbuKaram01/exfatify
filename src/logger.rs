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

//! Run statistics and dual console/file logging.
//!
//! [`emit`] is the single place output happens: every message goes to
//! stdout, and — if a log file was opened — a stripped (no ANSI color
//! codes) copy is also appended to it. Keeping one call site means colored
//! terminal output and a clean, `grep`-able log file can never drift out
//! of sync.

use std::fs;
use std::io::Write;
use std::path::Path;

/// Tally of what happened during a [`crate::processor::process`] run.
///
/// A GUI front-end can read these fields after a run completes to render
/// a results summary without having to re-parse log output.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Number of entries whose names violated an exFAT rule.
    pub found: usize,
    /// Number of entries actually renamed (only incremented in fix mode).
    pub fixed: usize,
    /// Number of entries skipped (symlinks, special files, non-UTF-8 names).
    pub skipped: usize,
    /// Number of entries that failed to process (rename/backup/copy errors).
    pub errors: usize,
}

/// Strips ANSI escape sequences (e.g. the `colored` crate's terminal
/// color codes) from `s`, returning a plain-text copy.
///
/// Used so the log file written by [`emit`] stays human-readable in
/// editors and greppable, even though the same string printed to the
/// terminal is colorized.
///
/// # Examples
///
/// ```
/// use exfatify::logger::strip_ansi;
///
/// let colored = "\x1b[31mERROR\x1b[0m: something broke";
/// assert_eq!(strip_ansi(colored), "ERROR: something broke");
/// ```
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
/// (ANSI-stripped) copy to the log file.
///
/// This is the only place the crate writes output — every status line
/// from [`crate::processor::process`] and the CLI binary's banner/summary
/// goes through here.
pub fn emit(msg: &str, log: &mut Option<fs::File>) {
    println!("{}", msg);
    if let Some(ref mut f) = log {
        let _ = writeln!(f, "{}", strip_ansi(msg));
    }
}

/// Opens (creating or truncating) the log file at `path`.
///
/// On Unix, the file is created with mode `0600` (owner read/write only),
/// since the log can contain full filesystem paths — information that
/// shouldn't be world-readable by default.
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
/// Non-Unix fallback: no special permission bits are set, since the
/// `std::os::unix` API used on Unix isn't available here.
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
