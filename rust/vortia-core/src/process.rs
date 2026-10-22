use crate::error::{ErrorCode, Result, VortiaError};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ProcessRequest {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub environment: Vec<(String, String)>,
    pub timeout: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub started: bool,
    pub exit_code: i32,
    pub timed_out: bool,
    pub cancelled: bool,
    pub duration: Duration,
    pub stdout: String,
    pub stderr: String,
}

impl ProcessResult {
    pub fn succeeded(&self) -> bool {
        self.started && !self.timed_out && !self.cancelled && self.exit_code == 0
    }
}

/// Run a process without a shell. Arguments are passed verbatim.
pub fn run(request: &ProcessRequest) -> Result<ProcessResult> {
    if request.executable.is_empty() {
        return Err(VortiaError::new(
            ErrorCode::InvalidArgument,
            "process executable is empty",
            "",
        ));
    }
    let start = Instant::now();
    let mut cmd = Command::new(&request.executable);
    cmd.args(&request.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(wd) = &request.working_directory {
        cmd.current_dir(wd);
    }
    for (k, v) in &request.environment {
        cmd.env(k, v);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(ProcessResult {
                started: false,
                exit_code: -1,
                timed_out: false,
                cancelled: false,
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: e.to_string(),
            })
        }
    };

    // Wait with timeout (polling).
    let timeout = request.timeout.unwrap_or(Duration::from_secs(3600));
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(ProcessResult {
                        started: true,
                        exit_code: -1,
                        timed_out: true,
                        cancelled: false,
                        duration: start.elapsed(),
                        stdout: String::new(),
                        stderr: String::new(),
                    });
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return Err(VortiaError::new(
                    ErrorCode::Internal,
                    e.to_string(),
                    request.executable.clone(),
                ))
            }
        }
    }

    let output = child.wait_with_output().map_err(|e| {
        VortiaError::new(ErrorCode::Internal, e.to_string(), request.executable.clone())
    })?;

    Ok(ProcessResult {
        started: true,
        exit_code: output.status.code().unwrap_or(-1),
        timed_out: false,
        cancelled: false,
        duration: start.elapsed(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}