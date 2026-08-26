# exfatify

A CLI tool that finds and fixes filenames that break on exFAT or Windows. It never renames anything unless you explicitly run it with `--fix`.

## Why exfatify?

exFAT's naming rules are mostly inherited from Windows, not from exFAT's own underlying constraints — which is exactly why they're easy to violate without noticing on Linux or macOS. Both allow characters like `:`, `*`, and `?` in filenames, and both are case-sensitive, while exFAT is case-insensitive but case-preserving: `Report.txt` and `report.txt` are the same file on an exFAT volume, even though they're two different files everywhere else. None of this shows up until you've already copied a few thousand files to a drive and something fails halfway through, or two files silently overwrite each other.

## Why Not an Existing Tool?

Most generic filename sanitizers stop at illegal characters — they don't check for exFAT's case-insensitive collisions, since that's not something Linux or macOS filesystems ever have to deal with. A script that blindly replaces `:` and `*` can still hand you two files that merge into one the moment they land on an exFAT drive. exfatify treats that as a first-class problem: every rename is checked against the rest of the folder, case-insensitively, before it happens — nothing gets silently overwritten. It's also a single native binary with no scripting runtime or external dependency to install.

## What It Does and Doesn't Do

Fixes:
- Illegal characters (`\ / : * ? " < > |`) and control characters
- Reserved Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`), regardless of extension
- Leading/trailing spaces and trailing periods
- Names over 255 UTF-16 code units
- Case-insensitive collisions between sibling files — the losing name gets a numeric suffix instead of overwriting the other

Leaves untouched:
- File contents
- Permissions, timestamps, ownership
- Symlink targets — only the link itself is renamed, never followed (skip entirely with `--no-symlinks`)
- Directory structure — files are renamed in place, never moved
- Unicode normalization differences

## Installation and Building

Debian/Ubuntu:
```bash
cargo install cargo-deb
cargo deb
sudo dpkg -i target/debian/exfatify_*.deb
```

Fedora/RHEL:
```bash
cargo install cargo-generate-rpm
cargo build --release
cargo generate-rpm
sudo rpm -i target/generate-rpm/exfatify-*.rpm
```

From source:
```bash
git clone https://github.com/AbuKaram01/exfatify.git
cd exfatify
cargo install --path .
```

## Usage

```
exfatify [OPTIONS] <PATH>
```

```bash
exfatify ~/Downloads                          # scan (default) — nothing changes
exfatify --fix --dry-run ~/Downloads          # preview the exact renames
exfatify --fix --backup ~/Downloads           # apply them, with a safety net
exfatify --fix --replace _ ~/Music            # use a different replacement char
exfatify --fix --backup --log run.txt ~/Docs  # keep a record of what happened
```

Flags:
- `-s, --scan` — report problems only, change nothing (default)
- `-f, --fix` — rename files
- `-n, --dry-run` — show what `--fix` would do, without changing anything
- `-r, --replace <CHAR>` — character used to replace illegal characters (default `-`)
- `-b, --backup` — copy each file to `<name>.bak` before renaming it
- `-l, --log <FILE>` — write a plain-text copy of the run to a file
- `-v, --verbose` — also print entries that are already safe
- `--no-symlinks` — skip symlinks entirely instead of renaming the link itself
- `-h, --help` / `-V, --version`

## License

GPL-3.0-or-later © AbuKaram01 — see [LICENSE](LICENSE).
