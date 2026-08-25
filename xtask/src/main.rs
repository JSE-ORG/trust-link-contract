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
//!
//! Layout:
//!   * [`tasks`]       — the subcommand table, help text and the CI gate
//!   * [`process`]     — spawning cargo/stellar/npm/bash and capturing output
//!   * [`gas`]         — local gas profiling and report rendering
//!   * [`gas_network`] — gas profiling against a live Stellar network

mod gas;
mod gas_network;
mod process;
mod tasks;

use std::process::ExitCode;

fn main() -> ExitCode {
    let registry = tasks::all();

    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        tasks::print_help(&registry);
        return ExitCode::SUCCESS;
    };
    let forwarded: Vec<String> = args.collect();

    let name = match command.as_str() {
        "--help" | "-h" => "help",
        other => other,
    };

    let Some(task) = tasks::find(&registry, name) else {
        eprintln!("error: unknown command '{command}'\n");
        tasks::print_help(&registry);
        return ExitCode::FAILURE;
    };

    match task.run(&forwarded) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {name} failed: {err}");
            ExitCode::FAILURE
        }
    }
}
