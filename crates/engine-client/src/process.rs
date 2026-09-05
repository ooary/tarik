use std::{
    io::{BufRead, BufReader, BufWriter, Lines, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use serde_json::Value;
use tarik_engine_protocol::{EngineInfo, ErrorEnvelope, RequestEnvelope, ResponseEnvelope};

use crate::ClientError;

const MAX_STDERR_LINES: usize = 30;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Long-lived engine process with newline-delimited JSON framing over stdio.
/// The child's stderr is captured so a startup crash surfaces its real reason
/// (for example a missing dynamic library) instead of a bare channel error.
pub struct EngineProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: Lines<BufReader<ChildStdout>>,
    stderr_tail: Arc<Mutex<Vec<String>>>,
    broken: bool,
}

impl EngineProcess {
    pub fn start(executable: &Path) -> Result<Self, ClientError> {
        let mut command = Command::new(executable);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .spawn()
            .map_err(|error| ClientError::Spawn(format!("{error:?}")))?;
        let stdin = BufWriter::new(child.stdin.take().ok_or(ClientError::ChannelClosed)?);
        let stdout = BufReader::new(child.stdout.take().ok_or(ClientError::ChannelClosed)?).lines();
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));

        if let Some(stderr) = child.stderr.take() {
            let tail = stderr_tail.clone();
            std::thread::Builder::new()
                .name("tarik-engine-stderr".into())
                .spawn(move || {
                    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                        if let Ok(mut tail) = tail.lock() {
                            if tail.len() >= MAX_STDERR_LINES {
                                tail.remove(0);
                            }
                            tail.push(line);
                        }
                    }
                })
                .map_err(|error| ClientError::Spawn(format!("stderr thread: {error:?}")))?;
        }

        Ok(Self {
            child,
            stdin,
            stdout,
            stderr_tail,
            broken: false,
        })
    }

    pub fn handshake(&mut self) -> Result<EngineInfo, ClientError> {
        let result = self.request("engine.handshake", serde_json::json!({}))?;
        serde_json::from_value(result).map_err(|error| ClientError::Protocol(format!("{error:?}")))
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, ClientError> {
        let request = RequestEnvelope {
            id: tarik_engine_protocol::new_request_id(),
            method: method.into(),
            params: params.as_object().cloned().unwrap_or_default(),
        };
        let line = serde_json::to_string(&request)
            .map_err(|error| ClientError::Protocol(format!("{error:?}")))?;
        if self.stdin.write_all(line.as_bytes()).is_err() {
            return Err(self.closed_with_detail());
        }
        if self.stdin.write_all(b"\n").is_err() {
            return Err(self.closed_with_detail());
        }
        if self.stdin.flush().is_err() {
            return Err(self.closed_with_detail());
        }

        loop {
            let Some(Ok(response_line)) = self.stdout.next() else {
                return Err(self.closed_with_detail());
            };
            let response: ResponseEnvelope = serde_json::from_str(&response_line)
                .map_err(|error| ClientError::Protocol(format!("{error:?}")))?;
            if response.id != request.id {
                continue;
            }
            if !response.ok {
                let error = response.error.unwrap_or_else(|| {
                    ErrorEnvelope::new("engine.unknown", "engine returned an empty failure")
                });
                return Err(ClientError::Engine {
                    code: error.code,
                    message: error.message,
                });
            }
            // A successful call with a missing/null result is a valid no-return
            // response (for example session.open). Deserialize JSON null as Null.
            return Ok(response.result.unwrap_or(Value::Null));
        }
    }

    fn closed_with_detail(&mut self) -> ClientError {
        self.broken = true;
        let exit = self
            .child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.to_string())
            .unwrap_or_else(|| "still running but channel closed".to_string());
        let stderr = self
            .stderr_tail
            .lock()
            .ok()
            .map(|lines| lines.join("\n"))
            .unwrap_or_default();
        ClientError::Engine {
            code: "engine.exited".into(),
            message: format!(
                "engine process closed its channel before responding (exit: {exit}).\n\
                 Engine output:\n{stderr}"
            ),
        }
    }

    pub fn is_usable(&mut self) -> bool {
        !self.broken && matches!(self.child.try_wait(), Ok(None))
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }

    pub fn terminate(&mut self) {
        self.broken = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    pub fn shutdown(mut self) {
        let _ = self.request("engine.shutdown", serde_json::json!({}));
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for EngineProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_sidecar_has_no_main_window() {
        let executable = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target/debug/tarik-engine-duckdb.exe");
        assert!(
            executable.exists(),
            "build the DuckDB engine before this test"
        );
        let mut process = EngineProcess::start(&executable).unwrap();
        process.handshake().unwrap();
        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "(Get-Process -Id {} -ErrorAction Stop).MainWindowHandle",
                    process.id()
                ),
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
        process.shutdown();
    }
}
