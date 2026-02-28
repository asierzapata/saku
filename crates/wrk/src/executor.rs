use std::path::Path;
use std::time::Duration;

use thiserror::Error;

/// Default timeout for a single task execution (30 minutes).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug)]
pub struct ExecutionResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration: Duration,
}

#[derive(Debug, Error)]
pub enum ExecutorError {
    #[error("Failed to spawn claude process: {0}")]
    SpawnFailed(#[source] std::io::Error),

    #[error("Execution timed out after {0:?}")]
    Timeout(Duration),

    #[error("Failed to read process output: {0}")]
    OutputFailed(#[source] std::io::Error),
}

/// Execute a prompt via `claude --print -p <prompt>`.
///
/// Spawns a subprocess, captures stdout/stderr, and returns the result.
pub async fn execute_task(
    prompt: &str,
    working_dir: &Path,
    timeout: Option<Duration>,
) -> Result<ExecutionResult, ExecutorError> {
    let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
    let start = std::time::Instant::now();

    let child = tokio::process::Command::new("claude")
        .args(["--print", "-p", prompt])
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(ExecutorError::SpawnFailed)?;

    let result = tokio::time::timeout(timeout, child.wait_with_output()).await;

    let duration = start.elapsed();

    match result {
        Ok(Ok(output)) => Ok(ExecutionResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
            duration,
        }),
        Ok(Err(e)) => Err(ExecutorError::OutputFailed(e)),
        Err(_) => {
            // Timeout — child process already dropped when wait_with_output was cancelled
            Err(ExecutorError::Timeout(timeout))
        }
    }
}
