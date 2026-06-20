# exfatify

**Find and fix filenames that break on exFAT — before they break on exFAT.**

exFAT is the de facto standard for SD cards, USB drives, and external SSDs because it's readable by Windows, macOS, and Linux alike. The catch: it inherits Windows' naming restrictions, and most other filesystems don't enforce them. Build up a folder on Linux or macOS for a while and you *will* eventually have a filename that exFAT simply refuses to write — or worse, silently mangles.

`exfatify` finds every filename in a directory tree that exFAT would choke on, shows you exactly what it would change, and — only when you ask it to — fixes it.

```
$ exfatify --scan ~/Pictures

  Path:            /home/you/Pictures
  Mode:            Scan Only (default — use --fix to apply changes)
  Replace:         '-'
  Skip symlinks:   false
  Backup files:    false

  Illegal chars: \ : * ? " < > |  +  ctrl U+0000-U+001F
  Also illegal: leading space, trailing space, or trailing period
  Max name len: 255 UTF-16 code units
  Reserved: CON PRN AUX NUL COM1-9 LPT1-9 (any extension)

──────────────────────────────────────────
  [PROBLEM] /home/you/Pictures/Trip: Day 1?.jpg
            -> would become: Trip- Day 1-.jpg
  [PROBLEM] /home/you/Pictures/notes.txt 
            -> would become: notes.txt
  [PROBLEM] /home/you/Pictures/Screenshot.PNG
            -> would become: Screenshot-1.PNG

──────────────────────────────────────────
  Problems found:  3
──────────────────────────────────────────
  ⚠ Run with --fix to apply changes.
```

---

## Table of contents

- [Why this exists](#why-this-exists)
- [What it catches](#what-it-catches)
- [Installation](#installation)
- [Usage](#usage)
- [CLI reference](#cli-reference)
- [Modes](#modes)
- [Using it as a library](#using-it-as-a-library)
- [Testing](#testing)
- [Known limitations](#known-limitations)
- [Contributing](#contributing)
- [License](#license)

---

## Why this exists

exFAT's naming rules are mostly inherited from Windows, not from exFAT's own underlying constraints — which is exactly why they're easy to violate without noticing on a Linux or macOS machine:

- Linux and macOS happily allow `:`, `*`, `?`, and friends in filenames. exFAT doesn't.
- Linux and macOS are case-*sensitive*. exFAT is case-*insensitive* but case-*preserving* — `Report.txt` and `report.txt` are the **same file** on an exFAT volume, even though they're two different files everywhere else.
- A file named `NUL.tar.gz` is completely normal on Linux. Try opening it on Windows.
- A filename ending in a space or a period gets silently stripped by Windows — or rejected outright, depending on the tool.
- exFAT's 255-character limit is measured in UTF-16 *code units*, not bytes and not characters — so a filename full of emoji can hit the limit at half the character count you'd expect.

None of this shows up until you've already copied a few thousand files to a drive and something fails halfway through, or two files silently overwrote each other. `exfatify` catches all of it up front.

## What it catches

| Problem | Example | Why it matters |
|---|---|---|
| Illegal characters | `Report: Q3?.pdf` | `\ : * ? " < > \|` and control characters (`U+0000`–`U+001F`) are forbidden outright |
| Reserved device names | `NUL.txt`, `con.tar.gz` | `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9` are blocked regardless of extension |
| Leading or trailing space, trailing period | ` notes.txt`, `notes `, `archive.` | The FAT/exFAT long-name spec itself says these are ignored on write — confirmed against Microsoft's exFAT docs and direct `CopyFile` API testing. (A *leading* period is explicitly fine — `.bashrc`-style dotfiles are untouched.) |
| Names over 255 UTF-16 units | a very long filename | Measured the same way exFAT and the Win32 API measure it — not bytes, not `char`s |
| Case-insensitive collisions | `Vacation.JPG` + `vacation.jpg` | Two distinct files on your source drive become **one file** on exFAT, silently |

Every fix is **collision-safe**: if the cleaned-up name would collide with something else in the same folder (including case-insensitively), a numeric suffix gets appended instead of overwriting anything.

## Installation

### From source

```bash
git clone https://github.com/AbuKaram01/exfatify.git
cd exfatify
cargo install --path .
```

### Debian/Ubuntu (`.deb`)

```bash
cargo install cargo-deb
cargo deb
sudo dpkg -i target/debian/exfatify_*.deb
```

### Fedora/RHEL (`.rpm`)

```bash
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm
sudo rpm -i target/generate-rpm/exfatify-*.rpm
```

### As a Rust dependency

```toml
[dependencies]
exfatify = "1.0.1"
```

See [Using it as a library](#using-it-as-a-library).

## Usage

**Always start with `--scan`** (the default — you don't even need to pass it) to see what would change without touching anything:

```bash
exfatify ~/Downloads
```

Then preview the exact renames `--fix` would perform, still without changing anything:

```bash
exfatify --fix --dry-run ~/Downloads
```

When you're happy with the plan, apply it for real:

```bash
exfatify --fix ~/Downloads
```

Keep a safety net by backing up every file before it's renamed:

```bash
exfatify --fix --backup ~/Downloads
```

Use a different replacement character (default is `-`):

```bash
exfatify --fix --replace _ ~/Music
```

Keep a record of everything that happened:

```bash
exfatify --fix --backup --log ~/exfat-report.txt ~/Documents
```

## CLI reference

```
exfatify [OPTIONS] <PATH>
```

| Flag | Short | Description |
|---|---|---|
| `--scan` | `-s` | Report problems only, change nothing. **Default mode.** |
| `--fix` | `-f` | Actually rename files. |
| `--dry-run` | `-n` | Show what `--fix` would do, without changing anything. |
| `--replace <CHAR>` | `-r` | Character used to replace illegal characters. Default: `-` |
| `--backup` | `-b` | Copy each file to `<name>.bak` before renaming it. |
| `--log <FILE>` | `-l` | Write a plain-text copy of the run to a file (mode `0600`). |
| `--verbose` | `-v` | Also print entries that are already exFAT-safe. |
| `--no-symlinks` | | Skip symlinks entirely instead of renaming the link itself. |
| `--help` | `-h` | Show full help, including detailed rule reference. |
| `--version` | `-V` | Print the version. |

## Modes

| | Reads the filesystem | Renames anything | Use it for |
|---|:---:|:---:|---|
| **Scan** (default) | ✅ | ❌ | "What's wrong, exactly?" |
| **Dry-run** (`--fix --dry-run`) | ✅ | ❌ | "What would `--fix` actually do?" |
| **Fix** (`--fix`) | ✅ | ✅ | Apply the plan |

Scan and dry-run always report the **exact same set of changes** that a subsequent fix run will make — including which side of a case-insensitive collision gets renamed — so there are no surprises between preview and execution.

Directories are processed contents-first: every file and subdirectory gets renamed before its parent, so a renamed parent directory never orphans paths still queued underneath it.

## Using it as a library

The CLI binary is a thin wrapper around the `exfatify` library — everything it does is available directly, which makes it straightforward to build a GUI, a batch-processing pipeline, or your own tooling on top of it instead of shelling out to the binary.

```rust
use exfatify::checker::needs_fix;
use exfatify::sanitizer::sanitize;

let candidate = "Report: Q3?.pdf";

if needs_fix(candidate) {
    let fixed = sanitize(candidate, '-');
    println!("{candidate} -> {fixed}"); // Report- Q3-.pdf
}
```

Walking and fixing a whole directory tree, the same way the CLI does:

```rust
use exfatify::cli::Args;
use exfatify::logger::Stats;
use exfatify::processor::process;
use std::path::PathBuf;

let args = Args {
    path: PathBuf::from("/path/to/folder"),
    scan: false,
    fix: true,
    dry_run: false,
    replace: '-',
    verbose: false,
    log: None,
    backup: true,
    no_symlinks: false,
};

let mut stats = Stats::default();
process(&args, &mut stats, &mut None);

println!("fixed {} of {} problems", stats.fixed, stats.found);
```

Full API documentation, including every public function's edge cases:

```bash
cargo doc --open
```

## Testing

```bash
cargo test     # 61 unit tests + 9 integration tests + 12 doctests
cargo clippy   # zero warnings
```

The test suite covers the exFAT rule set itself (illegal characters, reserved names, trailing characters, UTF-16 length), collision-avoidance (including the case-insensitive collisions exFAT introduces that most tools miss), backup behavior, symlink handling, and directory-traversal ordering — including dangling symlinks and pathological filenames (e.g. very long dotfiles) that broke naive implementations during development.

## Known limitations

- **Per-name only, not full-path length.** `exfatify` checks each filename and folder name's own length (255 UTF-16 code units, exFAT's actual limit), but it doesn't track the *cumulative* path length. Some Windows software still enforces the legacy `MAX_PATH` limit (260 characters for the full `drive:\folder\...\file.ext` string) — a deeply nested folder structure can hit that even when every individual name is perfectly valid. This isn't an exFAT filesystem limit (exFAT itself has no such restriction), and modern Windows can disable it entirely via `LongPathsEnabled`, so whether it actually affects you depends on your OS configuration and the specific tools you use to access the drive. If you hit this, the fix is restructuring folders to be shallower — not something this tool attempts automatically, since "fixing" it would mean renaming directories you may not want touched.
- **Unicode normalization isn't checked.** Two filenames that *look* identical but use different Unicode normalization forms (e.g., a precomposed `é` vs. an `e` + combining accent) are treated as different names, since they're different byte sequences — exFAT itself doesn't normalize either, so this matches real on-disk behavior, but it can be a source of confusing-looking "duplicates" that the case-insensitive collision check won't catch.

## Contributing

Issues and pull requests are welcome. A few things that'll make a PR easier to review:

- Run `cargo test` and `cargo clippy` before opening the PR — both should be clean.
- New behavior should come with a test. If you're fixing a bug, a regression test that fails before your fix and passes after it is ideal.
- Keep the module boundaries: `checker` stays read-only, `sanitizer` stays pure (aside from the documented filesystem reads), filesystem writes stay in `processor`.

## License

[AGPL-3.0-or-later](LICENSE) © AbuKaram01

This is a stricter copyleft than the plain GPL: if you modify `exfatify` and run it as part of a network service (for example, a web-based version of this tool), the AGPL requires you to make your modified source available to that service's users — not just to people you hand a binary to.
