use anyhow::{Context, Result};
use dlt_core::{
    dlt::{DltTimeStamp, Message, MessageType, PayloadContent, StorageHeader, Value},
    parse::{DltParseError, ParsedMessage},
    read::{DltMessageReader, read_message},
};
use flate2::read::GzDecoder;
use std::{
    cmp::Ordering,
    fmt::Write as FmtWrite,
    fs::File,
    io::{Read, Write},
    path::Path,
};

/// ANSI color codes (only emitted when the caller sets `GrepOpts::highlight`).
pub const COLOR_PATH: &str = "\x1b[1;35m"; // bold magenta  — file paths
pub const COLOR_MATCH: &str = "\x1b[1;31m"; // bold red      — matched text
pub const COLOR_LINENO: &str = "\x1b[1;32m"; // bold green    — line numbers
pub const COLOR_RESET: &str = "\x1b[0m";

/// Timestamp used for deterministic message ordering.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TimestampKey {
    pub seconds: u64,
    pub microseconds: u32,
}

impl TimestampKey {
    fn from_monotonic(raw: u32) -> Self {
        let us = u64::from(raw) * 100;
        Self {
            seconds: us / 1_000_000,
            microseconds: (us % 1_000_000) as u32,
        }
    }

    fn from_storage(ts: &DltTimeStamp) -> Self {
        Self {
            seconds: u64::from(ts.seconds),
            microseconds: ts.microseconds,
        }
    }
}

/// Timestamp used for sorting matching messages.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SortBy {
    /// Keep messages in file read order.
    None,
    /// Sort on the DLT standard-header timestamp (0.1 ms ticks since ECU start).
    #[default]
    Monotonic,
    /// Sort on the DLT storage-header timestamp.
    Storage,
}

/// One grep hit with preformatted output and timestamp sort keys.
#[derive(Debug, Clone)]
pub struct GrepMatch {
    pub index: u64,
    pub monotonic_timestamp: Option<TimestampKey>,
    pub storage_timestamp: Option<TimestampKey>,
    pub line: String,
}

impl GrepMatch {
    pub fn sort_key(&self, sort_by: SortBy) -> Option<TimestampKey> {
        match sort_by {
            SortBy::None => None,
            SortBy::Monotonic => self.monotonic_timestamp,
            SortBy::Storage => self.storage_timestamp,
        }
    }
}

/// Options controlling grep behaviour (output-format details live in the caller).
#[derive(Clone, Copy)]
pub struct GrepOpts {
    /// Only count matches; write a single summary line per file.
    pub count: bool,
    /// Prefix each match with its 0-based message index.
    pub line_number: bool,
    /// Invert: emit messages that do NOT match the pattern.
    pub invert: bool,
    /// DLT storage header present (normal files); false for raw streams.
    pub with_storage_header: bool,
    /// Highlight matched spans in bold red; color the line-number in bold green.
    /// The caller is responsible for also coloring the prefix it passes in.
    pub highlight: bool,
    /// Timestamp used when sorting matching messages.
    pub sort_by: SortBy,
}

fn write_monotonic_timestamp(out: &mut String, ts: Option<TimestampKey>) {
    match ts {
        Some(ts) => {
            let _ = write!(out, "{}.{:06} ", ts.seconds, ts.microseconds);
        }
        None => out.push_str("-.------ "),
    }
}

fn write_storage_timestamp(out: &mut String, ts: Option<TimestampKey>) {
    match ts {
        Some(ts) => {
            let (year, month, day) = civil_from_days((ts.seconds / 86_400) as i64);
            let second_of_day = ts.seconds % 86_400;
            let hour = second_of_day / 3_600;
            let minute = (second_of_day % 3_600) / 60;
            let second = second_of_day % 60;
            let _ = write!(
                out,
                "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{:06}Z ",
                ts.microseconds
            );
        }
        None => out.push_str("--------------------------- "),
    }
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };

    (year as i32, month as u32, day as u32)
}

fn storage_timestamp(storage_header: Option<&StorageHeader>) -> Option<TimestampKey> {
    storage_header.map(|sh| TimestampKey::from_storage(&sh.timestamp))
}

fn monotonic_timestamp(msg: &Message) -> Option<TimestampKey> {
    msg.header.timestamp.map(TimestampKey::from_monotonic)
}

pub fn format_message(msg: &Message) -> String {
    let mut out = String::with_capacity(168);

    write_monotonic_timestamp(&mut out, monotonic_timestamp(msg));
    write_storage_timestamp(&mut out, storage_timestamp(msg.storage_header.as_ref()));

    if let Some(sh) = &msg.storage_header {
        let _ = write!(out, "{:<4} ", sh.ecu_id);
    } else {
        match &msg.header.ecu_id {
            Some(id) => {
                let _ = write!(out, "{:<4} ", id);
            }
            None => out.push_str("---- "),
        }
    }

    if let Some(eh) = &msg.extended_header {
        let _ = write!(out, "{:<4} ", eh.application_id);
        let _ = write!(out, "{:<4} ", eh.context_id);
        let _ = write!(out, "{} ", format_message_type(&eh.message_type));
    } else {
        out.push_str("---- ---- --- ");
    }

    out.push_str(&format_payload(&msg.payload));
    out
}

pub fn format_message_type(mt: &MessageType) -> &'static str {
    match mt {
        MessageType::Log(level) => match level {
            dlt_core::dlt::LogLevel::Fatal => "fatal  ",
            dlt_core::dlt::LogLevel::Error => "error  ",
            dlt_core::dlt::LogLevel::Warn => "warn   ",
            dlt_core::dlt::LogLevel::Info => "info   ",
            dlt_core::dlt::LogLevel::Debug => "debug  ",
            dlt_core::dlt::LogLevel::Verbose => "verbose",
            dlt_core::dlt::LogLevel::Invalid(_) => "log?   ",
        },
        MessageType::ApplicationTrace(_) => "apptrace",
        MessageType::NetworkTrace(_) => "nettrace",
        MessageType::Control(_) => "control ",
        MessageType::Unknown(_) => "unknown ",
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
                    bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<Vec<_>>()
                        .join(" ")
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
        Value::Bool(b) => {
            if *b != 0 {
                "true".into()
            } else {
                "false".into()
            }
        }
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::F32(f) => f.to_string(),
        Value::F64(f) => f.to_string(),
        Value::StringVal(s) => s.clone(),
        Value::Raw(bytes) => bytes
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Writes `line` to `out` with every match of `pattern` wrapped in bold-red
/// ANSI codes.  Falls back to a plain write when the pattern has no match.
fn highlight_matches<W: Write>(
    line: &str,
    pattern: &regex::Regex,
    out: &mut W,
) -> std::io::Result<()> {
    let mut last = 0;
    for m in pattern.find_iter(line) {
        out.write_all(line[last..m.start()].as_bytes())?;
        out.write_all(COLOR_MATCH.as_bytes())?;
        out.write_all(m.as_str().as_bytes())?;
        out.write_all(COLOR_RESET.as_bytes())?;
        last = m.end();
    }
    out.write_all(line[last..].as_bytes())
}

fn cmp_optional_timestamp(a: Option<TimestampKey>, b: Option<TimestampKey>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub fn sort_matches(matches: &mut [GrepMatch], sort_by: SortBy) {
    if sort_by == SortBy::None {
        return;
    }

    matches.sort_by(|a, b| {
        cmp_optional_timestamp(a.sort_key(sort_by), b.sort_key(sort_by))
            .then_with(|| a.index.cmp(&b.index))
    });
}

pub fn write_grep_match<W: Write>(
    m: &GrepMatch,
    pattern: &regex::Regex,
    opts: &GrepOpts,
    out: &mut W,
    prefix: &str,
) -> std::io::Result<()> {
    if opts.line_number {
        if opts.highlight {
            write!(out, "{prefix}{COLOR_LINENO}{}{COLOR_RESET}:", m.index)?;
        } else {
            write!(out, "{prefix}{}:", m.index)?;
        }
    } else {
        out.write_all(prefix.as_bytes())?;
    }

    // In invert mode the line doesn't contain the pattern, so there's
    // nothing to highlight; skip the extra find_iter pass.
    if opts.highlight && !opts.invert {
        highlight_matches(&m.line, pattern, out)?;
    } else {
        out.write_all(m.line.as_bytes())?;
    }
    out.write_all(b"\n")
}

pub fn write_grep_matches<W: Write>(
    matches: &[GrepMatch],
    pattern: &regex::Regex,
    opts: &GrepOpts,
    out: &mut W,
    prefix: &str,
) -> std::io::Result<()> {
    for m in matches {
        write_grep_match(m, pattern, opts, out, prefix)?;
    }
    Ok(())
}

fn open_dlt_reader(path: &Path) -> Result<Box<dyn Read>> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;

    match path.extension() {
        Some(extension) if extension.eq_ignore_ascii_case("gz") => {
            Ok(Box::new(GzDecoder::new(file)))
        }
        Some(extension) if extension.eq_ignore_ascii_case("zst") => {
            let decoder = zstd::stream::read::Decoder::new(file)
                .with_context(|| format!("cannot decompress {}", path.display()))?;
            Ok(Box::new(decoder))
        }
        _ => Ok(Box::new(file)),
    }
}

pub fn grep_file_matches(
    path: &Path,
    pattern: &regex::Regex,
    opts: &GrepOpts,
) -> Result<(Vec<GrepMatch>, u64)> {
    let mut reader = DltMessageReader::new(open_dlt_reader(path)?, opts.with_storage_header);

    let mut index: u64 = 0;
    let mut matches: u64 = 0;
    let mut records: Vec<GrepMatch> = Vec::new();

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
                records.push(GrepMatch {
                    index,
                    monotonic_timestamp: monotonic_timestamp(&msg),
                    storage_timestamp: storage_timestamp(msg.storage_header.as_ref()),
                    line,
                });
            }
        }

        index += 1;
    }

    Ok((records, matches))
}

fn grep_file_unsorted<W: Write>(
    path: &Path,
    pattern: &regex::Regex,
    opts: &GrepOpts,
    out: &mut W,
    prefix: &str,
) -> Result<u64> {
    let mut reader = DltMessageReader::new(open_dlt_reader(path)?, opts.with_storage_header);

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
                write_grep_match(
                    &GrepMatch {
                        index,
                        monotonic_timestamp: None,
                        storage_timestamp: None,
                        line,
                    },
                    pattern,
                    opts,
                    out,
                    prefix,
                )?;
            }
        }

        index += 1;
    }

    Ok(matches)
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
    if opts.sort_by == SortBy::None && !opts.count {
        return grep_file_unsorted(path, pattern, opts, out, prefix);
    }

    let (mut records, matches) = grep_file_matches(path, pattern, opts)?;

    if opts.count {
        writeln!(out, "{prefix}{matches}")?;
    } else {
        sort_matches(&mut records, opts.sort_by);
        write_grep_matches(&records, pattern, opts, out, prefix)?;
    }

    Ok(matches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dlt_core::dlt::{Endianness, MessageConfig};
    use regex::Regex;

    fn message(monotonic_ticks: u32, storage_seconds: u32, msg_id: u32) -> Message {
        Message::new(
            MessageConfig {
                version: 1,
                counter: 0,
                endianness: Endianness::Big,
                ecu_id: Some("ECU".to_string()),
                session_id: None,
                timestamp: Some(monotonic_ticks),
                payload: PayloadContent::NonVerbose(msg_id, Vec::new()),
                extended_header_info: None,
            },
            Some(StorageHeader {
                timestamp: DltTimeStamp {
                    seconds: storage_seconds,
                    microseconds: 123_456,
                },
                ecu_id: "ECU".to_string(),
            }),
        )
    }

    fn opts(sort_by: SortBy) -> GrepOpts {
        GrepOpts {
            count: false,
            line_number: false,
            invert: false,
            with_storage_header: true,
            highlight: false,
            sort_by,
        }
    }

    #[test]
    fn format_message_prints_monotonic_then_iso_storage_timestamp() {
        let line = format_message(&message(12_345, 1_000, 7));

        assert!(line.starts_with("1.234500 1970-01-01T00:16:40.123456Z ECU  "));
        assert!(line.ends_with("[non-verbose id=7]"));
    }

    #[test]
    fn grep_file_sorts_matches_by_selected_timestamp_or_keeps_file_order() {
        let path = std::env::temp_dir().join(format!(
            "dlt-grep-sort-{}-{}.dlt",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));

        {
            let mut file = File::create(&path).expect("create temp dlt");
            file.write_all(&message(20_000, 30, 1).as_bytes())
                .expect("write first message");
            file.write_all(&message(10_000, 40, 2).as_bytes())
                .expect("write second message");
        }

        let pattern = Regex::new(".").unwrap();

        let mut no_sort = Vec::new();
        let no_sort_count = grep_file(&path, &pattern, &opts(SortBy::None), &mut no_sort, "")
            .expect("grep no sort");
        let no_sort = String::from_utf8(no_sort).unwrap();
        assert_eq!(no_sort_count, 2);
        assert!(
            no_sort.find("[non-verbose id=1]").unwrap()
                < no_sort.find("[non-verbose id=2]").unwrap()
        );

        let mut monotonic = Vec::new();
        let monotonic_count = grep_file(
            &path,
            &pattern,
            &opts(SortBy::Monotonic),
            &mut monotonic,
            "",
        )
        .expect("grep monotonic");
        let monotonic = String::from_utf8(monotonic).unwrap();
        assert_eq!(monotonic_count, 2);
        assert!(
            monotonic.find("[non-verbose id=2]").unwrap()
                < monotonic.find("[non-verbose id=1]").unwrap()
        );

        let mut storage = Vec::new();
        let storage_count = grep_file(&path, &pattern, &opts(SortBy::Storage), &mut storage, "")
            .expect("grep storage");
        let storage = String::from_utf8(storage).unwrap();
        assert_eq!(storage_count, 2);
        assert!(
            storage.find("[non-verbose id=1]").unwrap()
                < storage.find("[non-verbose id=2]").unwrap()
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn grep_file_reads_gzip_and_zstd_inputs() {
        let base = std::env::temp_dir().join(format!(
            "dlt-grep-compression-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let bytes = message(10_000, 20, 42).as_bytes();
        let gzip_path = base.with_extension("dlt.gz");
        let zstd_path = base.with_extension("dlt.zst");

        {
            let file = File::create(&gzip_path).expect("create gzip file");
            let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            encoder.write_all(&bytes).expect("compress gzip input");
            encoder.finish().expect("finish gzip input");
        }
        std::fs::write(
            &zstd_path,
            zstd::stream::encode_all(bytes.as_slice(), 0).expect("compress zstd input"),
        )
        .expect("write zstd file");

        let pattern = Regex::new("id=42").unwrap();
        for (path, sort_by) in [(&gzip_path, SortBy::Monotonic), (&zstd_path, SortBy::None)] {
            let mut output = Vec::new();
            let matches = grep_file(path, &pattern, &opts(sort_by), &mut output, "")
                .expect("grep compressed DLT");
            assert_eq!(matches, 1);
            assert!(
                String::from_utf8(output)
                    .unwrap()
                    .contains("[non-verbose id=42]")
            );
        }

        std::fs::remove_file(gzip_path).expect("remove gzip file");
        std::fs::remove_file(zstd_path).expect("remove zstd file");
    }
}
