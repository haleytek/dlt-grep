# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-09-04

### Added

- Print both the DLT monotonic timestamp and the storage timestamp for matching messages.
- Render storage timestamps as UTC ISO-8601 datetimes.
- Add `--sort <none|monotonic|storage>` to control per-file match ordering.
- Publish a fast glibc Linux release binary as `dg-linux-x86_64-gnu`.
- Keep a static musl Linux release binary as `dg-linux-x86_64-musl`.

### Changed

- Sort matches within each file by DLT monotonic timestamp by default.
- Group multi-file output by file again, using rg-style headings on terminals.
- Make the glibc Linux binary the recommended Linux download.

## [0.1.0] - 2026-08-29

### Added

- Initial grep-like DLT search CLI.
- Recursive directory search for `*.dlt` files.
- Parallel file processing via rayon.
- Regex matching, case-insensitive matching, inverted matching, and match counts.
- rg-like output with filename headings, line numbers, and ANSI color control.
- Release binaries for Linux and macOS.
