use std::{
    io::{BufRead, BufReader, BufWriter, Lines, Write},
    path::Path,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde_json::Value;
use tarik_engine_protocol::{EngineInfo, ErrorEnvelope, RequestEnvelope, ResponseEnvelope};

use crate::ClientError;

/// Long-lived engine process with newline-delimited JSON framing over stdio.
pub struct EngineProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl EngineProcess {
    pub fn start(executable: &Path) -> Result<Self, ClientError> {
        let mut child = Command::new(executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| ClientError::Spawn(format!("{error:?}")))?;
        let stdin = BufWriter::new(child.stdin.take().ok_or(ClientError::ChannelClosed)?);
        let stdout = BufReader::new(child.stdout.take().ok_or(ClientError::ChannelClosed)?).lines();
        Ok(Self {
            child,
            stdin,
            stdout,
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
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|_| ClientError::ChannelClosed)?;
        self.stdin
            .write_all(b"\n")
            .map_err(|_| ClientError::ChannelClosed)?;
        self.stdin.flush().map_err(|_| ClientError::ChannelClosed)?;

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
        ClientError::Engine {
            code: "engine.exited".into(),
            message: format!(
                "engine process closed its channel before responding (exit: {exit}). \
                 Check that the engine binary exists and its DuckDB runtime library is beside it."
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
