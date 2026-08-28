//! Local gas profiling: run the `gas_profile_*` tests, parse their output and
//! render a console table and/or a JSON report.
//!
//! The tests emit one line per measurement in the form:
//!
//! ```text
//! gas_profile | <label> | cpu=<insns> | mem=<bytes>
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;

use crate::process::{cargo, now_secs, run_capture};

/// Cargo arguments used to run only the gas-profile tests.
const DEFAULT_GAS_TEST_ARGS: &[&str] = &[
    "test",
    "--package",
    "trustlink-escrow",
    "--lib",
    "--",
    "gas_profile_",
    "--nocapture",
];

/// Column widths of the console table: operation, cpu, mem, category.
const COLUMN_WIDTHS: [usize; 4] = [42, 14, 12, 16];

/// A single measurement parsed out of the test output.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GasMetric {
    pub label: String,
    pub cpu_insns: u64,
    pub mem_bytes: u64,
    pub category: String,
}

/// Every measurement from one run, plus totals and a per-category index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GasReport {
    pub generated_at: u64,
    pub version: String,
    pub total_samples: usize,
    pub total_cpu_insns: u64,
    pub total_mem_bytes: u64,
    pub metrics: Vec<GasMetric>,
    pub by_category: BTreeMap<String, Vec<GasMetric>>,
}

/// Options accepted after `--` on the `gas-profile` subcommand.
#[derive(Debug, Default, PartialEq, Eq)]
struct GasOptions {
    out_path: Option<String>,
    category_filter: Option<String>,
    show_table: bool,
    /// Anything unrecognised, forwarded to the test binary verbatim.
    forwarded: Vec<String>,
}

/// Classification rules, applied in order — the first match wins.
///
/// Each entry is `(category, substrings, prefixes)`: a label belongs to the
/// category if it contains any substring or starts with any prefix.
const CATEGORY_RULES: &[(&str, &[&str], &[&str])] = &[
    ("views", &["view"], &["get_"]),
    ("creation", &["create"], &["initialize"]),
    ("lifecycle", &["fund", "mark_shipped"], &[]),
    ("completion", &["confirm", "release", "co_signed"], &[]),
    (
        "dispute",
        &["dispute", "vote", "appeal", "finalize", "raise", "resolve"],
        &[],
    ),
    ("cancel-refund", &["cancel", "refund", "drain"], &[]),
    (
        "admin-config",
        &["admin", "fee", "pause", "set_", "rotate"],
        &[],
    ),
    (
        "advanced",
        &["message", "batch", "multicall", "basket"],
        &[],
    ),
];

/// Bucket a measurement label into a category for the summary rows.
pub fn categorize(label: &str) -> String {
    let lower = label.to_lowercase();
    for (category, substrings, prefixes) in CATEGORY_RULES {
        let matched = substrings.iter().any(|s| lower.contains(s))
            || prefixes.iter().any(|p| lower.starts_with(p));
        if matched {
            return (*category).to_string();
        }
    }
    "other".to_string()
}

/// Extract `key=value` from a `cpu=123` / `mem=456` fragment.
fn parse_kv(fragment: &str) -> Option<u64> {
    fragment.split('=').nth(1)?.trim().parse::<u64>().ok()
}

/// Pull every `gas_profile | ...` line out of captured test output.
///
/// Malformed lines are skipped rather than failing the run — a single bad
/// measurement should not discard an otherwise useful report.
pub fn parse_gas_output(stdout: &str) -> Vec<GasMetric> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if !line.starts_with("gas_profile |") {
            continue;
        }
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            continue;
        }
        let (Some(cpu_insns), Some(mem_bytes)) = (parse_kv(parts[2]), parse_kv(parts[3])) else {
            continue;
        };
        let label = parts[1].trim().to_string();
        out.push(GasMetric {
            category: categorize(&label),
            label,
            cpu_insns,
            mem_bytes,
        });
    }
    out
}

/// Aggregate measurements into a report with totals and category buckets.
pub fn build_report(metrics: Vec<GasMetric>) -> GasReport {
    let total_cpu_insns = metrics.iter().map(|m| m.cpu_insns).sum();
    let total_mem_bytes = metrics.iter().map(|m| m.mem_bytes).sum();

    let mut by_category: BTreeMap<String, Vec<GasMetric>> = BTreeMap::new();
    for m in &metrics {
        by_category
            .entry(m.category.clone())
            .or_default()
            .push(m.clone());
    }

    GasReport {
        generated_at: now_secs(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        total_samples: metrics.len(),
        total_cpu_insns,
        total_mem_bytes,
        metrics,
        by_category,
    }
}

/// True when `value` passes the optional `--category` prefix filter.
fn included(value: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| value.starts_with(f))
}

fn print_table(report: &GasReport, category_filter: Option<&str>) {
    let [w0, w1, w2, w3] = COLUMN_WIDTHS;

    println!("\n=== Gas Profile Report ===");
    println!(
        "Generated: {} | Samples: {} | Total CPU: {} | Total Mem: {}",
        report.generated_at, report.total_samples, report.total_cpu_insns, report.total_mem_bytes
    );
    println!();
    println!(
        "{:<w0$} | {:>w1$} | {:>w2$} | {:<w3$}",
        "Operation", "CPU Insns", "Mem Bytes", "Category"
    );
    println!("{0:-<w0$}-+-{0:-<w1$}-+-{0:-<w2$}-+-{0:-<w3$}", "");

    for m in &report.metrics {
        if !included(&m.category, category_filter) {
            continue;
        }
        println!(
            "{:<w0$} | {:>w1$} | {:>w2$} | {:<w3$}",
            m.label, m.cpu_insns, m.mem_bytes, m.category
        );
    }
    println!();

    for (category, ms) in &report.by_category {
        if ms.is_empty() || !included(category, category_filter) {
            continue;
        }
        let sum_cpu: u64 = ms.iter().map(|m| m.cpu_insns).sum();
        let sum_mem: u64 = ms.iter().map(|m| m.mem_bytes).sum();
        let n = ms.len() as u64;
        println!(
            "Category [{:<12}] n={:<2} sum_cpu={:>12} avg_cpu={:>10} sum_mem={:>10} avg_mem={:>8}",
            category,
            ms.len(),
            sum_cpu,
            sum_cpu / n,
            sum_mem,
            sum_mem / n
        );
    }
    println!("=== End Gas Report ===\n");
}

fn parse_options(extra: &[String]) -> Result<GasOptions, String> {
    let mut opts = GasOptions {
        show_table: true,
        ..GasOptions::default()
    };
    let mut it = extra.iter().cloned();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--out" => opts.out_path = Some(it.next().ok_or("--out requires a path")?),
            "--category" => {
                opts.category_filter = Some(it.next().ok_or("--category requires a prefix")?)
            }
            "--no-table" => opts.show_table = false,
            other => opts.forwarded.push(other.to_string()),
        }
    }
    Ok(opts)
}

/// Serialize `report` to `path` as pretty-printed JSON.
pub fn write_json<T: Serialize>(report: &T, path: &str, what: &str) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(report).map_err(|e| format!("serialize {what}: {e}"))?;
    fs::write(path, &json).map_err(|e| format!("write {path}: {e}"))?;
    Ok(())
}

/// Entry point for `cargo xtask gas-profile`.
pub fn run(extra: &[String]) -> Result<(), String> {
    let opts = parse_options(extra)?;

    let mut args: Vec<String> = DEFAULT_GAS_TEST_ARGS
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(opts.forwarded);

    println!("==> Running gas-profile tests (package=trustlink-escrow)");
    let stdout = run_capture(cargo(&[], &args))?;

    let metrics = parse_gas_output(&stdout);
    if metrics.is_empty() {
        return Err("no gas_profile metrics were emitted. Did the tests fail to run?".into());
    }

    let report = build_report(metrics);
    if opts.show_table {
        print_table(&report, opts.category_filter.as_deref());
    }
    if let Some(path) = opts.out_path {
        write_json(&report, &path, "report")?;
        println!("Wrote JSON gas report to: {path}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorize_buckets_labels_by_first_matching_rule() {
        assert_eq!(categorize("get_escrow"), "views");
        assert_eq!(categorize("view_stats"), "views");
        assert_eq!(categorize("create_escrow"), "creation");
        assert_eq!(categorize("initialize"), "creation");
        assert_eq!(categorize("fund_escrow"), "lifecycle");
        assert_eq!(categorize("mark_shipped"), "lifecycle");
        assert_eq!(categorize("auto_release"), "completion");
        assert_eq!(categorize("co_signed_release"), "completion");
        assert_eq!(categorize("raise_dispute"), "dispute");
        assert_eq!(categorize("cancel_escrow"), "cancel-refund");
        assert_eq!(categorize("set_fee"), "admin-config");
        assert_eq!(categorize("multicall"), "advanced");
        assert_eq!(categorize("something_else"), "other");
    }

    #[test]
    fn categorize_is_case_insensitive() {
        assert_eq!(categorize("GET_ESCROW"), "views");
        assert_eq!(categorize("Raise_Dispute"), "dispute");
    }

    #[test]
    fn parse_gas_output_reads_well_formed_lines() {
        let stdout = "\
noise before
gas_profile | create_escrow | cpu=1234 | mem=567
  gas_profile | get_escrow | cpu=10 | mem=20
noise after
";
        let metrics = parse_gas_output(stdout);
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].label, "create_escrow");
        assert_eq!(metrics[0].cpu_insns, 1234);
        assert_eq!(metrics[0].mem_bytes, 567);
        assert_eq!(metrics[0].category, "creation");
        assert_eq!(metrics[1].category, "views");
    }

    #[test]
    fn parse_gas_output_skips_malformed_lines() {
        let stdout = "\
gas_profile | too_few_fields
gas_profile | bad_cpu | cpu=abc | mem=1
gas_profile | bad_mem | cpu=1 | mem=xyz
gas_profile | good | cpu=1 | mem=2
";
        let metrics = parse_gas_output(stdout);
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].label, "good");
    }

    #[test]
    fn build_report_totals_and_groups() {
        let metrics = parse_gas_output(
            "\
gas_profile | create_escrow | cpu=100 | mem=10
gas_profile | get_escrow | cpu=5 | mem=1
gas_profile | get_dispute | cpu=7 | mem=3
",
        );
        let report = build_report(metrics);
        assert_eq!(report.total_samples, 3);
        assert_eq!(report.total_cpu_insns, 112);
        assert_eq!(report.total_mem_bytes, 14);
        assert_eq!(report.by_category["views"].len(), 2);
        assert_eq!(report.by_category["creation"].len(), 1);
    }

    #[test]
    fn parse_options_reads_flags_and_forwards_the_rest() {
        let extra: Vec<String> = [
            "--out",
            "r.json",
            "--category",
            "views",
            "--no-table",
            "--exact",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let opts = parse_options(&extra).expect("parse");
        assert_eq!(opts.out_path.as_deref(), Some("r.json"));
        assert_eq!(opts.category_filter.as_deref(), Some("views"));
        assert!(!opts.show_table);
        assert_eq!(opts.forwarded, vec!["--exact".to_string()]);
    }

    #[test]
    fn parse_options_defaults_to_showing_the_table() {
        let opts = parse_options(&[]).expect("parse");
        assert!(opts.show_table);
        assert!(opts.out_path.is_none());
        assert!(opts.category_filter.is_none());
    }

    #[test]
    fn parse_options_rejects_flags_missing_their_value() {
        assert!(parse_options(&["--out".to_string()]).is_err());
        assert!(parse_options(&["--category".to_string()]).is_err());
    }

    #[test]
    fn category_filter_matches_on_prefix() {
        assert!(included("views", None));
        assert!(included("views", Some("vie")));
        assert!(!included("dispute", Some("vie")));
    }
}
