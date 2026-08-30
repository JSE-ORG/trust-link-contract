//! Developer task runner for trust-link-contract (Escrow CLI Tool).
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
//! # Examples
//!
//! Deploy the contract to a local standalone network:
//! ```bash
//! cargo xtask deploy -- --network standalone --source default
//! ```
//!
//! Run local CI checks:
//! ```bash
//! cargo xtask ci
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
mod tasks;

use gas::{run_gas_profile, run_gas_profile_network};
use std::process::ExitCode;
use tasks::{print_help, run, tasks};

fn main() -> ExitCode {
    let task_list = tasks();
    let mut args = std::env::args().skip(1);
    let command = match args.next() {
        Some(c) => c,
        None => {
            print_help(&task_list);
            return ExitCode::SUCCESS;
        }
    };
    let forwarded: Vec<String> = args.collect();

    if command == "help" || command == "--help" || command == "-h" {
        print_help(&task_list);
        return ExitCode::SUCCESS;
    }

    if command == "ci" {
        let gates = ["fmt-check", "clippy", "build-wasm", "test"];
        for gate in gates {
            let task = task_list.iter().find(|t| t.name == gate).unwrap();
            println!("\n==> cargo xtask {gate}");
            let cmd = (task.run)(&[]);
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

    match task_list.iter().find(|t| t.name == command) {
        Some(task) => match run((task.run)(&forwarded)).map(|_| ()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err}");
                ExitCode::FAILURE
            }
        },
        None => {
            eprintln!("error: unknown command '{command}'\n");
            print_help(&task_list);
    match task.run(&forwarded) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {name} failed: {err}");
            ExitCode::FAILURE
        }
    }
}
