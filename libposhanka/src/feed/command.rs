use std::io;
use std::process::{Command, Output, Stdio};

use thiserror::Error;

use super::provider::ProviderSpec;

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("provider command not configured")]
    NoCommand,
    #[error("failed to spawn provider command: {0}")]
    Spawn(#[source] io::Error),
    #[error("failed to read provider command output: {0}")]
    Io(#[source] io::Error),
    #[error("provider command failed with {status}: {stderr}")]
    Status {
        status: std::process::ExitStatus,
        stderr: String,
    },
}

/// Run `[provider].command` with the given subcommand args (e.g. `["list"]`, `["close", "1"]`).
pub fn run_command(spec: &ProviderSpec, args: &[&str]) -> Result<String, CommandError> {
    let program = spec.command.as_deref().ok_or(CommandError::NoCommand)?;
    let mut cmd = Command::new(program);
    if let Some(socket) = &spec.socket {
        cmd.arg("--socket").arg(socket);
    }
    cmd.args(args);
    cmd.stdin(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.stdout(Stdio::piped());

    let output = cmd.output().map_err(CommandError::Spawn)?;
    check_output(output)
}

pub fn close(spec: &ProviderSpec, id: u32) -> Result<(), CommandError> {
    let id = id.to_string();
    run_command(spec, &["close", &id]).map(|_| ())
}

pub fn activate(spec: &ProviderSpec, id: u32, key: Option<&str>) -> Result<(), CommandError> {
    let id = id.to_string();
    match key {
        Some(k) => run_command(spec, &["activate", &id, k]).map(|_| ()),
        None => run_command(spec, &["activate", &id]).map(|_| ()),
    }
}

pub fn input(spec: &ProviderSpec, id: u32, event_kind: &str) -> Result<(), CommandError> {
    let id = id.to_string();
    run_command(spec, &["input", &id, event_kind]).map(|_| ())
}

fn check_output(output: Output) -> Result<String, CommandError> {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|e| CommandError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
            .map(|s| s.to_string())
    } else {
        Err(CommandError::Status {
            status: output.status,
            stderr,
        })
    }
}

#[cfg(test)]
mod tests;
