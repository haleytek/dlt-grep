use anyhow::{Context, Result};
use clap::Parser;
use dlt_grep::{grep_file, GrepOpts};
use regex::RegexBuilder;
use std::path::PathBuf;

/// grep-like search inside DLT (Diagnostic Log and Trace) files
#[derive(Parser, Debug)]
#[command(name = "dg", version, about)]
struct Args {
    /// Regex pattern to search for (matched against the formatted message line)
    pattern: String,

    /// DLT files to search
    #[arg(required = true)]
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

fn run() -> Result<i32> {
    let args = Args::parse();

    let pattern = RegexBuilder::new(&args.pattern)
        .case_insensitive(args.ignore_case)
        .build()
        .with_context(|| format!("invalid pattern: {:?}", args.pattern))?;

    let opts = GrepOpts {
        count: args.count,
        line_number: args.line_number,
        invert: args.invert,
        with_storage_header: !args.no_storage_header,
    };

    let multi_file = args.files.len() > 1;
    let mut total_matches: u64 = 0;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for path in &args.files {
        let prefix = if multi_file {
            format!("{}:", path.display())
        } else {
            String::new()
        };

        match grep_file(path, &pattern, &opts, &mut out, &prefix) {
            Ok(n) => total_matches += n,
            Err(e) => {
                if let Some(io) = e.downcast_ref::<std::io::Error>() {
                    if io.kind() == std::io::ErrorKind::BrokenPipe {
                        return Ok(0);
                    }
                }
                return Err(e);
            }
        }
    }

    Ok(if total_matches == 0 { 1 } else { 0 })
}
