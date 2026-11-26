//! Thin wrappers around the external tools the task runner drives.
//!
//! Every subcommand ultimately shells out to `cargo`, `stellar`, `npm` or
//! `bash`. Centralising the spawn/wait plumbing here keeps the task table in
//! [`crate::tasks`] declarative and gives every caller identical error text.

use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// `cargo <args...> <extra...>`
pub fn cargo(args: &[&str], extra: &[String]) -> Command {
    build(Command::new("cargo"), args, extra)
}

/// `stellar <args...> <extra...>`
pub fn stellar(args: &[&str], extra: &[String]) -> Command {
    build(Command::new("stellar"), args, extra)
}

/// `npm <args...> <extra...>`
pub fn npm(args: &[&str], extra: &[String]) -> Command {
    build(Command::new("npm"), args, extra)
}

/// `bash <args...> <extra...>`
pub fn bash(args: &[&str], extra: &[String]) -> Command {
    build(Command::new("bash"), args, extra)
}

fn build(mut cmd: Command, args: &[&str], extra: &[String]) -> Command {
    cmd.args(args);
    cmd.args(extra);
    cmd
}

/// Human-readable program name, used in error messages.
fn program_of(cmd: &Command) -> String {
    format!("{:?}", cmd.get_program())
}

/// Run `cmd`, letting it write straight through to this process's streams.
///
/// Used for the build/test/lint tasks: contributors want cargo's progress and
/// test output live rather than buffered until the process exits.
pub fn run_streaming(mut cmd: Command) -> Result<(), String> {
    let program = program_of(&cmd);
    let status = cmd
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to launch {program}: {e}"))?;
    if !status.success() {
        return Err(format!("{program} exited with {status}"));
    }
    Ok(())
}

/// Run `cmd` and capture its stdout; stderr still streams to the terminal.
///
/// Used where the output has to be parsed (gas metrics, deployed contract ids).
pub fn run_capture(mut cmd: Command) -> Result<String, String> {
    let program = program_of(&cmd);
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

/// Current unix timestamp in seconds, or 0 if the clock predates the epoch.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
