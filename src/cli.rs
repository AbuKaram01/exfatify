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
//! Used directly only by `src/main.rs`. A GUI front-end embedding this
//! crate would normally talk to [`crate::checker`], [`crate::sanitizer`],
//! and [`crate::processor`] directly rather than constructing an [`Args`]
//! value — but [`Args::validate_replace_char`] is reusable validation
//! logic worth calling from a GUI too (e.g. as soon as the user types into
//! a "replacement character" field).

use std::path::PathBuf;

use clap::Parser;

/// Parsed command-line arguments.
///
/// Field-level doc comments below double as each flag's `--help` text
/// (via `clap`'s derive macro) and as `cargo doc` documentation.
#[derive(Parser, Debug)]
#[command(
    name = "exfatify",
    version = "1.0.0",
    about = "Sanitize filenames for exFAT compatibility",
    after_help = "\
MODES (default: --scan):
  --scan      Report problems only, change nothing  [DEFAULT]
  --dry-run   Show what would change, change nothing
  --fix       Actually rename files

ILLEGAL exFAT CHARACTERS:
  \\ : * ? \" < > |  and control chars U+0000 - U+001F
  Filenames longer than 255 UTF-16 code units
  Filenames starting with a space, or ending with a space or a period
  (a leading PERIOD is fine — dotfiles like .bashrc are untouched)
  Reserved names: CON PRN AUX NUL COM1-9 LPT1-9
  (reserved names are blocked with any extension, e.g. NUL.tar.gz)

EXAMPLES:
  # Safe first — see what would change
  exfatify --scan ~/Downloads
  exfatify --fix --dry-run ~/Documents

  # Apply fixes
  exfatify --fix ~/Pictures
  exfatify --fix --replace _ ~/Music
  exfatify --fix --backup --log /tmp/report.txt ~/Documents

NOTE: Checks each name's own length, not the full path's length. Some
Windows software still enforces a 260-char limit on the whole path —
see the README for details."
)]
pub struct Args {
    /// Root directory to scan (recursively).
    pub path: PathBuf,

    /// Report problems only; change nothing. This is the default mode.
    #[arg(short = 's', long, conflicts_with_all = ["fix", "dry_run"])]
    pub scan: bool,

    /// Actually rename files that violate exFAT naming rules.
    #[arg(short = 'f', long, conflicts_with = "scan")]
    pub fix: bool,

    /// Show what would be renamed under --fix, without changing anything.
    #[arg(short = 'n', long = "dry-run", conflicts_with = "scan")]
    pub dry_run: bool,

    /// Character used to replace each illegal character. Must not itself
    /// be illegal, a control character, a space, or a period — see
    /// [`Args::validate_replace_char`].
    #[arg(short = 'r', long, default_value = "-")]
    pub replace: char,

    /// Also print entries that are already exFAT-safe.
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Write a plain-text (ANSI-stripped) copy of the run's output to this file.
    #[arg(short = 'l', long, value_name = "FILE")]
    pub log: Option<PathBuf>,

    /// Before renaming a file (not a directory), copy it to `<name>.bak`.
    #[arg(short = 'b', long)]
    pub backup: bool,

    /// Skip symlinks entirely instead of renaming the link itself.
    #[arg(long = "no-symlinks")]
    pub no_symlinks: bool,
}

/// Why a candidate replacement character ([`Args::replace`]) can't be used.
///
/// Returned by [`Args::validate_replace_char`]. Implements [`std::fmt::Display`]
/// with the same human-readable message the CLI binary prints, so GUI code
/// can show it directly too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReplaceChar {
    /// The character is itself one of exFAT's illegal characters, or a
    /// control character — using it as a replacement would just
    /// reintroduce the same problem it's meant to fix.
    Illegal(char),
    /// The character is a space or a period, which would recreate the
    /// "trailing space/period" rule violation that's also illegal on exFAT.
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
    /// Returns `true` if this invocation must not modify the filesystem —
    /// i.e. `--scan` (explicit or implied by default) or `--dry-run`.
    ///
    /// # Examples
    ///
    /// ```
    /// use clap::Parser;
    /// use exfat_sanitize::cli::Args;
    ///
    /// let args = Args::parse_from(["exfatify", "--scan", "/tmp"]);
    /// assert!(args.is_readonly());
    ///
    /// let args = Args::parse_from(["exfatify", "--fix", "/tmp"]);
    /// assert!(!args.is_readonly());
    /// ```
    pub fn is_readonly(&self) -> bool {
        // Equivalent to `scan || dry_run || (!fix && !dry_run)`, simplified:
        // the `dry_run` disjunct already covers the `&& !dry_run` half of
        // the third term, so all that's left to OR in is `!fix`.
        self.scan || self.dry_run || !self.fix
    }

    /// Validates [`Self::replace`], returning an error describing why it's
    /// unusable if so.
    ///
    /// Extracted as its own method (rather than living inline in `main`)
    /// so a GUI can call it the moment the user picks a replacement
    /// character — e.g. to disable a "Fix" button and show an inline error
    /// — instead of only discovering the problem after the run starts.
    ///
    /// # Examples
    ///
    /// ```
    /// use clap::Parser;
    /// use exfat_sanitize::cli::{Args, InvalidReplaceChar};
    ///
    /// let args = Args::parse_from(["exfatify", "--replace", "*", "/tmp"]);
    /// assert_eq!(args.validate_replace_char(), Err(InvalidReplaceChar::Illegal('*')));
    ///
    /// let args = Args::parse_from(["exfatify", "--replace", "_", "/tmp"]);
    /// assert_eq!(args.validate_replace_char(), Ok(()));
    /// ```
    pub fn validate_replace_char(&self) -> Result<(), InvalidReplaceChar> {
        use crate::constants::ILLEGAL_CHARS;

        if ILLEGAL_CHARS.contains(&self.replace) || (self.replace as u32) <= 0x1F {
            return Err(InvalidReplaceChar::Illegal(self.replace));
        }
        if self.replace == '.' || self.replace == ' ' {
            return Err(InvalidReplaceChar::ProducesTrailingIssue(self.replace));
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
    fn invalid_replace_char_display_message_mentions_the_char() {
        let err = InvalidReplaceChar::Illegal('*');
        assert!(err.to_string().contains('*'));
    }
}
