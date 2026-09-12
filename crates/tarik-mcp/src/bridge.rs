#[cfg(unix)]
use std::time::Duration;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

use hmac::{Hmac, Mac};
#[cfg(unix)]
use interprocess::local_socket::GenericFilePath;
#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{prelude::*, Stream};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tarik_agent_protocol::{
    AgentExecutionResult, AgentExportIntent, AgentExportReleaseResult, AgentExportView,
    AgentResultPage, ApprovalResult, AuthenticationResult, BridgeAction, BridgeRequest,
    BridgeResponse, CatalogPageResult, ExportDestinationList, GrantedProjectsResult, HelloResult,
    RelationDescriptionResult, SqlSnapshotResult, MAX_BRIDGE_MESSAGE_BYTES,
};
use zeroize::Zeroizing;

use crate::profile::ClientProfile;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeDescriptor {
    protocol_version: u32,
    endpoint: String,
    transport: String,
    #[allow(dead_code)]
    instance_id: String,
}

pub struct BridgeClient {
    stream: BufReader<Stream>,
    connection_id: Option<String>,
    instance_id: String,
}

impl BridgeClient {
    pub fn connect(agent_dir: &Path) -> Result<Self, String> {
        let descriptor = read_descriptor(agent_dir)?;
        if descriptor.protocol_version != tarik_agent_protocol::BRIDGE_PROTOCOL_VERSION {
            return Err("Tarik and tarik-mcp bridge versions do not match".into());
        }
        let name = endpoint_name(&descriptor)?;
        let stream = Stream::connect(name)
            .map_err(|_| "Tarik is not running or Agent Access is disabled".to_string())?;
        #[cfg(unix)]
        {
            stream
                .set_recv_timeout(Some(Duration::from_secs(10)))
                .map_err(|error| format!("could not configure Tarik bridge: {error}"))?;
            stream
                .set_send_timeout(Some(Duration::from_secs(10)))
                .map_err(|error| format!("could not configure Tarik bridge: {error}"))?;
        }
        Ok(Self {
            stream: BufReader::new(stream),
            connection_id: None,
            instance_id: descriptor.instance_id,
        })
    }

    pub fn endpoint_is_current(&self, agent_dir: &Path) -> bool {
        descriptor_matches_instance(agent_dir, &self.instance_id)
    }

    pub fn hello(
        &mut self,
        profile: Option<&ClientProfile>,
        label: &str,
        new_pairing_key: Option<&str>,
    ) -> Result<HelloResult, String> {
        self.request(BridgeAction::Hello {
            profile_id: profile
                .map(|profile| profile.profile_id.clone())
                .unwrap_or_default(),
            label: label.into(),
            pairing_key: new_pairing_key.map(ToOwned::to_owned),
        })
    }

    pub fn authenticate(
        &mut self,
        hello: &HelloResult,
        pairing_key_hex: &str,
    ) -> Result<AuthenticationResult, String> {
        let pairing_key = Zeroizing::new(decode_hex::<32>(pairing_key_hex)?);
        let salt = decode_hex::<32>(&hello.salt)?;
        let challenge = decode_hex::<32>(&hello.challenge)?;
        let verifier = Zeroizing::new(derive_verifier(&pairing_key, &salt));
        let proof = challenge_proof(verifier.as_ref(), &hello.connection_id, &challenge)?;
        let result = self.request(BridgeAction::Authenticate {
            connection_id: hello.connection_id.clone(),
            profile_id: hello.profile_id.clone(),
            proof: encode_hex(&proof),
        })?;
        self.connection_id = Some(hello.connection_id.clone());
        Ok(result)
    }

    pub fn list_projects(&mut self) -> Result<GrantedProjectsResult, String> {
        self.request(BridgeAction::ListProjects {
            connection_id: self.connection_id()?,
        })
    }

    pub fn list_export_destinations(
        &mut self,
        project_id: String,
    ) -> Result<ExportDestinationList, String> {
        self.request(BridgeAction::ListExportDestinations {
            connection_id: self.connection_id()?,
            project_id,
        })
    }

    pub fn propose_export(&mut self, intent: AgentExportIntent) -> Result<AgentExportView, String> {
        self.request(BridgeAction::ProposeExport {
            connection_id: self.connection_id()?,
            intent,
        })
    }

    pub fn export_status(&mut self, export_id: String) -> Result<AgentExportView, String> {
        self.request(BridgeAction::ExportStatus {
            connection_id: self.connection_id()?,
            export_id,
        })
    }

    pub fn export_cancel(&mut self, export_id: String) -> Result<AgentExportView, String> {
        self.request(BridgeAction::ExportCancel {
            connection_id: self.connection_id()?,
            export_id,
        })
    }

    pub fn export_release(
        &mut self,
        export_id: String,
    ) -> Result<AgentExportReleaseResult, String> {
        self.request(BridgeAction::ExportRelease {
            connection_id: self.connection_id()?,
            export_id,
        })
    }

    pub fn list_catalog(
        &mut self,
        project_id: String,
        search: Option<String>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<CatalogPageResult, String> {
        self.request(BridgeAction::ListCatalog {
            connection_id: self.connection_id()?,
            project_id,
            search,
            cursor,
            limit,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn describe_relation(
        &mut self,
        project_id: String,
        database: String,
        schema: String,
        name: String,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<RelationDescriptionResult, String> {
        self.request(BridgeAction::DescribeRelation {
            connection_id: self.connection_id()?,
            project_id,
            database,
            schema,
            name,
            cursor,
            limit,
        })
    }

    pub fn classify_sql(
        &mut self,
        project_id: String,
        sql: String,
    ) -> Result<SqlSnapshotResult, String> {
        self.request(BridgeAction::ClassifySql {
            connection_id: self.connection_id()?,
            project_id,
            sql,
        })
    }

    pub fn start_query(&mut self, snapshot_id: String) -> Result<AgentExecutionResult, String> {
        self.request(BridgeAction::StartQuery {
            connection_id: self.connection_id()?,
            snapshot_id,
        })
    }

    pub fn query_status(&mut self, execution_id: String) -> Result<AgentExecutionResult, String> {
        self.request(BridgeAction::QueryStatus {
            connection_id: self.connection_id()?,
            execution_id,
        })
    }

    pub fn cancel_query(&mut self, execution_id: String) -> Result<AgentExecutionResult, String> {
        self.request(BridgeAction::CancelQuery {
            connection_id: self.connection_id()?,
            execution_id,
        })
    }

    pub fn result_page(
        &mut self,
        result_id: String,
        offset: u64,
        max_rows: u32,
    ) -> Result<AgentResultPage, String> {
        self.request(BridgeAction::GetResultPage {
            connection_id: self.connection_id()?,
            result_id,
            offset,
            max_rows,
        })
    }

    pub fn release_result(&mut self, result_id: String) -> Result<bool, String> {
        self.request(BridgeAction::ReleaseResult {
            connection_id: self.connection_id()?,
            result_id,
        })
    }

    pub fn propose_sql(&mut self, snapshot_id: String) -> Result<ApprovalResult, String> {
        self.request(BridgeAction::ProposeSql {
            connection_id: self.connection_id()?,
            snapshot_id,
        })
    }

    pub fn approval_status(&mut self, approval_id: String) -> Result<ApprovalResult, String> {
        self.request(BridgeAction::ApprovalStatus {
            connection_id: self.connection_id()?,
            approval_id,
        })
    }

    pub fn execute_approved(
        &mut self,
        approval_id: String,
    ) -> Result<AgentExecutionResult, String> {
        self.request(BridgeAction::ExecuteApproved {
            connection_id: self.connection_id()?,
            approval_id,
        })
    }

    pub fn explain_sql(
        &mut self,
        snapshot_id: String,
        actual: bool,
    ) -> Result<serde_json::Value, String> {
        self.request(BridgeAction::ExplainSql {
            connection_id: self.connection_id()?,
            snapshot_id,
            actual,
        })
    }

    pub fn start_profile(
        &mut self,
        request: tarik_engine_protocol::ProfileRequest,
    ) -> Result<tarik_engine_protocol::ProfileStatus, String> {
        self.request(BridgeAction::StartProfile {
            connection_id: self.connection_id()?,
            request,
        })
    }

    pub fn profile_status(
        &mut self,
        profile_id: String,
    ) -> Result<tarik_engine_protocol::ProfileStatus, String> {
        self.request(BridgeAction::ProfileStatus {
            connection_id: self.connection_id()?,
            profile_id,
        })
    }

    pub fn cancel_profile(
        &mut self,
        profile_id: String,
    ) -> Result<tarik_engine_protocol::ProfileStatus, String> {
        self.request(BridgeAction::CancelProfile {
            connection_id: self.connection_id()?,
            profile_id,
        })
    }

    pub fn list_quality(
        &mut self,
        project_id: String,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value, String> {
        self.request(BridgeAction::ListQuality {
            connection_id: self.connection_id()?,
            project_id,
            offset,
            limit,
        })
    }

    pub fn list_quality_runs(
        &mut self,
        project_id: String,
        check_id: Option<String>,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value, String> {
        self.request(BridgeAction::ListQualityRuns {
            connection_id: self.connection_id()?,
            project_id,
            check_id,
            offset,
            limit,
        })
    }

    pub fn list_saved_queries(
        &mut self,
        project_id: String,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value, String> {
        self.request(BridgeAction::ListSavedQueries {
            connection_id: self.connection_id()?,
            project_id,
            offset,
            limit,
        })
    }

    fn connection_id(&self) -> Result<String, String> {
        self.connection_id
            .clone()
            .ok_or_else(|| "Authenticate with Tarik before using project tools".into())
    }

    fn request<T: serde::de::DeserializeOwned>(
        &mut self,
        action: BridgeAction,
    ) -> Result<T, String> {
        let request = BridgeRequest {
            id: uuid::Uuid::new_v4().to_string(),
            protocol_version: tarik_agent_protocol::BRIDGE_PROTOCOL_VERSION,
            action,
        };
        let mut bytes = serde_json::to_vec(&request)
            .map_err(|error| format!("could not encode Tarik bridge request: {error}"))?;
        if bytes.len() >= MAX_BRIDGE_MESSAGE_BYTES {
            return Err("Tarik bridge request is too large".into());
        }
        bytes.push(b'\n');
        self.stream
            .get_mut()
            .write_all(&bytes)
            .map_err(|_| "Tarik closed the local agent connection".to_string())?;
        let mut response = Vec::with_capacity(1024);
        std::io::Read::by_ref(&mut self.stream)
            .take((MAX_BRIDGE_MESSAGE_BYTES + 1) as u64)
            .read_until(b'\n', &mut response)
            .map_err(|_| "Tarik did not answer the local agent request".to_string())?;
        if response.len() > MAX_BRIDGE_MESSAGE_BYTES {
            return Err("Tarik bridge response is too large".into());
        }
        let response: BridgeResponse = serde_json::from_slice(&response)
            .map_err(|_| "Tarik returned an invalid local agent response".to_string())?;
        if !response.ok {
            let error = response
                .error
                .map(|error| format!("{}: {}", error.code, error.message))
                .unwrap_or_else(|| "Tarik rejected the local agent request".into());
            return Err(error);
        }
        serde_json::from_value(response.result.unwrap_or(serde_json::Value::Null))
            .map_err(|_| "Tarik returned an unexpected local agent response".into())
    }
}

fn read_descriptor(agent_dir: &Path) -> Result<BridgeDescriptor, String> {
    let path = agent_dir.join("bridge.json");
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "Tarik is not running or Agent Access is disabled".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Tarik agent endpoint descriptor is unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err("Tarik agent endpoint descriptor is not user-private".into());
        }
    }
    let bytes = fs::read(path).map_err(|_| "Tarik agent endpoint is unavailable".to_string())?;
    serde_json::from_slice(&bytes).map_err(|_| "Tarik agent endpoint descriptor is invalid".into())
}

fn descriptor_matches_instance(agent_dir: &Path, instance_id: &str) -> bool {
    read_descriptor(agent_dir)
        .map(|descriptor| descriptor.instance_id == instance_id)
        .unwrap_or(false)
}

fn endpoint_name(
    descriptor: &BridgeDescriptor,
) -> Result<interprocess::local_socket::Name<'static>, String> {
    match descriptor.transport.as_str() {
        #[cfg(unix)]
        "unix_socket" => std::path::PathBuf::from(&descriptor.endpoint)
            .to_fs_name::<GenericFilePath>()
            .map_err(|error| format!("invalid Tarik Unix socket: {error}")),
        #[cfg(windows)]
        "windows_named_pipe" => descriptor
            .endpoint
            .clone()
            .to_ns_name::<GenericNamespaced>()
            .map_err(|error| format!("invalid Tarik named pipe: {error}")),
        _ => Err("Tarik agent endpoint transport is unsupported".into()),
    }
}

pub fn generate_pairing_key() -> Result<String, String> {
    let mut key = Zeroizing::new([0u8; 32]);
    getrandom::fill(key.as_mut())
        .map_err(|error| format!("secure entropy is unavailable: {error}"))?;
    Ok(encode_hex(key.as_ref()))
}

fn derive_verifier(key: &[u8; 32], salt: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"tarik-agent-verifier-v1");
    hash.update(salt);
    hash.update(key);
    hash.finalize().into()
}

fn challenge_proof(
    verifier: &[u8],
    connection_id: &str,
    challenge: &[u8; 32],
) -> Result<[u8; 32], String> {
    let mut mac = HmacSha256::new_from_slice(verifier)
        .map_err(|_| "could not construct Tarik authentication proof".to_string())?;
    mac.update(b"tarik-agent-proof-v1");
    mac.update(connection_id.as_bytes());
    mac.update(challenge);
    Ok(mac.finalize().into_bytes().into())
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(DIGITS[(byte >> 4) as usize] as char);
        value.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    value
}

fn decode_hex<const N: usize>(value: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("stored Tarik MCP credential is invalid; pair again".into());
    }
    let mut decoded = [0u8; N];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| "stored Tarik MCP credential is invalid; pair again".to_string())?;
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_descriptor(root: &Path, instance_id: &str) {
        fs::write(
            root.join("bridge.json"),
            serde_json::to_vec(&serde_json::json!({
                "protocolVersion": tarik_agent_protocol::BRIDGE_PROTOCOL_VERSION,
                "endpoint": "test-endpoint",
                "transport": if cfg!(windows) {
                    "windows_named_pipe"
                } else {
                    "unix_socket"
                },
                "instanceId": instance_id,
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn key_derivation_and_proof_are_deterministic() {
        let key = [7u8; 32];
        let salt = [3u8; 32];
        let challenge = [9u8; 32];
        let first = derive_verifier(&key, &salt);
        assert_eq!(first, derive_verifier(&key, &salt));
        assert_eq!(
            challenge_proof(&first, "connection", &challenge).unwrap(),
            challenge_proof(&first, "connection", &challenge).unwrap()
        );
    }

    #[test]
    fn descriptor_instance_change_marks_bridge_stale() {
        let root = std::env::temp_dir().join(format!(
            "tarik-mcp-bridge-descriptor-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        write_test_descriptor(&root, "first");
        assert!(descriptor_matches_instance(&root, "first"));

        write_test_descriptor(&root, "second");
        assert!(!descriptor_matches_instance(&root, "first"));

        fs::remove_dir_all(root).unwrap();
    }
}
