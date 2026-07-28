//! Developer task runner for trust-link-contract.
//!
//! Wraps the longer cargo / stellar commands documented in CONTRIBUTING.md so
//! contributors can run them by name. Invoke via the cargo alias:
//!
//! ```text
//! cargo xtask help
//! cargo xtask ci
//! cargo xtask gas-profile
//! cargo xtask gas-profile -- --out gas-report.json
//! cargo xtask deploy -- --network testnet --source alice
//! ```
//!
//! Extra arguments after the subcommand are forwarded to the underlying tool,
//! so `cargo xtask test -- --nocapture` works as expected.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::process::{Command, ExitCode, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GasMetric {
    label: String,
    cpu_insns: u64,
    mem_bytes: u64,
    category: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct GasReport {
    generated_at: u64,
    version: String,
    total_samples: usize,
    total_cpu_insns: u64,
    total_mem_bytes: u64,
    metrics: Vec<GasMetric>,
    by_category: BTreeMap<String, Vec<GasMetric>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NetworkGasTx {
    operation: String,
    tx_hash: String,
    fee_charged: String,
    cpu_instructions: Option<u64>,
    ram_bytes: Option<u64>,
    ledger_reads: Option<u64>,
    ledger_writes: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct NetworkGasReport {
    generated_at: u64,
    network: String,
    contract_id: String,
    transactions: Vec<NetworkGasTx>,
}

struct Task {
    name: &'static str,
    about: &'static str,
    run: fn(&[String]) -> Command,
}

const WASM_TARGET: &str = "wasm32v1-none";
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

fn cargo(args: &[&str], extra: &[String]) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    cmd.args(extra);
    cmd
}

fn stellar(args: &[&str], extra: &[String]) -> Command {
    let mut cmd = Command::new("stellar");
    cmd.args(args);
    cmd.args(extra);
    cmd
}

fn tasks() -> Vec<Task> {
    vec![
        Task {
            name: "build",
            about: "Build the whole workspace in release mode",
            run: |e| cargo(&["build", "--workspace", "--release"], e),
        },
        Task {
            name: "build-wasm",
            about: "Build the deployable wasm artifact (wasm32v1-none target)",
            run: |e| {
                cargo(
                    &["build", "--workspace", "--release", "--target", WASM_TARGET],
                    e,
                )
            },
        },
        Task {
            name: "test",
            about: "Run the full workspace test suite",
            run: |e| cargo(&["test", "--workspace"], e),
        },
        Task {
            name: "fmt",
            about: "Format all crates with rustfmt",
            run: |e| cargo(&["fmt", "--all"], e),
        },
        Task {
            name: "fmt-check",
            about: "Check formatting without writing changes",
            run: |e| cargo(&["fmt", "--all", "--check"], e),
        },
        Task {
            name: "clippy",
            about: "Lint the workspace, denying warnings",
            run: |e| cargo(&["clippy", "--workspace", "--", "-D", "warnings"], e),
        },
        Task {
            name: "optimize",
            about: "Build an optimized wasm via build.sh (requires wasm-opt)",
            run: |e| {
                let mut cmd = Command::new("bash");
                cmd.arg("build.sh");
                cmd.args(e);
                cmd
            },
        },
        Task {
            name: "bindings",
            about: "Generate the TypeScript bindings (npm run build in bindings/)",
            run: |e| {
                let mut cmd = Command::new("npm");
                cmd.args(["run", "build", "--prefix", "bindings"]);
                cmd.args(e);
                cmd
            },
        },
        Task {
            name: "deploy",
            about: "Deploy the contract via the Stellar CLI (pass --network/--source after --)",
            run: |e| {
                stellar(
                    &[
                        "contract",
                        "deploy",
                        "--wasm",
                        "target/wasm32v1-none/release/trustlink_escrow.wasm",
                    ],
                    e,
                )
            },
        },
        Task {
            name: "gas-profile",
            about: "Run gas-profile tests, print a console summary, and (optionally) write JSON",
            run: |_| Command::new("cargo"),
        },
        Task {
            name: "gas-profile-network",
            about: "Profile gas on a live/standalone network via stellar CLI (--network/--source/--contract after --)",
            run: |_| Command::new("cargo"),
        },
        Task {
            name: "ci",
            about: "Run the full local CI gate: fmt-check, clippy, wasm build, and tests",
            run: |_| Command::new("cargo"),
        },
    ]
}

fn print_help(tasks: &[Task]) {
    println!("cargo xtask — developer task runner for trust-link-contract\n");
    println!("Usage:");
    println!("    cargo xtask <command> [-- <args forwarded to the tool>]\n");
    println!("Commands:");
    let width = tasks.iter().map(|t| t.name.len()).max().unwrap_or(0);
    for task in tasks {
        println!(
            "    {:<width$}  {}",
            task.name, task.about,
            width = width
        );
    }
    println!(
        "\ngas-profile options (after --):\n    --out <file>    Write JSON report to <file>\n    --category <cat> Only show metrics whose category starts with <cat>\n    --no-table       Suppress the console table (useful with --out)\n"
    );
    println!(
        "gas-profile-network options (after --):\n    --network <net>   Stellar network (default: standalone)\n    --source <acc>    Stellar CLI account name (default: alice)\n    --contract <id>   Existing deployed contract (if omitted, deploys a new one)\n    --out <file>      Write JSON report to <file>\n"
    );
    println!("    {:<width$}  {}", "help", "Show this help text", width = width);
}

fn run(mut cmd: Command) -> Result<String, String> {
    let program = format!("{:?}", cmd.get_program());
    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("failed to launch {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("non-utf8 stdout from {program}: {e}"))
}

fn categorize(label: &str) -> String {
    let lower = label.to_lowercase();
    if lower.contains("view") || lower.starts_with("get_") {
        "views".to_string()
    } else if lower.contains("create") || lower.starts_with("initialize") {
        "creation".to_string()
    } else if lower.contains("fund") || lower.contains("mark_shipped") {
        "lifecycle".to_string()
    } else if lower.contains("confirm") || lower.contains("release") || lower.contains("auto_release") || lower.contains("co_signed") {
        "completion".to_string()
    } else if lower.contains("dispute") || lower.contains("vote") || lower.contains("appeal") || lower.contains("finalize") || lower.contains("raise") || lower.contains("resolve") {
        "dispute".to_string()
    } else if lower.contains("cancel") || lower.contains("refund") || lower.contains("drain") {
        "cancel-refund".to_string()
    } else if lower.contains("admin") || lower.contains("fee") || lower.contains("pause") || lower.contains("set_") || lower.contains("rotate") {
        "admin-config".to_string()
    } else if lower.contains("message") || lower.contains("batch") || lower.contains("multicall") || lower.contains("basket") {
        "advanced".to_string()
    } else {
        "other".to_string()
    }
}

fn parse_gas_output(stdout: &str) -> Vec<GasMetric> {
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
        let label = parts[1].trim().to_string();
        let cpu_part = parts[2].trim();
        let mem_part = parts[3].trim();
        let parse_kv = |s: &str| -> Option<u64> {
            let tokens: Vec<&str> = s.split('=').collect();
            tokens.get(1).and_then(|v| v.trim().parse::<u64>().ok())
        };
        let cpu = match parse_kv(cpu_part) {
            Some(v) => v,
            None => continue,
        };
        let mem = match parse_kv(mem_part) {
            Some(v) => v,
            None => continue,
        };
        out.push(GasMetric {
            category: categorize(&label),
            label,
            cpu_insns: cpu,
            mem_bytes: mem,
        });
    }
    out
}

fn print_table(report: &GasReport, category_filter: Option<&str>) {
    println!("\n=== Gas Profile Report ===");
    println!("Generated: {} | Samples: {} | Total CPU: {} | Total Mem: {}",
             report.generated_at, report.total_samples,
             report.total_cpu_insns, report.total_mem_bytes);
    println!();
    let headers = ["Operation", "CPU Insns", "Mem Bytes", "Category"];
    let widths = [42usize, 14, 12, 16];
    println!(
        "{:<w0$} | {:>w1$} | {:>w2$} | {:<w3$}",
        headers[0], headers[1], headers[2], headers[3],
        w0 = widths[0], w1 = widths[1], w2 = widths[2], w3 = widths[3]
    );
    println!("{0:-<w0$}-+-{0:-<w1$}-+-{0:-<w2$}-+-{0:-<w3$}", "",
             w0 = widths[0], w1 = widths[1], w2 = widths[2], w3 = widths[3]);
    for m in &report.metrics {
        if let Some(f) = category_filter {
            if !m.category.starts_with(f) {
                continue;
            }
        }
        println!(
            "{:<w0$} | {:>w1$} | {:>w2$} | {:<w3$}",
            m.label, m.cpu_insns, m.mem_bytes, m.category,
            w0 = widths[0], w1 = widths[1], w2 = widths[2], w3 = widths[3]
        );
    }
    println!();
    for (cat, ms) in &report.by_category {
        if ms.is_empty() {
            continue;
        }
        if let Some(f) = category_filter {
            if !cat.starts_with(f) {
                continue;
            }
        }
        let sum_cpu: u64 = ms.iter().map(|m| m.cpu_insns).sum();
        let sum_mem: u64 = ms.iter().map(|m| m.mem_bytes).sum();
        let avg_cpu = if ms.is_empty() { 0 } else { sum_cpu / ms.len() as u64 };
        let avg_mem = if ms.is_empty() { 0 } else { sum_mem / ms.len() as u64 };
        println!(
            "Category [{:<12}] n={:<2} sum_cpu={:>12} avg_cpu={:>10} sum_mem={:>10} avg_mem={:>8}",
            cat, ms.len(), sum_cpu, avg_cpu, sum_mem, avg_mem
        );
    }
    println!("=== End Gas Report ===\n");
}

fn build_report(metrics: Vec<GasMetric>) -> GasReport {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let version = env!("CARGO_PKG_VERSION").to_string();
    let total_cpu: u64 = metrics.iter().map(|m| m.cpu_insns).sum();
    let total_mem: u64 = metrics.iter().map(|m| m.mem_bytes).sum();
    let mut by_category: BTreeMap<String, Vec<GasMetric>> = BTreeMap::new();
    for m in metrics.iter() {
        by_category
            .entry(m.category.clone())
            .or_default()
            .push(m.clone());
    }
    GasReport {
        generated_at: now,
        version,
        total_samples: metrics.len(),
        total_cpu_insns: total_cpu,
        total_mem_bytes: total_mem,
        metrics,
        by_category,
    }
}

fn run_gas_profile(extra: &[String]) -> Result<(), String> {
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

fn run_gas_profile_network(extra: &[String]) -> Result<(), String> {
    let mut network = "standalone".to_string();
    let mut source = "alice".to_string();
    let mut contract_id: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut it = extra.iter().cloned();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--network" => network = it.next().ok_or("--network requires a value")?,
            "--source" => source = it.next().ok_or("--source requires a value")?,
            "--contract" => contract_id = Some(it.next().ok_or("--contract requires a value")?),
            "--out" => out_path = Some(it.next().ok_or("--out requires a path")?),
            other => return Err(format!("unknown gas-profile-network argument: {other}")),
        }
    }

    println!("==> gas-profile-network on network={network} source={source}");

    let cid = match contract_id {
        Some(c) => c,
        None => {
            println!("    Deploying fresh contract instance...");
            let stdout = run(stellar(
                &[
                    "contract",
                    "deploy",
                    "--wasm",
                    "target/wasm32v1-none/release/trustlink_escrow.wasm",
                    "--network",
                    &network,
                    "--source",
                    &source,
                ],
                &[],
            ))?;
            stdout.trim().split('\n').last().unwrap_or("").trim().to_string()
        }
    };
    if cid.is_empty() {
        return Err("failed to obtain deployed contract id".into());
    }
    println!("    Contract ID: {cid}");

    let stellar_invoke = |func: &str, json_args: &[&str], label: &str| -> Result<NetworkGasTx, String> {
        let mut cmd_args = vec![
            "contract".to_string(),
            "invoke".to_string(),
            "--id".to_string(),
            cid.clone(),
            "--network".to_string(),
            network.clone(),
            "--source".to_string(),
            source.clone(),
            "--".to_string(),
            func.to_string(),
        ];
        for a in json_args {
            cmd_args.push((*a).to_string());
        }
        let output = Command::new("stellar")
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("launch stellar: {e}"))?;
        let status_ok = output.status.success();
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !status_ok {
            eprintln!("warn: stellar invoke {label} failed; skipping:\n{combined}");
            return Ok(NetworkGasTx {
                operation: label.to_string(),
                tx_hash: "ERROR".to_string(),
                fee_charged: "ERROR".to_string(),
                cpu_instructions: None,
                ram_bytes: None,
                ledger_reads: None,
                ledger_writes: None,
            });
        }
        let tx_hash = combined
            .lines()
            .find(|l| l.contains("Transaction Hash") || l.contains("txHash") || l.contains("hash"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "UNKNOWN".to_string());
        let fee = combined
            .lines()
            .find(|l| l.contains("Fee") || l.contains("feeCharged"))
            .and_then(|l| l.split(':').nth(1))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "N/A".to_string());
        Ok(NetworkGasTx {
            operation: label.to_string(),
            tx_hash,
            fee_charged: fee,
            cpu_instructions: None,
            ram_bytes: None,
            ledger_reads: None,
            ledger_writes: None,
        })
    };

    let mut txs = Vec::new();

    let admin_addr_output = run(stellar(&[
        "keys", "address", "--account", &source, "--network", &network,
    ], &[])).unwrap_or_else(|_| source.clone());
    let admin = admin_addr_output.lines().last().unwrap_or(&source).trim().to_string();

    let init_args = &[
        &format!("--addr={admin}") as &str,
        &format!("--addr={admin}"),
        "--u32=0",
    ];
    txs.push(stellar_invoke("initialize", init_args, "initialize")?);
    txs.push(stellar_invoke(
        "get_escrow_count",
        &[],
        "get_escrow_count (view)",
    )?);
    txs.push(stellar_invoke("get_stats", &[], "get_stats (view)")?);
    txs.push(stellar_invoke("get_fee_config", &[], "get_fee_config (view)")?);

    let now_report = NetworkGasReport {
        generated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        network,
        contract_id: cid,
        transactions: txs.clone(),
    };

    println!("\n=== Network Gas Profile (Preview: {} txs) ===", now_report.transactions.len());
    for tx in &now_report.transactions {
        println!(
            "  {:<24} fee={:<12} tx={}",
            tx.operation, tx.fee_charged, tx.tx_hash
        );
    }
    println!("(Note: for full CPU/RAM metrics a Soroban RPC returning meta/sorobanResources is needed.)\n");

    if let Some(p) = out_path {
        let json = serde_json::to_string_pretty(&now_report)
            .map_err(|e| format!("serialize network report: {e}"))?;
        fs::write(&p, &json).map_err(|e| format!("write {p}: {e}"))?;
        println!("Wrote network gas report to: {p}");
    }
    Ok(())
}

fn main() -> ExitCode {
    let tasks = tasks();
    let mut args = std::env::args().skip(1);
    let command = match args.next() {
        Some(c) => c,
        None => {
            print_help(&tasks);
            return ExitCode::SUCCESS;
        }
    };
    let forwarded: Vec<String> = args.collect();

    if command == "help" || command == "--help" || command == "-h" {
        print_help(&tasks);
        return ExitCode::SUCCESS;
    }

    if command == "ci" {
        let gates = ["fmt-check", "clippy", "build-wasm", "test"];
        for gate in gates {
            let task = tasks.iter().find(|t| t.name == gate).unwrap();
            println!("\n==> cargo xtask {gate}");
            let mut cmd = (task.run)(&[]);
            if let Err(err) = run(cmd).map(|_| ()) {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    if command == "gas-profile" {
        match run_gas_profile(&forwarded) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: gas-profile failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    if command == "gas-profile-network" {
        match run_gas_profile_network(&forwarded) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: gas-profile-network failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

    match tasks.iter().find(|t| t.name == command) {
        Some(task) => match run((task.run)(&forwarded)).map(|_| ()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        },
        None => {
            eprintln!("error: unknown command '{command}'\n");
            print_help(&tasks);
            ExitCode::FAILURE
        }
    }
}
