use crate::gas::parser::{build_report, parse_gas_output};
use crate::gas::table::print_table;
use crate::tasks::{cargo, run};
use std::fs;

const GAS_TEST_FILTER: &str = "gas_profile_";
const DEFAULT_GAS_TEST_ARGS: &[&str] = &[
    "test",
    "--package",
    "trustlink-escrow",
    "--lib",
    "--",
    GAS_TEST_FILTER,
    "--nocapture",
];

pub fn run_gas_profile(extra: &[String]) -> Result<(), String> {
    let mut out_path: Option<String> = None;
    let mut category_filter: Option<String> = None;
    let mut show_table = true;
    let mut filtered_extra = Vec::new();
    let mut it = extra.iter().cloned();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => {
                out_path = Some(it.next().ok_or("--out requires a path")?);
            }
            "--category" => {
                category_filter = Some(it.next().ok_or("--category requires a prefix")?);
            }
            "--no-table" => {
                show_table = false;
            }
            other => {
                filtered_extra.push(other.to_string());
            }
        }
    }

    let mut base_args: Vec<String> = DEFAULT_GAS_TEST_ARGS.iter().map(|s| s.to_string()).collect();
    base_args.extend(filtered_extra);

    println!("==> Running gas-profile tests (package=trustlink-escrow)");
    let stdout = run(cargo(&[], &base_args))?;
    let metrics = parse_gas_output(&stdout);
    if metrics.is_empty() {
        return Err("no gas_profile metrics were emitted. Did the tests fail to run?".into());
    }
    let report = build_report(metrics);
    if show_table {
        print_table(&report, category_filter.as_deref());
    }
    if let Some(p) = out_path {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| format!("serialize report: {e}"))?;
        fs::write(&p, &json).map_err(|e| format!("write {p}: {e}"))?;
        println!("Wrote JSON gas report to: {p}");
    }
    Ok(())
}
