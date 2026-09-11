//! Private, versioned protocol between the Tarik desktop and `tarik-mcp`.
//!
//! This is intentionally not the MCP protocol. It is the narrow authenticated
//! bridge contract behind the public MCP tools and never mirrors Tauri or
//! engine methods dynamically.

use serde::{Deserialize, Serialize};

pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
pub const MAX_BRIDGE_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_CLIENT_LABEL_BYTES: usize = 80;
pub const MAX_PROFILE_ID_BYTES: usize = 128;
pub const CHALLENGE_BYTES: usize = 32;
pub const PROOF_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRequest {
    pub id: String,
    pub protocol_version: u32,
    #[serde(flatten)]
    pub action: BridgeAction,
}

impl BridgeRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != BRIDGE_PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        if self.id.trim().is_empty() || self.id.len() > 128 {
            return Err(ProtocolError::InvalidRequestId);
        }
        match &self.action {
            BridgeAction::Hello {
                profile_id,
                label,
                pairing_key,
            } => {
                if profile_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProfileId);
                }
                if label.trim().is_empty() || label.len() > MAX_CLIENT_LABEL_BYTES {
                    return Err(ProtocolError::InvalidClientLabel);
                }
                match (profile_id.is_empty(), pairing_key) {
                    (true, Some(key)) if valid_proof_hex(key) => {}
                    (true, _) => return Err(ProtocolError::PairingKeyRequired),
                    (false, None) => {}
                    (false, Some(_)) => return Err(ProtocolError::UnexpectedPairingKey),
                }
            }
            BridgeAction::Authenticate {
                connection_id,
                profile_id,
                proof,
            } => {
                if connection_id.trim().is_empty() {
                    return Err(ProtocolError::InvalidConnectionId);
                }
                if profile_id.trim().is_empty() || profile_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProfileId);
                }
                if !valid_proof_hex(proof) {
                    return Err(ProtocolError::InvalidProof);
                }
            }
            BridgeAction::Status { connection_id } | BridgeAction::Disconnect { connection_id } => {
                if connection_id.trim().is_empty() {
                    return Err(ProtocolError::InvalidConnectionId);
                }
            }
            BridgeAction::Ping => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BridgeAction {
    Ping,
    Hello {
        #[serde(default)]
        profile_id: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pairing_key: Option<String>,
    },
    Authenticate {
        connection_id: String,
        profile_id: String,
        proof: String,
    },
    Status {
        connection_id: String,
    },
    Disconnect {
        connection_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeResponse {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BridgeError>,
}

impl BridgeResponse {
    pub fn success<T: Serialize>(id: impl Into<String>, value: T) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: serde_json::to_value(value).ok(),
            error: None,
        }
    }

    pub fn failure(
        id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(BridgeError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResult {
    pub state: HelloState,
    pub connection_id: String,
    pub profile_id: String,
    pub challenge: String,
    pub salt: String,
    pub pairing_request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelloState {
    PairingRequired,
    AuthenticationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationResult {
    pub client_profile_id: String,
    pub connection_id: String,
    pub grants: Vec<ProjectGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusResult {
    pub authenticated: bool,
    pub client_profile_id: Option<String>,
    pub grants: Vec<ProjectGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGrant {
    pub project_id: String,
    pub inspect: bool,
    pub analyze: bool,
    pub modify_workspace: bool,
    pub modify_data: bool,
}

impl ProjectGrant {
    pub fn has_any(&self) -> bool {
        self.inspect || self.analyze || self.modify_workspace || self.modify_data
    }
}

fn valid_proof_hex(value: &str) -> bool {
    value.len() == PROOF_BYTES * 2 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("unsupported bridge protocol version")]
    UnsupportedVersion,
    #[error("invalid bridge request id")]
    InvalidRequestId,
    #[error("invalid client profile id")]
    InvalidProfileId,
    #[error("invalid client display label")]
    InvalidClientLabel,
    #[error("a new client must provide a 32-byte pairing key")]
    PairingKeyRequired,
    #[error("an existing client must not replace its pairing key")]
    UnexpectedPairingKey,
    #[error("invalid bridge connection id")]
    InvalidConnectionId,
    #[error("invalid authentication proof")]
    InvalidProof,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: BridgeAction) -> BridgeRequest {
        BridgeRequest {
            id: "request-1".into(),
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            action,
        }
    }

    #[test]
    fn validates_hello_and_authentication_boundaries() {
        assert!(request(BridgeAction::Hello {
            profile_id: String::new(),
            label: "Claude Desktop".into(),
            pairing_key: Some("01".repeat(PROOF_BYTES)),
        })
        .validate()
        .is_ok());
        assert_eq!(
            request(BridgeAction::Authenticate {
                connection_id: "connection-1".into(),
                profile_id: "profile-1".into(),
                proof: "not-hex".into(),
            })
            .validate(),
            Err(ProtocolError::InvalidProof)
        );
    }

    #[test]
    fn rejects_wrong_protocol_and_oversized_labels() {
        let mut wrong = request(BridgeAction::Ping);
        wrong.protocol_version += 1;
        assert_eq!(wrong.validate(), Err(ProtocolError::UnsupportedVersion));
        assert_eq!(
            request(BridgeAction::Hello {
                profile_id: String::new(),
                label: "x".repeat(MAX_CLIENT_LABEL_BYTES + 1),
                pairing_key: Some("01".repeat(PROOF_BYTES)),
            })
            .validate(),
            Err(ProtocolError::InvalidClientLabel)
        );
    }

    #[test]
    fn grants_report_only_real_capabilities() {
        assert!(!ProjectGrant {
            project_id: "p".into(),
            inspect: false,
            analyze: false,
            modify_workspace: false,
            modify_data: false,
        }
        .has_any());
    }
}
