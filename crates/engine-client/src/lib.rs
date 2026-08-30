//! Placeholder for the engine client: process lifecycle, framing, and typed
//! request/response plumbing. E5.5-T3 fills this crate.

use serde_json::Value;
use tarik_engine_protocol::{EngineManifest, EngineFrame, RequestEnvelope, ResponseEnvelope};

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("engine process failed to start: {0}")]
    Spawn(String),
    #[error("engine channel closed before response")]
    ChannelClosed,
    #[error("engine returned a structured error: {code}")]
    Engine { code: String, message: String },
}

pub fn build_request(method: &str, params: Value) -> RequestEnvelope {
    RequestEnvelope {
        id: tarik_engine_protocol::new_request_id(),
        method: method.into(),
        params: params
            .as_object()
            .cloned()
            .unwrap_or_default(),
    }
}

pub fn frame_response(frame: EngineFrame) -> Result<ResponseEnvelope, ClientError> {
    match frame {
        EngineFrame::Response(response) => Ok(response),
        EngineFrame::Request(_) => Err(ClientError::ChannelClosed),
    }
}

pub fn check_manifest(manifest: &EngineManifest) -> Result<(), ClientError> {
    if manifest.protocol_version != tarik_engine_protocol::PROTOCOL_VERSION {
        return Err(ClientError::Engine {
            code: "protocol.mismatch".into(),
            message: format!(
                "engine {} protocol {} does not match required {}",
                manifest.id, manifest.protocol_version, tarik_engine_protocol::PROTOCOL_VERSION
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarik_engine_protocol::ErrorEnvelope;

    #[test]
    fn builds_request_with_unique_id() {
        let request = build_request("catalog.inspect", serde_json::json!({ "sessionId": "s1" }));
        assert_eq!(request.method, "catalog.inspect");
        assert!(!request.id.is_empty());
        assert_eq!(request.params["sessionId"], "s1");
    }

    #[test]
    fn surfaces_structured_engine_error() {
        let frame = EngineFrame::Response(ResponseEnvelope::err(
            "req-1",
            ErrorEnvelope::new("sql.parse", "syntax error"),
        ));
        let response = frame_response(frame).unwrap();
        assert!(!response.ok);
    }

    #[test]
    fn rejects_protocol_mismatch() {
        let manifest = EngineManifest {
            id: "duckdb".into(),
            executable: "tarik-engine-duckdb".into(),
            protocol_version: 99,
            display_name: "DuckDB".into(),
            description: None,
        };
        assert!(check_manifest(&manifest).is_err());
    }
}
