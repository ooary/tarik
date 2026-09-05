use std::collections::HashMap;

use duckdb::Connection;
use tarik_engine_protocol::{EffectiveEngineResources, EngineResourceSettings};

use crate::{error::EngineError, resources};

struct Session {
    connection: Connection,
    effective_resources: EffectiveEngineResources,
}

pub struct SessionManager {
    sessions: HashMap<String, Session>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn open(
        &mut self,
        session_id: String,
        path: &str,
        requested: &EngineResourceSettings,
    ) -> Result<EffectiveEngineResources, EngineError> {
        if self.sessions.contains_key(&session_id) {
            return Err(EngineError::SessionExists(session_id));
        }
        let connection = Connection::open(path)?;
        let effective_resources = resources::apply(&connection, requested)?;
        self.sessions.insert(
            session_id,
            Session {
                connection,
                effective_resources: effective_resources.clone(),
            },
        );
        Ok(effective_resources)
    }

    pub fn configure(
        &mut self,
        session_id: &str,
        requested: &EngineResourceSettings,
    ) -> Result<EffectiveEngineResources, EngineError> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| EngineError::SessionMissing(session_id.to_string()))?;
        let previous = session.effective_resources.clone();
        match resources::apply(&session.connection, requested) {
            Ok(effective) => {
                session.effective_resources = effective.clone();
                Ok(effective)
            }
            Err(apply_error) => {
                let rollback = EngineResourceSettings {
                    preset: previous.preset,
                    memory_limit_mib: previous.memory_limit_mib,
                    threads: previous.threads,
                };
                if let Err(rollback_error) = resources::apply(&session.connection, &rollback) {
                    return Err(EngineError::InvalidResources(format!(
                        "resource apply failed ({apply_error}); previous settings could not be restored ({rollback_error})"
                    )));
                }
                Err(apply_error)
            }
        }
    }

    pub fn resources(&self, session_id: &str) -> Result<EffectiveEngineResources, EngineError> {
        self.sessions
            .get(session_id)
            .map(|session| session.effective_resources.clone())
            .ok_or_else(|| EngineError::SessionMissing(session_id.to_string()))
    }

    pub fn close(&mut self, session_id: &str) -> Result<(), EngineError> {
        if self.sessions.remove(session_id).is_none() {
            return Err(EngineError::SessionMissing(session_id.to_string()));
        }
        Ok(())
    }

    pub fn get(&self, session_id: &str) -> Result<&Connection, EngineError> {
        self.sessions
            .get(session_id)
            .map(|session| &session.connection)
            .ok_or_else(|| EngineError::SessionMissing(session_id.to_string()))
    }
}
