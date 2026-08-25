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

//! Command-line argument definitions for the `exfatify` binary.
//!
//! Used directly only by `src/main.rs`.

use std::path::PathBuf;

use clap::Parser;

/// Parsed command-line arguments for the `exfatify` CLI.
#[derive(Parser, Debug)]
#[command(
    name = "exfatify",
    version,
    about = "Sanitize filenames for exFAT compatibility"
)]
pub struct Args {
    /// Root directory to scan (recursively).
    pub path: PathBuf,

    /// Report problems only; change nothing. Default mode.
    #[arg(
        short = 's',
        long,
        conflicts_with_all = ["fix", "dry_run"],
        long_help = "Report problems only; change nothing. Default mode. \
                     Cannot be combined with --fix or --dry-run."
    )]
    pub scan: bool,

    /// Rename files that violate exFAT naming rules.
    #[arg(
        short = 'f',
        long,
        conflicts_with = "scan",
        long_help = "Rename files that violate exFAT naming rules. \
                     Pair with --backup for a safety net."
    )]
    pub fix: bool,

    /// Show what --fix would do, without changing anything.
    #[arg(
        short = 'n',
        long = "dry-run",
        conflicts_with = "scan",
        long_help = "Show what --fix would do, without changing anything. \
                     Reports the exact same renames a subsequent --fix run will make."
    )]
    pub dry_run: bool,

    /// Character used to replace each illegal character.
    #[arg(
        short = 'r',
        long,
        default_value = "-",
        long_help = "Character used to replace each illegal character. \
                     Must not itself be illegal, a control character, a space, a period, or '/'."
    )]
    pub replace: char,

    /// Also print entries that are already exFAT-safe.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Write a plain-text copy of the run's output to this file.
    #[arg(
        short = 'l',
        long,
        value_name = "FILE",
        long_help = "Write a plain-text copy of the run's output to this file. \
                     Created with permissions 0600 on Unix, since it can contain full paths."
    )]
    pub log: Option<PathBuf>,

    /// Before renaming a file, copy it to `<n>.bak`.
    #[arg(
        short = 'b',
        long,
        long_help = "Before renaming a file, copy it to `<n>.bak`. \
                     Applies to regular files only — directories and symlinks are never backed up."
    )]
    pub backup: bool,

    /// Skip symlinks entirely instead of renaming the link itself.
    #[arg(
        long = "no-symlinks",
        long_help = "Skip symlinks entirely instead of renaming the link itself. \
                     By default, symlinks are renamed like any other entry, without following them."
    )]
    pub no_symlinks: bool,
}

/// Why a candidate replacement character ([`Args::replace`]) can't be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReplaceChar {
    /// Itself an exFAT-illegal or control character.
    Illegal(char),
    /// A space or period — would recreate the trailing-space/period rule
    /// violation.
    ProducesTrailingIssue(char),
}

impl std::fmt::Display for InvalidReplaceChar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidReplaceChar::Illegal(c) => write!(
                f,
                "'{c}' is itself illegal in exFAT — choose a different replacement char"
            ),
            InvalidReplaceChar::ProducesTrailingIssue(c) => write!(
                f,
                "'{c}' as replacement char can produce filenames ending in space/dot \
                 (forbidden on exFAT) — choose a different character"
            ),
        }
    }
}

impl Args {
    /// `true` if this invocation must not modify the filesystem: `--scan`
    /// (explicit or default) or `--dry-run`.
    pub fn is_readonly(&self) -> bool {
        // Equivalent to `scan || dry_run || (!fix && !dry_run)`, simplified
        // to `scan || dry_run || !fix`.
        self.scan || self.dry_run || !self.fix
    }

    /// Validates [`Self::replace`], returning why it's unusable if so.
    pub fn validate_replace_char(&self) -> Result<(), InvalidReplaceChar> {
        use crate::constants::ILLEGAL_CHARS;

        if ILLEGAL_CHARS.contains(&self.replace) || (self.replace as u32) <= 0x1F {
            return Err(InvalidReplaceChar::Illegal(self.replace));
        }
        if self.replace == '.' || self.replace == ' ' {
            return Err(InvalidReplaceChar::ProducesTrailingIssue(self.replace));
        }
        if self.replace == '/' || self.replace == '\0' {
            return Err(InvalidReplaceChar::Illegal(self.replace));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(extra_args: &[&str]) -> Args {
        let mut full = vec!["exfatify"];
        full.extend_from_slice(extra_args);
        Args::parse_from(full)
    }

    #[test]
    fn default_mode_is_readonly() {
        let args = parse(&["/tmp"]);
        assert!(args.is_readonly());
        assert!(!args.fix);
        assert!(!args.scan);
    }

    #[test]
    fn explicit_scan_is_readonly() {
        assert!(parse(&["--scan", "/tmp"]).is_readonly());
    }

    #[test]
    fn dry_run_is_readonly_even_combined_with_fix() {
        assert!(parse(&["--fix", "--dry-run", "/tmp"]).is_readonly());
    }

    #[test]
    fn fix_alone_is_not_readonly() {
        assert!(!parse(&["--fix", "/tmp"]).is_readonly());
    }

    #[test]
    fn default_replace_char_is_hyphen() {
        assert_eq!(parse(&["/tmp"]).replace, '-');
    }

    #[test]
    fn scan_and_fix_are_mutually_exclusive() {
        let result = Args::try_parse_from(["exfatify", "--scan", "--fix", "/tmp"]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_replace_char_accepts_a_safe_default() {
        assert_eq!(parse(&["/tmp"]).validate_replace_char(), Ok(()));
    }

    #[test]
    fn validate_replace_char_rejects_illegal_chars() {
        for &c in crate::constants::ILLEGAL_CHARS {
            let args = parse(&["--replace", &c.to_string(), "/tmp"]);
            assert_eq!(
                args.validate_replace_char(),
                Err(InvalidReplaceChar::Illegal(c))
            );
        }
    }

    #[test]
    fn validate_replace_char_rejects_space_and_period() {
        let args = parse(&["--replace", " ", "/tmp"]);
        assert_eq!(
            args.validate_replace_char(),
            Err(InvalidReplaceChar::ProducesTrailingIssue(' '))
        );

        let args = parse(&["--replace", ".", "/tmp"]);
        assert_eq!(
            args.validate_replace_char(),
            Err(InvalidReplaceChar::ProducesTrailingIssue('.'))
        );
    }

    #[test]
    fn validate_replace_char_rejects_slash() {
        let args = parse(&["--replace", "/", "/tmp"]);
        assert_eq!(
            args.validate_replace_char(),
            Err(InvalidReplaceChar::Illegal('/'))
        );
    }

    #[test]
    fn invalid_replace_char_display_message_mentions_the_char() {
        let err = InvalidReplaceChar::Illegal('*');
        assert!(err.to_string().contains('*'));
    }
}
