use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dlt_grep::{grep_file, GrepOpts};
use rayon::prelude::*;
use regex::RegexBuilder;
use std::path::PathBuf;
use walkdir::WalkDir;

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

fn make_opts() -> GrepOpts {
    GrepOpts {
        count: true,       // suppress stdout; we only measure parse + match
        line_number: false,
        invert: false,
        with_storage_header: true,
        highlight: false,
    }
}

/// Single-file microbenchmark.
///
/// Set the `DG_BENCH_FILE` environment variable to the DLT file to use:
///
///   DG_BENCH_FILE=/path/to/large.dlt cargo bench --bench grep single_file
///
/// The group is skipped with a notice when the variable is unset.
fn bench_grep(c: &mut Criterion) {
    let path = match std::env::var_os("DG_BENCH_FILE") {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("bench: skipping single_file — set DG_BENCH_FILE=/path/to/file.dlt");
            return;
        }
    };
    if !path.exists() {
        eprintln!("bench: skipping single_file — file not found: {}", path.display());
        return;
    }

    let file_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let opts = make_opts();

    let mut group = c.benchmark_group("single_file");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(file_bytes));

    for &(label, pat, icase) in PATTERNS {
        let re = RegexBuilder::new(pat)
            .case_insensitive(icase)
            .build()
            .unwrap_or_else(|e| panic!("bad pattern {pat:?}: {e}"));

        group.bench_with_input(BenchmarkId::from_parameter(label), label, |b, _| {
            b.iter(|| {
                grep_file(&path, &re, &opts, &mut std::io::sink(), "")
                    .expect("grep_file failed")
            });
        });
    }

    group.finish();
}

/// Multi-file parallel benchmark.
///
/// Set the `DG_BENCH_DIR` environment variable to a directory containing
/// `*.dlt` files.  All files found recursively are processed with
/// `rayon::par_iter` — the same execution model as `dg <dir>`.
/// Throughput is reported in bytes/s against the total size of all files.
///
///   DG_BENCH_DIR=/path/to/logs cargo bench --bench grep multi_file
///
/// The group is skipped with a notice when the variable is unset.
fn bench_grep_dir(c: &mut Criterion) {
    let dir = match std::env::var_os("DG_BENCH_DIR") {
        Some(d) => PathBuf::from(d),
        None => {
            eprintln!("bench: skipping multi_file — set DG_BENCH_DIR=/path/to/dir");
            return;
        }
    };
    if !dir.exists() {
        eprintln!("bench: skipping multi_file — dir not found: {}", dir.display());
        return;
    }

    let mut files: Vec<PathBuf> = WalkDir::new(&dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().is_file()
                && e.path()
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case("dlt"))
                    .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort_unstable();

    if files.is_empty() {
        eprintln!("bench: skipping multi_file — no *.dlt files found in {}", dir.display());
        return;
    }

    let total_bytes: u64 = files
        .iter()
        .filter_map(|f| std::fs::metadata(f).ok())
        .map(|m| m.len())
        .sum();

    eprintln!(
        "bench: multi_file — {} files, {:.1} MB in {}",
        files.len(),
        total_bytes as f64 / 1_048_576.0,
        dir.display(),
    );

    let opts = make_opts();

    let mut group = c.benchmark_group("multi_file");
    group.sample_size(3);
    group.throughput(Throughput::Bytes(total_bytes));

    for &(label, pat, icase) in PATTERNS {
        let re = RegexBuilder::new(pat)
            .case_insensitive(icase)
            .build()
            .unwrap_or_else(|e| panic!("bad pattern {pat:?}: {e}"));

        group.bench_with_input(
            BenchmarkId::from_parameter(label),
            label,
            |b, _| {
                b.iter(|| {
                    files
                        .par_iter()
                        .map(|p| {
                            grep_file(p, &re, &opts, &mut std::io::sink(), "")
                                .expect("grep_file failed")
                        })
                        .sum::<u64>()
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_grep, bench_grep_dir);
criterion_main!(benches);
