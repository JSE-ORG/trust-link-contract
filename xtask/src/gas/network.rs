use crate::gas::types::{NetworkGasReport, NetworkGasTx};
use crate::tasks::{run, stellar};
use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn run_gas_profile_network(extra: &[String]) -> Result<(), String> {
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
            stdout
                .trim()
                .split('\n')
                .last()
                .unwrap_or("")
                .trim()
                .to_string()
        }
    };
    if cid.is_empty() {
        return Err("failed to obtain deployed contract id".into());
    }
    println!("    Contract ID: {cid}");

    let stellar_invoke =
        |func: &str, json_args: &[&str], label: &str| -> Result<NetworkGasTx, String> {
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
                .find(|l| {
                    l.contains("Transaction Hash") || l.contains("txHash") || l.contains("hash")
                })
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

    let admin_addr_output = run(stellar(
        &[
            "keys",
            "address",
            "--account",
            &source,
            "--network",
            &network,
        ],
        &[],
    ))
    .unwrap_or_else(|_| source.clone());
    let admin = admin_addr_output
        .lines()
        .last()
        .unwrap_or(&source)
        .trim()
        .to_string();

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

    println!(
        "\n=== Network Gas Profile (Preview: {} txs) ===",
        now_report.transactions.len()
    );
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
