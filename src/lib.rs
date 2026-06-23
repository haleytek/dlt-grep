use anyhow::{Context, Result};
use dlt_core::{
    dlt::{Message, MessageType, PayloadContent, Value},
    parse::{DltParseError, ParsedMessage},
    read::{read_message, DltMessageReader},
};
use std::{fs::File, io::Write, path::Path};

/// Options controlling grep behaviour (output-format details live in the caller).
pub struct GrepOpts {
    /// Only count matches; write a single summary line per file.
    pub count: bool,
    /// Prefix each match with its 0-based message index.
    pub line_number: bool,
    /// Invert: emit messages that do NOT match the pattern.
    pub invert: bool,
    /// DLT storage header present (normal files); false for raw streams.
    pub with_storage_header: bool,
}

pub fn format_message(msg: &Message) -> String {
    let mut out = String::with_capacity(128);

    if let Some(sh) = &msg.storage_header {
        let ts = &sh.timestamp;
        out.push_str(&format!("{}.{:06} ", ts.seconds, ts.microseconds));
        out.push_str(&format!("{:<4} ", sh.ecu_id));
    } else {
        if let Some(ts) = msg.header.timestamp {
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

    if let Some(eh) = &msg.extended_header {
        out.push_str(&format!("{:<4} ", eh.application_id));
        out.push_str(&format!("{:<4} ", eh.context_id));
        out.push_str(&format!("{} ", format_message_type(&eh.message_type)));
    } else {
        out.push_str("---- ---- --- ");
    }

    out.push_str(&format_payload(&msg.payload));
    out
}

pub fn format_message_type(mt: &MessageType) -> &'static str {
    match mt {
        MessageType::Log(level) => match level {
            dlt_core::dlt::LogLevel::Fatal      => "fatal  ",
            dlt_core::dlt::LogLevel::Error      => "error  ",
            dlt_core::dlt::LogLevel::Warn       => "warn   ",
            dlt_core::dlt::LogLevel::Info       => "info   ",
            dlt_core::dlt::LogLevel::Debug      => "debug  ",
            dlt_core::dlt::LogLevel::Verbose    => "verbose",
            dlt_core::dlt::LogLevel::Invalid(_) => "log?   ",
        },
        MessageType::ApplicationTrace(_) => "apptrace",
        MessageType::NetworkTrace(_)     => "nettrace",
        MessageType::Control(_)          => "control ",
        MessageType::Unknown(_)          => "unknown ",
    }
}

pub fn format_payload(payload: &PayloadContent) -> String {
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
            let hex: String = bytes
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            format!("[control {ctrl:?}] {hex}")
        }
        PayloadContent::NetworkTrace(slices) => {
            let total: usize = slices.iter().map(|s| s.len()).sum();
            format!("[network-trace {total}B]")
        }
    }
}

pub fn format_value(v: &Value) -> String {
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
        Value::Raw(bytes)   => bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Search `path` for messages matching `pattern`, writing results to `out`.
/// `prefix` is prepended to every output line (used for multi-file mode).
pub fn grep_file<W: Write>(
    path: &Path,
    pattern: &regex::Regex,
    opts: &GrepOpts,
    out: &mut W,
    prefix: &str,
) -> Result<u64> {
    let file = File::open(path)
        .with_context(|| format!("cannot open {}", path.display()))?;

    let mut reader = DltMessageReader::new(file, opts.with_storage_header);

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
        let emit = if opts.invert { !is_match } else { is_match };

        if emit {
            matches += 1;
            if !opts.count {
                if opts.line_number {
                    write!(out, "{prefix}{index}:")?;
                } else {
                    out.write_all(prefix.as_bytes())?;
                }
                writeln!(out, "{line}")?;
            }
        }

        index += 1;
    }

    if opts.count {
        writeln!(out, "{prefix}{matches}")?;
    }

    Ok(matches)
}
