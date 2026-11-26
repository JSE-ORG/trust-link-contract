//! Network gas profiling: deploy (or reuse) a contract on a Stellar network,
//! invoke a handful of entry points through the Stellar CLI and summarise what
//! each transaction cost.
//!
//! Only the fee and transaction hash are available from the CLI today; the
//! CPU/RAM fields are reserved for when a Soroban RPC that returns
//! `sorobanResources` meta is wired in.

use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

use crate::gas::write_json;
use crate::process::{now_secs, run_capture, stellar};

const WASM_PATH: &str = "target/wasm32v1-none/release/trustlink_escrow.wasm";

/// Placeholder recorded when an invocation fails; the run continues.
const FAILED: &str = "ERROR";

/// One profiled contract invocation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkGasTx {
    pub operation: String,
    pub tx_hash: String,
    pub fee_charged: String,
    pub cpu_instructions: Option<u64>,
    pub ram_bytes: Option<u64>,
    pub ledger_reads: Option<u64>,
    pub ledger_writes: Option<u64>,
}

impl NetworkGasTx {
    fn failed(operation: &str) -> Self {
        Self {
            operation: operation.to_string(),
            tx_hash: FAILED.to_string(),
            fee_charged: FAILED.to_string(),
            cpu_instructions: None,
            ram_bytes: None,
            ledger_reads: None,
            ledger_writes: None,
        }
    }
}

/// All invocations from one network profiling run.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkGasReport {
    pub generated_at: u64,
    pub network: String,
    pub contract_id: String,
    pub transactions: Vec<NetworkGasTx>,
}

/// Options accepted after `--` on the `gas-profile-network` subcommand.
#[derive(Debug, PartialEq, Eq)]
struct NetworkOptions {
    network: String,
    source: String,
    contract_id: Option<String>,
    out_path: Option<String>,
}

impl Default for NetworkOptions {
    fn default() -> Self {
        Self {
            network: "standalone".to_string(),
            source: "alice".to_string(),
            contract_id: None,
            out_path: None,
        }
    }
}

fn parse_options(extra: &[String]) -> Result<NetworkOptions, String> {
    let mut opts = NetworkOptions::default();
    let mut it = extra.iter().cloned();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--network" => opts.network = it.next().ok_or("--network requires a value")?,
            "--source" => opts.source = it.next().ok_or("--source requires a value")?,
            "--contract" => {
                opts.contract_id = Some(it.next().ok_or("--contract requires a value")?)
            }
            "--out" => opts.out_path = Some(it.next().ok_or("--out requires a path")?),
            other => return Err(format!("unknown gas-profile-network argument: {other}")),
        }
    }
    Ok(opts)
}

/// Pull the value out of the first line whose key matches any of `keys`.
///
/// The Stellar CLI prints `Key: value` lines interleaved with other output, so
/// this scans for the first recognisable one instead of a fixed position.
fn field_from_output(output: &str, keys: &[&str], fallback: &str) -> String {
    output
        .lines()
        .find(|line| keys.iter().any(|k| line.contains(k)))
        .and_then(|line| line.split(':').nth(1))
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| fallback.to_string())
}

/// Deploy a fresh contract and return its id.
fn deploy(network: &str, source: &str) -> Result<String, String> {
    println!("    Deploying fresh contract instance...");
    let stdout = run_capture(stellar(
        &[
            "contract",
            "deploy",
            "--wasm",
            WASM_PATH,
            "--network",
            network,
            "--source",
            source,
        ],
        &[],
    ))?;
    Ok(stdout
        .trim()
        .lines()
        .next_back()
        .unwrap_or("")
        .trim()
        .to_string())
}

/// Resolve the public address behind a Stellar CLI account name.
fn account_address(network: &str, source: &str) -> String {
    let output = run_capture(stellar(
        &["keys", "address", "--account", source, "--network", network],
        &[],
    ))
    .unwrap_or_else(|_| source.to_string());
    output
        .lines()
        .next_back()
        .unwrap_or(source)
        .trim()
        .to_string()
}

/// Invoke one contract function and summarise the resulting transaction.
///
/// A failed invocation is reported as a placeholder row rather than aborting,
/// so one unsupported entry point cannot discard the whole report.
fn invoke(
    contract_id: &str,
    network: &str,
    source: &str,
    function: &str,
    args: &[String],
    label: &str,
) -> Result<NetworkGasTx, String> {
    let mut cmd_args: Vec<String> = [
        "contract",
        "invoke",
        "--id",
        contract_id,
        "--network",
        network,
        "--source",
        source,
        "--",
        function,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    cmd_args.extend_from_slice(args);

    let output = Command::new("stellar")
        .args(&cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("launch stellar: {e}"))?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if !output.status.success() {
        eprintln!("warn: stellar invoke {label} failed; skipping:\n{combined}");
        return Ok(NetworkGasTx::failed(label));
    }

    Ok(NetworkGasTx {
        operation: label.to_string(),
        tx_hash: field_from_output(
            &combined,
            &["Transaction Hash", "txHash", "hash"],
            "UNKNOWN",
        ),
        fee_charged: field_from_output(&combined, &["Fee", "feeCharged"], "N/A"),
        cpu_instructions: None,
        ram_bytes: None,
        ledger_reads: None,
        ledger_writes: None,
    })
}

fn print_summary(report: &NetworkGasReport) {
    println!(
        "\n=== Network Gas Profile (Preview: {} txs) ===",
        report.transactions.len()
    );
    for tx in &report.transactions {
        println!(
            "  {:<24} fee={:<12} tx={}",
            tx.operation, tx.fee_charged, tx.tx_hash
        );
    }
    println!(
        "(Note: for full CPU/RAM metrics a Soroban RPC returning meta/sorobanResources is needed.)\n"
    );
}

/// Entry point for `cargo xtask gas-profile-network`.
pub fn run(extra: &[String]) -> Result<(), String> {
    let opts = parse_options(extra)?;
    let (network, source) = (opts.network.as_str(), opts.source.as_str());

    println!("==> gas-profile-network on network={network} source={source}");

    let contract_id = match opts.contract_id {
        Some(id) => id,
        None => deploy(network, source)?,
    };
    if contract_id.is_empty() {
        return Err("failed to obtain deployed contract id".into());
    }
    println!("    Contract ID: {contract_id}");

    let admin = account_address(network, source);
    let init_args: Vec<String> = vec![
        format!("--addr={admin}"),
        format!("--addr={admin}"),
        "--u32=0".to_string(),
    ];

    let calls: &[(&str, &[String], &str)] = &[
        ("initialize", &init_args, "initialize"),
        ("get_escrow_count", &[], "get_escrow_count (view)"),
        ("get_stats", &[], "get_stats (view)"),
        ("get_fee_config", &[], "get_fee_config (view)"),
    ];

    let mut transactions = Vec::with_capacity(calls.len());
    for (function, args, label) in calls {
        transactions.push(invoke(
            &contract_id,
            network,
            source,
            function,
            args,
            label,
        )?);
    }

    let report = NetworkGasReport {
        generated_at: now_secs(),
        network: opts.network.clone(),
        contract_id,
        transactions,
    };

    print_summary(&report);

    if let Some(path) = opts.out_path {
        write_json(&report, &path, "network report")?;
        println!("Wrote network gas report to: {path}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options_uses_documented_defaults() {
        let opts = parse_options(&[]).expect("parse");
        assert_eq!(opts.network, "standalone");
        assert_eq!(opts.source, "alice");
        assert!(opts.contract_id.is_none());
        assert!(opts.out_path.is_none());
    }

    #[test]
    fn parse_options_reads_every_flag() {
        let extra: Vec<String> = [
            "--network",
            "testnet",
            "--source",
            "bob",
            "--contract",
            "C123",
            "--out",
            "n.json",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let opts = parse_options(&extra).expect("parse");
        assert_eq!(opts.network, "testnet");
        assert_eq!(opts.source, "bob");
        assert_eq!(opts.contract_id.as_deref(), Some("C123"));
        assert_eq!(opts.out_path.as_deref(), Some("n.json"));
    }

    #[test]
    fn parse_options_rejects_unknown_arguments() {
        let err = parse_options(&["--nope".to_string()]).unwrap_err();
        assert!(err.contains("--nope"), "{err}");
    }

    #[test]
    fn parse_options_rejects_flags_missing_their_value() {
        assert!(parse_options(&["--network".to_string()]).is_err());
        assert!(parse_options(&["--contract".to_string()]).is_err());
    }

    #[test]
    fn field_from_output_finds_the_first_matching_key() {
        let out = "Simulating...\nTransaction Hash: abc123\nFee: 1000\n";
        assert_eq!(
            field_from_output(out, &["Transaction Hash", "txHash"], "UNKNOWN"),
            "abc123"
        );
        assert_eq!(field_from_output(out, &["Fee"], "N/A"), "1000");
    }

    #[test]
    fn field_from_output_falls_back_when_absent() {
        assert_eq!(field_from_output("nothing here", &["Fee"], "N/A"), "N/A");
    }

    #[test]
    fn failed_tx_is_marked_and_carries_no_metrics() {
        let tx = NetworkGasTx::failed("initialize");
        assert_eq!(tx.operation, "initialize");
        assert_eq!(tx.tx_hash, FAILED);
        assert_eq!(tx.fee_charged, FAILED);
        assert!(tx.cpu_instructions.is_none());
    }
}
