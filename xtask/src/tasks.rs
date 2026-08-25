use std::process::{Command, Stdio};

pub const WASM_TARGET: &str = "wasm32v1-none";

pub struct Task {
    pub name: &'static str,
    pub about: &'static str,
    pub run: fn(&[String]) -> Command,
}

pub fn cargo(args: &[&str], extra: &[String]) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    cmd.args(extra);
    cmd
}

pub fn stellar(args: &[&str], extra: &[String]) -> Command {
    let mut cmd = Command::new("stellar");
    cmd.args(args);
    cmd.args(extra);
    cmd
}

pub fn run(mut cmd: Command) -> Result<String, String> {
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

pub fn tasks() -> Vec<Task> {
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

pub fn print_help(task_list: &[Task]) {
    println!("cargo xtask — developer task runner for trust-link-contract\n");
    println!("Usage:");
    println!("    cargo xtask <command> [-- <args forwarded to the tool>]\n");
    println!("Commands:");
    let width = task_list.iter().map(|t| t.name.len()).max().unwrap_or(0);
    for task in task_list {
        println!(
            "    {:<width$}  {}",
            task.name,
            task.about,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tasks_list_contains_expected_commands() {
        let list = tasks();
        let names: Vec<&str> = list.iter().map(|t| t.name).collect();
        assert!(names.contains(&"build"));
        assert!(names.contains(&"build-wasm"));
        assert!(names.contains(&"test"));
        assert!(names.contains(&"fmt"));
        assert!(names.contains(&"fmt-check"));
        assert!(names.contains(&"clippy"));
        assert!(names.contains(&"optimize"));
        assert!(names.contains(&"bindings"));
        assert!(names.contains(&"deploy"));
        assert!(names.contains(&"gas-profile"));
        assert!(names.contains(&"gas-profile-network"));
        assert!(names.contains(&"ci"));
    }

    #[test]
    fn test_cargo_command_building() {
        let cmd = cargo(&["build", "--release"], &["--extra".to_string()]);
        assert_eq!(cmd.get_program(), "cargo");
    }

    #[test]
    fn test_stellar_command_building() {
        let cmd = stellar(&["contract", "deploy"], &["--network".to_string()]);
        assert_eq!(cmd.get_program(), "stellar");
    }
}
