use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dlt_grep::{grep_file, GrepOpts};
use regex::RegexBuilder;
use std::path::Path;

const DLT_FILE: &str = "/home/falk/sources/logs/SPA3/ARTHTP-6395/ECU1_DHU_UXC_HPA_SGA_TCA_2026-05-03T23_50-21.958799TZ02-00.dlt";

/// Patterns to benchmark: (label, pattern, case_insensitive)
const PATTERNS: &[(&str, &str, bool)] = &[
    // Baseline — every message matches; measures pure parse + format cost
    ("match_all",           r".",                       false),
    // Simple literal, ~20% hit rate
    ("literal_error",       r"error",                   false),
    // Same literal, case-insensitive — exercises regex case-fold path
    ("literal_error_icase", r"error",                   true),
    // Anchored prefix — short-circuits after ECU field
    ("anchor_ecu_HPA",      r"^[0-9.]+ HPA ",          false),
    // Alternation
    ("alternation",         r"warn   |error  ",         false),
    // Multi-segment: app id + keyword anywhere in payload
    ("multi_segment",       r"DFLT.*failed",            false),
    // Rare match — nearly nothing matches; measures the no-match fast path
    ("rare_match",          r"__unlikely_sentinel__",   false),
];

fn bench_grep(c: &mut Criterion) {
    let path = Path::new(DLT_FILE);
    if !path.exists() {
        eprintln!("bench: DLT file not found, skipping — {DLT_FILE}");
        return;
    }

    let file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let opts = GrepOpts {
        count: true,       // suppress stdout; we only measure parse + match
        line_number: false,
        invert: false,
        with_storage_header: true,
    };

    let mut group = c.benchmark_group("regex_patterns");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(file_bytes));

    for &(label, pat, icase) in PATTERNS {
        let re = RegexBuilder::new(pat)
            .case_insensitive(icase)
            .build()
            .unwrap_or_else(|e| panic!("bad pattern {pat:?}: {e}"));

        group.bench_with_input(BenchmarkId::from_parameter(label), label, |b, _| {
            b.iter(|| {
                grep_file(path, &re, &opts, &mut std::io::sink(), "")
                    .expect("grep_file failed")
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_grep);
criterion_main!(benches);
