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

//! Binary entry point for the `exfatify` command-line tool.
//!
//! All reusable logic lives in the `exfat_sanitize` library crate (see
//! `src/lib.rs` and its modules). This file is only responsible for:
//! parsing CLI args, validating them, printing the run summary, and
//! wiring everything together — nothing here is reachable from outside
//! the binary, which is exactly why it stays untested directly. Every
//! function it calls is unit-tested in its own module instead.

use clap::Parser;
use colored::*;

use exfat_sanitize::cli::Args;
use exfat_sanitize::logger::{emit, open_log_file, Stats};
use exfat_sanitize::processor::process;

fn main() {
    let args = Args::parse();

    // ── Validate path ────────────────────────────────────────────────────
    if !args.path.exists() {
        eprintln!(
            "{} path does not exist: {}",
            "[ERROR]".red().bold(),
            args.path.display()
        );
        std::process::exit(1);
    }

    // ── Validate replacement character ──────────────────────────────────
    if let Err(e) = args.validate_replace_char() {
        eprintln!("{} {}", "[ERROR]".red().bold(), e);
        std::process::exit(1);
    }

    // ── Open log file (if requested) ────────────────────────────────────
    let mut log: Option<std::fs::File> = if let Some(ref log_path) = args.log {
        match open_log_file(log_path) {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("{} cannot create log file: {}", "[ERROR]".red().bold(), e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    // ── Run summary ──────────────────────────────────────────────────────
    let mode_label = if args.scan {
        "Scan Only".yellow().to_string()
    } else if args.dry_run {
        "Dry Run".blue().to_string()
    } else if args.fix {
        "Fix".green().to_string()
    } else {
        format!(
            "{} {}",
            "Scan Only".yellow(),
            "(default — use --fix to apply changes)".dimmed()
        )
    };

    emit(
        &format!("  {:<16} {}", "Path:".bold(), args.path.display()),
        &mut log,
    );
    emit(
        &format!("  {:<16} {}", "Mode:".bold(), mode_label),
        &mut log,
    );
    emit(
        &format!("  {:<16} '{}'", "Replace:".bold(), args.replace),
        &mut log,
    );
    emit(
        &format!("  {:<16} {}", "Skip symlinks:".bold(), args.no_symlinks),
        &mut log,
    );
    emit(
        &format!("  {:<16} {}", "Backup files:".bold(), args.backup),
        &mut log,
    );
    if let Some(ref lp) = args.log {
        emit(
            &format!("  {:<16} {} (mode 0600)", "Log:".bold(), lp.display()),
            &mut log,
        );
    }
    emit("", &mut log);
    emit(
        &format!(
            "  {}: \\ : * ? \" < > |  +  ctrl U+0000-U+001F",
            "Illegal chars".bold()
        ),
        &mut log,
    );
    emit(
        &format!(
            "  {}: leading space, trailing space, or trailing period",
            "Also illegal".bold()
        ),
        &mut log,
    );
    emit(
        &format!("  {}: 255 UTF-16 code units", "Max name len".bold()),
        &mut log,
    );
    emit(
        &format!(
            "  {}: CON PRN AUX NUL COM1-9 LPT1-9 (any extension)",
            "Reserved".bold()
        ),
        &mut log,
    );
    emit("\n──────────────────────────────────────────", &mut log);

    // ── Process ──────────────────────────────────────────────────────────
    let mut stats = Stats::default();
    process(&args, &mut stats, &mut log);

    // ── Summary ──────────────────────────────────────────────────────────
    emit("\n──────────────────────────────────────────", &mut log);
    emit(
        &format!("  {:<16} {}", "Problems found:", stats.found),
        &mut log,
    );
    if args.fix && !args.dry_run {
        emit(
            &format!("  {:<16} {}", "Fixed:", stats.fixed.to_string().green()),
            &mut log,
        );
    }
    if stats.skipped > 0 {
        emit(&format!("  {:<16} {}", "Skipped:", stats.skipped), &mut log);
    }
    if stats.errors > 0 {
        emit(
            &format!("  {:<16} {}", "Errors:", stats.errors.to_string().red()),
            &mut log,
        );
    }
    emit("──────────────────────────────────────────", &mut log);

    let closing = if stats.found == 0 {
        format!(
            "  {} All filenames are exFAT-compatible!\n",
            "✔".green().bold()
        )
    } else if args.is_readonly() {
        format!("  {} Run with --fix to apply changes.\n", "⚠".yellow())
    } else {
        format!("  {} Done.\n", "✔".green().bold())
    };
    emit(&closing, &mut log);

    if stats.errors > 0 {
        std::process::exit(1);
    }
}
