use crate::registry::ToolError;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

const SUBPROCESS_TIMEOUT_SECS: u64 = 30;

/// Bridge to Python scripts via subprocess. Communicates via JSON over stdin/stdout.
/// All inputs passed as JSON stdin — never shell-interpolated (security: prevents injection).
pub struct PythonBridge {
    python_dir: String,
}

impl PythonBridge {
    pub fn new(python_dir: &str) -> Self {
        Self {
            python_dir: python_dir.to_string(),
        }
    }

    /// Execute a Python script with JSON input on stdin, return JSON output from stdout.
    pub fn call(
        &self,
        script: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value, ToolError> {
        let script_path = format!("{}/{}", self.python_dir, script);

        let mut child = Command::new("python3")
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to spawn Python: {e}")))?;

        // Write JSON input to stdin
        if let Some(mut stdin) = child.stdin.take() {
            let json_bytes = serde_json::to_vec(input)
                .map_err(|e| ToolError::ExecutionFailed(format!("JSON serialization: {e}")))?;
            stdin
                .write_all(&json_bytes)
                .map_err(|e| ToolError::ExecutionFailed(format!("Write to stdin: {e}")))?;
        }

        // Wait with timeout
        let output = wait_with_timeout(child, Duration::from_secs(SUBPROCESS_TIMEOUT_SECS))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::SubprocessCrash(stderr.to_string()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout)
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to parse Python output: {e}")))
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<std::process::Output, ToolError> {
    let start = std::time::Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child
                    .wait_with_output()
                    .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
                return Ok(output);
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    return Err(ToolError::SubprocessTimeout(timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(ToolError::ExecutionFailed(e.to_string())),
        }
    }
}
