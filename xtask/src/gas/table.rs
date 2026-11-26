use crate::gas::types::GasReport;

pub fn print_table(report: &GasReport, category_filter: Option<&str>) {
    println!("\n=== Gas Profile Report ===");
    println!(
        "Generated: {} | Samples: {} | Total CPU: {} | Total Mem: {}",
        report.generated_at, report.total_samples, report.total_cpu_insns, report.total_mem_bytes
    );
    println!();
    let headers = ["Operation", "CPU Insns", "Mem Bytes", "Category"];
    let widths = [42usize, 14, 12, 16];
    println!(
        "{:<w0$} | {:>w1$} | {:>w2$} | {:<w3$}",
        headers[0],
        headers[1],
        headers[2],
        headers[3],
        w0 = widths[0],
        w1 = widths[1],
        w2 = widths[2],
        w3 = widths[3]
    );
    println!(
        "{0:-<w0$}-+-{0:-<w1$}-+-{0:-<w2$}-+-{0:-<w3$}",
        "",
        w0 = widths[0],
        w1 = widths[1],
        w2 = widths[2],
        w3 = widths[3]
    );
    for m in &report.metrics {
        if let Some(f) = category_filter {
            if !m.category.starts_with(f) {
                continue;
            }
        }
        println!(
            "{:<w0$} | {:>w1$} | {:>w2$} | {:<w3$}",
            m.label,
            m.cpu_insns,
            m.mem_bytes,
            m.category,
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2],
            w3 = widths[3]
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
        let avg_cpu = if ms.is_empty() {
            0
        } else {
            sum_cpu / ms.len() as u64
        };
        let avg_mem = if ms.is_empty() {
            0
        } else {
            sum_mem / ms.len() as u64
        };
        println!(
            "Category [{:<12}] n={:<2} sum_cpu={:>12} avg_cpu={:>10} sum_mem={:>10} avg_mem={:>8}",
            cat,
            ms.len(),
            sum_cpu,
            avg_cpu,
            sum_mem,
            avg_mem
        );
    }
    println!("=== End Gas Report ===\n");
}
