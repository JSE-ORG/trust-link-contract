use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GasMetric {
    pub label: String,
    pub cpu_insns: u64,
    pub mem_bytes: u64,
    pub category: String,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkGasReport {
    pub generated_at: u64,
    pub network: String,
    pub contract_id: String,
    pub transactions: Vec<NetworkGasTx>,
}
