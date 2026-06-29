use anyhow::{Context, Result};
use clap::Parser;
use dlt_grep::{grep_file, GrepOpts, COLOR_PATH, COLOR_RESET};
use rayon::prelude::*;
use regex::RegexBuilder;
use std::{
    io::{IsTerminal, Write},
    path::PathBuf,
};
use walkdir::WalkDir;

/// When to emit ANSI color codes.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default)]
enum ColorMode {
    /// Emit color when stdout is a terminal and NO_COLOR is unset.
    #[default]
    Auto,
    /// Always emit color.
    Always,
    /// Never emit color.
    Never,
}

/// grep-like search inside DLT (Diagnostic Log and Trace) files
#[derive(Parser, Debug)]
#[command(name = "dg", version, about)]
struct Args {
    /// Regex pattern to search for (matched against the formatted message line)
    pattern: String,

    /// DLT files or directories to search (default: recurse from current directory)
    #[arg(num_args = 0..)]
    files: Vec<PathBuf>,

    /// Case-insensitive matching
    #[arg(short = 'i', long)]
    ignore_case: bool,

    /// Print only a count of matching messages per file
    #[arg(short = 'c', long)]
    count: bool,

    /// Print the message index (0-based) before each match
    #[arg(short = 'n', long = "line-number")]
    line_number: bool,

    /// Invert match: print non-matching messages
    #[arg(short = 'v', long = "invert-match")]
    invert: bool,

    /// Input has no DLT storage header (e.g. live trace streams)
    #[arg(long)]
    no_storage_header: bool,

    /// Always print filename:line (suppress heading format even on a terminal)
    #[arg(long)]
    no_heading: bool,

    /// Control ANSI color output
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    color: ColorMode,
}

fn main() {
    match run() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(e) => {
            if let Some(io) = e.downcast_ref::<std::io::Error>() {
                if io.kind() == std::io::ErrorKind::BrokenPipe {
                    std::process::exit(0);
                }
            }
            eprintln!("dg: {e:#}");
            std::process::exit(2);
        }
    }
}

/// Expand the user-supplied list into concrete file paths.
///
/// - Empty list → walk `.` for `*.dlt`.
/// - A supplied path that is a file → use it as-is (any extension).
/// - A supplied path that is a directory → walk it recursively for `*.dlt`.
///
/// Results are sorted so output order is stable regardless of rayon scheduling.
fn collect_files(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let cwd;
    let roots: &[PathBuf] = if inputs.is_empty() {
        cwd = [PathBuf::from(".")];
        &cwd
    } else {
        inputs
    };

    let mut files: Vec<PathBuf> = Vec::new();

    for root in roots {
        if root.is_file() {
            files.push(root.clone());
        } else {
            for entry in WalkDir::new(root)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file()
                    && path
                        .extension()
                        .map(|ext| ext.eq_ignore_ascii_case("dlt"))
                        .unwrap_or(false)
                {
                    files.push(path.to_path_buf());
                }
            }
        }
    }

    files.sort_unstable();
    files
}

fn run() -> Result<i32> {
    let args = Args::parse();

    let pattern = RegexBuilder::new(&args.pattern)
        .case_insensitive(args.ignore_case)
        .build()
        .with_context(|| format!("invalid pattern: {:?}", args.pattern))?;

    let files = collect_files(&args.files);

    if files.is_empty() {
        return Ok(1);
    }

    // Show filenames whenever the user specified a directory or used the default
    // CWD walk — even if only one .dlt file is found.  This matches rg's
    // recursive behaviour where the filename is always part of the output.
    let is_recursive = args.files.is_empty() || args.files.iter().any(|f| f.is_dir());
    let show_filename = files.len() > 1 || is_recursive;

    let is_tty = std::io::stdout().is_terminal();

    // Resolve color mode.  Respect the de-facto NO_COLOR convention.
    let use_color = match args.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => is_tty && std::env::var_os("NO_COLOR").is_none(),
    };

    // Heading mode (rg default on TTY): filename on its own line, matches
    // below, blank line between files that have matches.
    // Disabled for --count (which always uses filename:N) and when piping.
    let use_heading = show_filename && !args.count && !args.no_heading && is_tty;

    let opts = GrepOpts {
        count: args.count,
        line_number: args.line_number,
        invert: args.invert,
        with_storage_header: !args.no_storage_header,
        highlight: use_color,
    };

    // Stream results to stdout as each file completes — rg-style.
    //
    // A worker thread drives rayon's parallel iterator with for_each_with,
    // which clones the Sender once per rayon thread.  When for_each_with
    // returns all clones are dropped, closing the channel and terminating
    // the for-loop below.  The main thread therefore sees each file's output
    // the moment that file finishes, without waiting for slower siblings.
    //
    // Output order is completion order (fastest file first), matching rg's
    // default behaviour.  Within each file all lines are contiguous because
    // grep_file writes into a private Vec<u8> before the send.
    let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, Vec<u8>, u64, Option<anyhow::Error>)>();

    let worker = {
        let pattern = pattern.clone(); // Regex is Arc-backed; clone is O(1)
        std::thread::spawn(move || {
            files.par_iter().for_each_with(tx, |tx, path| {
                let prefix = if show_filename && !use_heading {
                    if use_color {
                        format!("{COLOR_PATH}{}{COLOR_RESET}:", path.display())
                    } else {
                        format!("{}:", path.display())
                    }
                } else {
                    String::new()
                };
                let mut buf: Vec<u8> = Vec::new();
                let (n, err) = match grep_file(path, &pattern, &opts, &mut buf, &prefix) {
                    Ok(n)  => (n, None),
                    Err(e) => (0, Some(e)),
                };
                // Ignore SendError: receiver gone means broken pipe on the main
                // thread; the worker simply finishes its remaining files and exits.
                let _ = tx.send((path.clone(), buf, n, err));
            });
        })
    };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut total_matches: u64 = 0;
    let mut had_error = false;

    for (path, buf, n, err) in rx {
        if let Some(e) = err {
            if let Some(io) = e.downcast_ref::<std::io::Error>() {
                if io.kind() == std::io::ErrorKind::BrokenPipe {
                    return Ok(0);
                }
            }
            eprintln!("dg: {}: {e:#}", path.display());
            had_error = true;
            continue;
        }

        if use_heading {
            // Only emit the heading when the file actually produced output.
            if !buf.is_empty() {
                let write_res = if use_color {
                    write!(out, "{COLOR_PATH}{}{COLOR_RESET}\n", path.display())
                } else {
                    write!(out, "{}\n", path.display())
                };
                if let Err(e) = write_res {
                    if e.kind() == std::io::ErrorKind::BrokenPipe { return Ok(0); }
                    return Err(e.into());
                }
                if let Err(e) = out.write_all(&buf) {
                    if e.kind() == std::io::ErrorKind::BrokenPipe { return Ok(0); }
                    return Err(e.into());
                }
                if let Err(e) = out.write_all(b"\n") {
                    if e.kind() == std::io::ErrorKind::BrokenPipe { return Ok(0); }
                    return Err(e.into());
                }
            }
        } else if !buf.is_empty() {
            if let Err(e) = out.write_all(&buf) {
                if e.kind() == std::io::ErrorKind::BrokenPipe { return Ok(0); }
                return Err(e.into());
            }
        }

        total_matches += n;
    }

    // rx loop ends when the channel closes (worker dropped all senders).
    // Join to surface any panic from the worker thread.
    worker.join().expect("worker thread panicked");

    if had_error {
        return Ok(2);
    }

    Ok(if total_matches == 0 { 1 } else { 0 })
}
