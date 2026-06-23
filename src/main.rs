use anyhow::{Context, Result};
use clap::Parser;
use dlt_core::{
    dlt::{Message, MessageType, PayloadContent, Value},
    parse::{DltParseError, ParsedMessage},
    read::{read_message, DltMessageReader},
};
use regex::RegexBuilder;
use std::{
    fs::File,
    io::Write,
    path::PathBuf,
};

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

fn format_message(msg: &Message) -> String {
    let mut out = String::with_capacity(128);

    // Timestamp from storage header (seconds.microseconds) or standard header (0.1ms units)
    if let Some(sh) = &msg.storage_header {
        let ts = &sh.timestamp;
        out.push_str(&format!("{}.{:06} ", ts.seconds, ts.microseconds));
        out.push_str(&format!("{:<4} ", sh.ecu_id));
    } else {
        if let Some(ts) = msg.header.timestamp {
            // unit is 0.1 ms → convert to seconds.microseconds
            let us = ts as u64 * 100;
            out.push_str(&format!("{}.{:06} ", us / 1_000_000, us % 1_000_000));
        } else {
            out.push_str("-.------ ");
        }
        match &msg.header.ecu_id {
            Some(id) => out.push_str(&format!("{:<4} ", id)),
            None => out.push_str("---- "),
        }
    }

    // App ID and context ID from extended header
    if let Some(eh) = &msg.extended_header {
        out.push_str(&format!("{:<4} ", eh.application_id));
        out.push_str(&format!("{:<4} ", eh.context_id));
        out.push_str(&format!("{} ", format_message_type(&eh.message_type)));
    } else {
        out.push_str("---- ---- --- ");
    }

    // Payload
    out.push_str(&format_payload(&msg.payload));

    out
}

fn format_message_type(mt: &MessageType) -> &'static str {
    match mt {
        MessageType::Log(level) => match level {
            dlt_core::dlt::LogLevel::Fatal   => "fatal  ",
            dlt_core::dlt::LogLevel::Error   => "error  ",
            dlt_core::dlt::LogLevel::Warn    => "warn   ",
            dlt_core::dlt::LogLevel::Info    => "info   ",
            dlt_core::dlt::LogLevel::Debug   => "debug  ",
            dlt_core::dlt::LogLevel::Verbose => "verbose",
            dlt_core::dlt::LogLevel::Invalid(_) => "log?   ",
        },
        MessageType::ApplicationTrace(_) => "apptrace",
        MessageType::NetworkTrace(_)     => "nettrace",
        MessageType::Control(_)          => "control ",
        MessageType::Unknown(_)          => "unknown ",
    }
}

fn format_payload(payload: &PayloadContent) -> String {
    match payload {
        PayloadContent::Verbose(args) => args
            .iter()
            .map(|a| format_value(&a.value))
            .collect::<Vec<_>>()
            .join(" "),
        PayloadContent::NonVerbose(msg_id, bytes) => {
            if bytes.is_empty() {
                format!("[non-verbose id={msg_id}]")
            } else {
                format!(
                    "[non-verbose id={msg_id}] {}",
                    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
                )
            }
        }
        PayloadContent::ControlMsg(ctrl, bytes) => {
            let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ");
            format!("[control {ctrl:?}] {hex}")
        }
        PayloadContent::NetworkTrace(slices) => {
            let total: usize = slices.iter().map(|s| s.len()).sum();
            format!("[network-trace {total}B]")
        }
    }
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Bool(b)      => if *b != 0 { "true".into() } else { "false".into() },
        Value::U8(n)        => n.to_string(),
        Value::U16(n)       => n.to_string(),
        Value::U32(n)       => n.to_string(),
        Value::U64(n)       => n.to_string(),
        Value::U128(n)      => n.to_string(),
        Value::I8(n)        => n.to_string(),
        Value::I16(n)       => n.to_string(),
        Value::I32(n)       => n.to_string(),
        Value::I64(n)       => n.to_string(),
        Value::I128(n)      => n.to_string(),
        Value::F32(f)       => f.to_string(),
        Value::F64(f)       => f.to_string(),
        Value::StringVal(s) => s.clone(),
        Value::Raw(bytes)   => bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "),
    }
}

fn grep_file(path: &PathBuf, args: &Args, pattern: &regex::Regex, multi_file: bool) -> Result<u64> {
    let file = File::open(path)
        .with_context(|| format!("cannot open {:?}", path))?;

    let with_storage_header = !args.no_storage_header;
    let mut reader = DltMessageReader::new(file, with_storage_header);

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    let prefix = if multi_file {
        format!("{}:", path.display())
    } else {
        String::new()
    };

    let mut index: u64 = 0;
    let mut matches: u64 = 0;

    loop {
        let parsed = match read_message(&mut reader, None) {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(DltParseError::ParsingHickup(_)) => {
                index += 1;
                continue;
            }
            // Truncated message at end of file — treat as clean EOF
            Err(DltParseError::IncompleteParse { .. }) => break,
            Err(e) => return Err(e.into()),
        };

        let msg = match parsed {
            ParsedMessage::Item(m) => m,
            _ => {
                index += 1;
                continue;
            }
        };

        let line = format_message(&msg);
        let is_match = pattern.is_match(&line);
        let print = if args.invert { !is_match } else { is_match };

        if print {
            matches += 1;
            if !args.count {
                if args.line_number {
                    write!(out, "{prefix}{index}:")?;
                } else {
                    out.write_all(prefix.as_bytes())?;
                }
                writeln!(out, "{line}")?;
            }
        }

        index += 1;
    }

    if args.count {
        writeln!(out, "{prefix}{matches}")?;
    }

    Ok(matches)
}

fn main() {
    match run() {
        Ok(exit_code) => std::process::exit(exit_code),
        Err(e) => {
            // Suppress SIGPIPE / broken pipe — normal when piped to head/less
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

    let multi_file = args.files.len() > 1;
    let mut total_matches: u64 = 0;

    for file in &args.files {
        match grep_file(file, &args, &pattern, multi_file) {
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

    // Exit code 1 when no matches found, like grep
    Ok(if total_matches == 0 { 1 } else { 0 })
}
