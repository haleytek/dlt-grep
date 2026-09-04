# dg — grep for DLT files

`dg` is a fast, `rg`-like search tool for [DLT (Diagnostic Log and Trace)](https://www.autosar.org/fileadmin/standards/foundation/1-0/AUTOSAR_PRS_DiagnosticLogAndTraceProtocol.pdf) binary log files, the standard logging format in automotive ECUs and embedded Linux systems.

Instead of the traditional `dlt-convert -a file.dlt | grep pattern` pipeline — which decodes the entire file to ASCII before searching a single byte — `dg` parses the binary format directly, applies the regex on each structured message, and prints timestamp-sorted results within each file. On a single 815 MB trace it is about **50× faster** than `dlt-convert | grep`.

## Use case

When a new bug comes in, the first question is usually: *have we seen this before?* Searching your own local log archive with `dlt-convert | grep` means waiting a minute or more per large file, making cross-file pattern searches impractical.

`dg` makes it fast enough to be a normal part of the debugging workflow: point it at a directory of past bug reports and search for the error pattern, component name, or message sequence that characterises the new case. Results are grouped by file, sorted within each file by DLT monotonic timestamp by default, with ISO-8601 storage timestamps printed for wall-clock context and storage timestamp sorting available when wall-clock ordering is the better fit.

For use cases related to debugging DLT (and other) log files contained within a singe bug report, [logcrab](https://github.com/daniel-freiermuth/logcrab) is very much recommended instead.

## Features

- **Recursive directory search** — pass a directory or nothing (defaults to `.`) and `dg` finds `*.dlt`, `*.dlt.gz`, and `*.dlt.zst` files recursively, just like `rg`
- **Parallel processing** — files are searched concurrently via rayon; matching messages are sorted within each file before printing
- **`rg`-like output** — heading format on TTY (filename header, matches below, blank separator), `filename:line` when piped; ANSI color with match highlighting
- **Full regex support** — powered by Rust's `regex` crate; case-insensitive (`-i`), inverted match (`-v`), anchors, alternation, etc.
- **Transparent decompression** — gzip- and zstd-compressed DLT files are streamed directly without an unpacking step
- **Exit codes** — `0` matches found, `1` no matches, `2` error; compatible with shell pipelines

## Usage

```
dg [OPTIONS] <PATTERN> [FILES...]
```

```
# Search current directory recursively
dg 'error'

# Search a specific directory
dg '.*fault' /logs/bugreport/

# Search a single file, case-insensitive
dg -i 'warning' ECU1.dlt

# Count matches per file across a directory tree
dg -c 'assert' /logs/

# Show message index (-n), case-insensitive, force color in a pipe
dg -ni 'timeout' /logs/ --color=always | less -R
```

### Options

| Flag | Description |
|---|---|
| `-i`, `--ignore-case` | Case-insensitive matching |
| `-c`, `--count` | Print match count per file instead of matches |
| `-n`, `--line-number` | Prefix each match with its 0-based message index |
| `-v`, `--invert-match` | Print messages that do NOT match |
| `--no-storage-header` | Input has no DLT storage header (raw/live streams) |
| `--no-heading` | Force `filename:line` format even on a terminal |
| `--color <auto\|always\|never>` | ANSI color control (default: `auto`) |
| `--sort <none\|monotonic\|storage>` | Keep file order, sort by DLT monotonic timestamp (default), or sort by storage timestamp |

## Installation

Pre-built binaries are attached to every [release](https://github.com/haleytek/dlt-grep/releases/latest).

**Linux (x86-64, Ubuntu/Debian/Fedora/Arch/glibc)**

```bash
mkdir -p ~/.local/bin
curl -fL https://github.com/haleytek/dlt-grep/releases/latest/download/dg-linux-x86_64-gnu \
  -o ~/.local/bin/dg
chmod +x ~/.local/bin/dg
```

Use `dg-linux-x86_64-musl` only when you need a fully static portable Linux binary.


**macOS (Apple silicon and Intel)**

```bash
mkdir -p ~/.local/bin
case "$(uname -m)" in
  arm64) asset=dg-macos-aarch64 ;;
  x86_64) asset=dg-macos-x86_64 ;;
  *) echo "unsupported macOS architecture: $(uname -m)" >&2; exit 1 ;;
esac
curl -fL "https://github.com/haleytek/dlt-grep/releases/latest/download/${asset}" \
  -o ~/.local/bin/dg
chmod +x ~/.local/bin/dg
```

Make sure `~/.local/bin` is on your `PATH`, or install the binary anywhere that already is (e.g. `/usr/local/bin`).

## Build from source

Requires Rust 1.85+ (`edition = "2024"`). No system dependencies.

```bash
cargo build --release
# binary at target/release/dg
```

## Benchmarks

**Setup:** AMD Ryzen 9 7950X (32 threads), NVMe SSD.
Two target sets:

- **Single file** — ECU1 trace, 815 MB, 2.3 M messages
- **Dir** — 10 DLT files, 27 MB total, 188 K messages

Competitors:

- `dlt-convert -a <files> | grep -c` — the standard approach, sequential
- `xargs -P32 dlt-convert | grep -c` — parallel dlt-convert, one process per file

### Single file — 815 MB (3 warm runs, 1 cold run)

| Pattern | `dg` warm | `dg` cold | `dlt-convert\|grep` warm | speedup |
|---|---:|---:|---:|---:|
| `.` (match all) | 1.50 s | 1.69 s | 93.3 s | **62×** |
| `error` | 1.59 s | 1.72 s | 93.8 s | **59×** |
| `-i error` | 1.69 s | 1.77 s | 94.9 s | **56×** |
| `^…HPA` (anchored prefix) | 1.61 s | 1.73 s | 92.5 s | **57×** |
| no match | 1.56 s | 1.66 s | 92.5 s | **59×** |

### Dir — 10 files, 27 MB (5 warm runs, 3 cold runs)

| Pattern | `dg` warm | `dg` cold | `conv(seq)` warm | `conv(par,32)` warm | `conv(par,32)` cold |
|---|---:|---:|---:|---:|---:|
| `.` (match all) | **56 ms** | 287 ms | 475 ms | 168 ms | 191 ms |
| `error` | **55 ms** | 308 ms | 476 ms | 167 ms | 189 ms |
| no match | **55 ms** | 320 ms | 480 ms | 167 ms | 194 ms |

### Key takeaways

**56–62× faster than `dlt-convert | grep` (warm cache, single file).** The entire gap is the text-conversion pipeline: `dlt-convert` serialises 815 MB of binary DLT to ASCII before grep can scan a single byte. `dg` parses binary directly and regex-matches the structured output in one pass — no intermediate representation, no pipe.

**Pattern complexity is irrelevant.** Match-all, case-insensitive, anchored, no-match — all land within 5% of each other for both tools. The bottleneck is I/O + parse cost, not regex evaluation. A tighter pattern saves no measurable time.

**Cold cache costs `dg` ~200 ms; costs `dlt-convert` nothing.** `dg` is I/O-bound on a warm file — it reads 815 MB and exhausts it. `dlt-convert` is CPU-bound on ASCII conversion and never comes close to saturating the NVMe, so warm vs cold makes no difference for it.

**`dg` wins the directory set 8.5× over sequential, 3× over 32-way parallel dlt-convert (warm).** One in-process rayon thread pool, no fork/exec, no inter-process pipes.

**One exception: cold cache + many small files, parallel dlt-convert wins by ~60%.** 10 independent processes give the kernel 10 separate readahead streams simultaneously. This effect vanishes at scale — any file large enough to be CPU-bound on conversion flips the result back in `dg`'s favour.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for the release history.

## License

Copyright 2026 HaleyTek AB.

Licensed under the [Apache License 2.0](LICENSE).
