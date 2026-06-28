//! Developer task runner for trust-link-contract.
//!
//! Wraps the longer cargo / stellar commands documented in CONTRIBUTING.md so
//! contributors can run them by name. Invoke via the cargo alias:
//!
//! ```text
//! cargo xtask help
//! cargo xtask ci
//! cargo xtask deploy -- --network testnet --source alice
//! ```
//!
//! Extra arguments after the subcommand are forwarded to the underlying tool,
//! so `cargo xtask test -- --nocapture` works as expected.

use std::process::{Command, ExitCode};

/// A single dev command: its name, a one-line description, and a builder that
/// turns forwarded args into the program + arguments to execute.
struct Task {
    name: &'static str,
    about: &'static str,
    run: fn(&[String]) -> Command,
}

const WASM_TARGET: &str = "wasm32v1-none";

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
            name: "ci",
            about: "Run the full local CI gate: fmt-check, clippy, wasm build, and tests",
            // `ci` is handled specially in `main` because it chains several tasks.
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
        println!("    {:<width$}  {}", task.name, task.about, width = width);
    }
    println!("    {:<width$}  {}", "help", "Show this help text", width = width);
}

fn run(mut cmd: Command) -> Result<(), String> {
    let program = format!("{:?}", cmd.get_program());
    let status = cmd
        .status()
        .map_err(|e| format!("failed to launch {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
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

    // `ci` chains the individual quality gates in order.
    if command == "ci" {
        let gates = ["fmt-check", "clippy", "build-wasm", "test"];
        for gate in gates {
            let task = tasks.iter().find(|t| t.name == gate).unwrap();
            println!("\n==> cargo xtask {gate}");
            if let Err(err) = run((task.run)(&[])) {
                eprintln!("error: {err}");
                return ExitCode::FAILURE;
            }
        }
        return ExitCode::SUCCESS;
    }

    match tasks.iter().find(|t| t.name == command) {
        Some(task) => match run((task.run)(&forwarded)) {
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
