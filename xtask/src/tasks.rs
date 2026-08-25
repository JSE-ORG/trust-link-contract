//! The subcommand table and help text.
//!
//! Adding a command means adding one [`Task`] entry here — dispatch, help
//! output and error handling all read from this table, so `main` never needs
//! per-command branches.

use std::process::Command;

use crate::gas;
use crate::gas_network;
use crate::process::{bash, cargo, npm, run_streaming, stellar};

const WASM_TARGET: &str = "wasm32v1-none";
const WASM_PATH: &str = "target/wasm32v1-none/release/trustlink_escrow.wasm";

/// Gates run, in order, by `cargo xtask ci`.
const CI_GATES: &[&str] = &["fmt-check", "clippy", "build-wasm", "test"];

/// How a task does its work.
pub enum Action {
    /// Build an external command and run it, streaming its output.
    Spawn(fn(&[String]) -> Command),
    /// Handle the command in-process.
    Custom(fn(&[String]) -> Result<(), String>),
}

/// One `cargo xtask <name>` subcommand.
pub struct Task {
    pub name: &'static str,
    pub about: &'static str,
    pub action: Action,
}

impl Task {
    /// Run the task with any arguments the caller passed after `--`.
    pub fn run(&self, extra: &[String]) -> Result<(), String> {
        match self.action {
            Action::Spawn(build) => run_streaming(build(extra)),
            Action::Custom(handler) => handler(extra),
        }
    }
}

/// Every subcommand, in the order they appear in `--help`.
pub fn all() -> Vec<Task> {
    vec![
        Task {
            name: "build",
            about: "Build the whole workspace in release mode",
            action: Action::Spawn(|e| cargo(&["build", "--workspace", "--release"], e)),
        },
        Task {
            name: "build-wasm",
            about: "Build the deployable wasm artifact (wasm32v1-none target)",
            action: Action::Spawn(|e| {
                cargo(
                    &["build", "--workspace", "--release", "--target", WASM_TARGET],
                    e,
                )
            }),
        },
        Task {
            name: "test",
            about: "Run the full workspace test suite",
            action: Action::Spawn(|e| cargo(&["test", "--workspace"], e)),
        },
        Task {
            name: "fmt",
            about: "Format all crates with rustfmt",
            action: Action::Spawn(|e| cargo(&["fmt", "--all"], e)),
        },
        Task {
            name: "fmt-check",
            about: "Check formatting without writing changes",
            action: Action::Spawn(|e| cargo(&["fmt", "--all", "--check"], e)),
        },
        Task {
            name: "clippy",
            about: "Lint the workspace, denying warnings",
            action: Action::Spawn(|e| cargo(&["clippy", "--workspace", "--", "-D", "warnings"], e)),
        },
        Task {
            name: "optimize",
            about: "Build an optimized wasm via build.sh (requires wasm-opt)",
            action: Action::Spawn(|e| bash(&["build.sh"], e)),
        },
        Task {
            name: "bindings",
            about: "Generate the TypeScript bindings (npm run build in bindings/)",
            action: Action::Spawn(|e| npm(&["run", "build", "--prefix", "bindings"], e)),
        },
        Task {
            name: "deploy",
            about: "Deploy the contract via the Stellar CLI (pass --network/--source after --)",
            action: Action::Spawn(|e| stellar(&["contract", "deploy", "--wasm", WASM_PATH], e)),
        },
        Task {
            name: "gas-profile",
            about: "Run gas-profile tests, print a console summary, and (optionally) write JSON",
            action: Action::Custom(gas::run),
        },
        Task {
            name: "gas-profile-network",
            about: "Profile gas on a live/standalone network via stellar CLI (--network/--source/--contract after --)",
            action: Action::Custom(gas_network::run),
        },
        Task {
            name: "ci",
            about: "Run the full local CI gate: fmt-check, clippy, wasm build, and tests",
            action: Action::Custom(run_ci),
        },
        Task {
            name: "help",
            about: "Show this help text",
            action: Action::Custom(|_| {
                print_help(&all());
                Ok(())
            }),
        },
    ]
}

/// Look up a subcommand by name.
pub fn find<'a>(tasks: &'a [Task], name: &str) -> Option<&'a Task> {
    tasks.iter().find(|t| t.name == name)
}

/// Run the local CI gate: each gate in turn, stopping at the first failure.
fn run_ci(_extra: &[String]) -> Result<(), String> {
    let tasks = all();
    for gate in CI_GATES {
        let task =
            find(&tasks, gate).ok_or_else(|| format!("ci gate '{gate}' is not a known task"))?;
        println!("\n==> cargo xtask {gate}");
        task.run(&[])?;
    }
    Ok(())
}

pub fn print_help(tasks: &[Task]) {
    println!("cargo xtask — developer task runner for trust-link-contract\n");
    println!("Usage:");
    println!("    cargo xtask <command> [-- <args forwarded to the tool>]\n");
    println!("Commands:");

    let width = tasks.iter().map(|t| t.name.len()).max().unwrap_or(0);
    for task in tasks {
        println!("    {:<width$}  {}", task.name, task.about, width = width);
    }

    println!(
        "\ngas-profile options (after --):\n    --out <file>    Write JSON report to <file>\n    --category <cat> Only show metrics whose category starts with <cat>\n    --no-table       Suppress the console table (useful with --out)\n"
    );
    println!(
        "gas-profile-network options (after --):\n    --network <net>   Stellar network (default: standalone)\n    --source <acc>    Stellar CLI account name (default: alice)\n    --contract <id>   Existing deployed contract (if omitted, deploys a new one)\n    --out <file>      Write JSON report to <file>\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn task_names_are_unique() {
        let tasks = all();
        let mut seen = HashSet::new();
        for task in &tasks {
            assert!(seen.insert(task.name), "duplicate task name: {}", task.name);
        }
    }

    #[test]
    fn every_documented_command_is_still_registered() {
        // Backward compatibility: these names are documented in CONTRIBUTING.md
        // and used by contributors' muscle memory, so none may disappear.
        let tasks = all();
        for name in [
            "build",
            "build-wasm",
            "test",
            "fmt",
            "fmt-check",
            "clippy",
            "optimize",
            "bindings",
            "deploy",
            "gas-profile",
            "gas-profile-network",
            "ci",
            "help",
        ] {
            assert!(find(&tasks, name).is_some(), "missing task: {name}");
        }
    }

    #[test]
    fn every_task_has_a_description() {
        for task in &all() {
            assert!(!task.about.is_empty(), "{} has no description", task.name);
        }
    }

    #[test]
    fn unknown_task_lookup_returns_none() {
        assert!(find(&all(), "does-not-exist").is_none());
    }

    #[test]
    fn ci_gates_all_resolve_to_spawnable_tasks() {
        let tasks = all();
        for gate in CI_GATES {
            let task = find(&tasks, gate).unwrap_or_else(|| panic!("missing ci gate: {gate}"));
            assert!(
                matches!(task.action, Action::Spawn(_)),
                "ci gate {gate} must be a plain command"
            );
        }
    }
}
