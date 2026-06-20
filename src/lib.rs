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

//! # exfatify
//!
//! A library (and CLI tool, see the `exfatify` binary) for detecting
//! and fixing filenames that are incompatible with the exFAT filesystem.
//!
//! exFAT — and by extension any drive that will ever touch a Windows
//! machine, since exFAT inherits most of its naming restrictions from
//! Windows — forbids certain characters, reserves certain device names,
//! rejects names ending in a space or period, and caps filenames at 255
//! UTF-16 code units. Copying a Linux/macOS folder full of files that
//! violate these rules onto an exFAT drive will fail outright, or worse,
//! get silently mangled depending on the copying tool.
//!
//! This crate splits cleanly into:
//!
//! - **Read-only inspection** ([`checker`]) — "is this name a problem?"
//! - **Pure transformation** ([`sanitizer`]) — "what should this name become?",
//!   plus case-insensitive collision detection (exFAT is case-insensitive
//!   but case-preserving, so `"Report.txt"` and `"report.txt"` are the
//!   *same file* on the target volume even though most source filesystems
//!   treat them as distinct).
//! - **Bulk traversal** ([`processor`]) — "apply the above to a whole directory tree."
//! - **Run bookkeeping** ([`logger`]) — stats + dual console/file output.
//! - **CLI-only concerns** ([`cli`]) — argument parsing, only used by the binary.
//! - **Shared rules** ([`constants`]) — the exFAT naming constraints themselves.
//!
//! ## Using this as a library
//!
//! The CLI binary in `src/main.rs` is a thin wrapper around this crate —
//! everything it does is also available to you directly. This is the
//! intended path for, e.g., a GUI front-end: let the user pick a folder,
//! call [`checker::needs_fix`] on each entry to build a live preview of
//! what's wrong, call [`sanitizer::sanitize`] to show what the fixed name
//! would look like, and only call [`processor::process`] once the user
//! confirms — exactly the scan → dry-run → fix flow the CLI exposes, but
//! driven by your own UI instead of command-line flags.
//!
//! ```
//! use exfat_sanitize::checker::needs_fix;
//! use exfat_sanitize::sanitizer::sanitize;
//!
//! let candidate = "report*.txt";
//! if needs_fix(candidate) {
//!     let fixed = sanitize(candidate, '-');
//!     assert_eq!(fixed, "report-.txt");
//! }
//! ```
//!
//! See the `tests/` directory for more complete, end-to-end examples of
//! the kind of workflow a GUI integrator would build (scan a temp
//! directory, preview, then fix).

pub mod checker;
pub mod cli;
pub mod constants;
pub mod logger;
pub mod processor;
pub mod sanitizer;
