use std::{
    io::{BufRead, BufReader, BufWriter, Lines, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex},
};

use serde_json::Value;
use tarik_engine_protocol::{EngineInfo, ErrorEnvelope, RequestEnvelope, ResponseEnvelope};

use crate::ClientError;

const MAX_STDERR_LINES: usize = 30;

/// Long-lived engine process with newline-delimited JSON framing over stdio.
/// The child's stderr is captured so a startup crash surfaces its real reason
/// (for example a missing dynamic library) instead of a bare channel error.
pub struct EngineProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: Lines<BufReader<ChildStdout>>,
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

impl EngineProcess {
    pub fn start(executable: &Path) -> Result<Self, ClientError> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
            return response.result.ok_or(ClientError::ChannelClosed);
        }
    }

    fn closed_with_detail(&mut self) -> ClientError {
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
