use crate::gas::types::{GasMetric, GasReport};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn categorize(label: &str) -> String {
    let lower = label.to_lowercase();
    if lower.contains("view") || lower.starts_with("get_") {
        "views".to_string()
    } else if lower.contains("create") || lower.starts_with("initialize") {
        "creation".to_string()
    } else if lower.contains("cancel") || lower.contains("refund") || lower.contains("drain") {
        "cancel-refund".to_string()
    } else if lower.contains("fund") || lower.contains("mark_shipped") {
        "lifecycle".to_string()
    } else if lower.contains("confirm")
        || lower.contains("release")
        || lower.contains("auto_release")
        || lower.contains("co_signed")
    {
        "completion".to_string()
    } else if lower.contains("dispute")
        || lower.contains("vote")
        || lower.contains("appeal")
        || lower.contains("finalize")
        || lower.contains("raise")
        || lower.contains("resolve")
    {
        "dispute".to_string()
    } else if lower.contains("cancel") || lower.contains("refund") || lower.contains("drain") {
        "cancel-refund".to_string()
    } else if lower.contains("admin")
        || lower.contains("fee")
        || lower.contains("pause")
        || lower.contains("set_")
        || lower.contains("rotate")
    {
        "admin-config".to_string()
    } else if lower.contains("message")
        || lower.contains("batch")
        || lower.contains("multicall")
        || lower.contains("basket")
    {
        "advanced".to_string()
    } else {
        "other".to_string()
    }
}

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

pub fn build_report(metrics: Vec<GasMetric>) -> GasReport {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_mapping() {
        assert_eq!(categorize("view_get_escrow"), "views");
        assert_eq!(categorize("get_stats"), "views");
        assert_eq!(categorize("create_escrow"), "creation");
        assert_eq!(categorize("initialize"), "creation");
        assert_eq!(categorize("fund_escrow"), "lifecycle");
        assert_eq!(categorize("mark_shipped"), "lifecycle");
        assert_eq!(categorize("confirm_delivery"), "completion");
        assert_eq!(categorize("auto_release"), "completion");
        assert_eq!(categorize("co_signed_release"), "completion");
        assert_eq!(categorize("raise_dispute"), "dispute");
        assert_eq!(categorize("resolve_dispute"), "dispute");
        assert_eq!(categorize("vote"), "dispute");
        assert_eq!(categorize("appeal_dispute"), "dispute");
        assert_eq!(categorize("finalize_dispute"), "dispute");
        assert_eq!(categorize("cancel_escrow"), "cancel-refund");
        assert_eq!(categorize("refund_buyer"), "cancel-refund");
        assert_eq!(categorize("emergency_drain"), "cancel-refund");
        assert_eq!(categorize("admin_config_setters"), "admin-config");
        assert_eq!(categorize("pause_unpause"), "admin-config");
        assert_eq!(categorize("create_basket_escrow"), "creation"); // starts with create
        assert_eq!(categorize("basket_payout"), "advanced");
        assert_eq!(categorize("multicall"), "advanced");
        assert_eq!(categorize("unknown_op"), "other");
    }

    #[test]
    fn test_parse_gas_output_valid() {
        let sample_output = r#"
running 1 test
gas_profile | initialize | cpu_insns= 18844 | mem_bytes= 1232 |
gas_profile | create_escrow | cpu_insns= 466440 | mem_bytes= 35222 |
ignored line
gas_profile | auto_release | cpu_insns= 705358 | mem_bytes= 52254 |
test test_gas_profile::gas_profile_summary ... ok
"#;
        let metrics = parse_gas_output(sample_output);
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].label, "initialize");
        assert_eq!(metrics[0].cpu_insns, 18844);
        assert_eq!(metrics[0].mem_bytes, 1232);
        assert_eq!(metrics[0].category, "creation");

        assert_eq!(metrics[1].label, "create_escrow");
        assert_eq!(metrics[1].cpu_insns, 466440);
        assert_eq!(metrics[1].mem_bytes, 35222);
        assert_eq!(metrics[1].category, "creation");

        assert_eq!(metrics[2].label, "auto_release");
        assert_eq!(metrics[2].cpu_insns, 705358);
        assert_eq!(metrics[2].mem_bytes, 52254);
        assert_eq!(metrics[2].category, "completion");
    }

    #[test]
    fn test_parse_gas_output_empty() {
        let empty_output = "running 0 tests\n";
        let metrics = parse_gas_output(empty_output);
        assert!(metrics.is_empty());
    }

    #[test]
    fn test_build_report() {
        let metrics = vec![
            GasMetric {
                label: "create_escrow".to_string(),
                cpu_insns: 100,
                mem_bytes: 50,
                category: "creation".to_string(),
            },
            GasMetric {
                label: "initialize".to_string(),
                cpu_insns: 200,
                mem_bytes: 150,
                category: "creation".to_string(),
            },
            GasMetric {
                label: "auto_release".to_string(),
                cpu_insns: 300,
                mem_bytes: 200,
                category: "completion".to_string(),
            },
        ];
        let report = build_report(metrics);
        assert_eq!(report.total_samples, 3);
        assert_eq!(report.total_cpu_insns, 600);
        assert_eq!(report.total_mem_bytes, 400);
        assert_eq!(report.by_category.len(), 2);
        assert_eq!(report.by_category.get("creation").unwrap().len(), 2);
        assert_eq!(report.by_category.get("completion").unwrap().len(), 1);
    }
}
