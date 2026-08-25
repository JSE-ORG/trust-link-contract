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

    if command == "gas-profile-network" {
        match run_gas_profile_network(&forwarded) {
            Ok(()) => return ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: gas-profile-network failed: {err}");
                return ExitCode::FAILURE;
            }
        }
    }

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
            ExitCode::FAILURE
        }
    }
}
