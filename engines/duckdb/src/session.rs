use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use duckdb::Connection;
use tarik_engine_protocol::{EffectiveEngineResources, EngineResourceSettings};

use crate::{error::EngineError, resources};

struct Session {
    connection: Connection,
    effective_resources: EffectiveEngineResources,
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn open(
        &mut self,
        session_id: String,
        path: &str,
        requested: &EngineResourceSettings,
    ) -> Result<EffectiveEngineResources, EngineError> {
        let mut sessions = self.lock()?;
        if sessions.contains_key(&session_id) {
            return Err(EngineError::SessionExists(session_id));
        }
        let connection = Connection::open(path)?;
        let effective_resources = resources::apply(&connection, requested)?;
        sessions.insert(
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
        let mut sessions = self.lock()?;
        let session = sessions
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
        self.lock()?
            .get(session_id)
            .map(|session| session.effective_resources.clone())
            .ok_or_else(|| EngineError::SessionMissing(session_id.to_string()))
    }

    pub fn close(&mut self, session_id: &str) -> Result<(), EngineError> {
        if self.lock()?.remove(session_id).is_none() {
            return Err(EngineError::SessionMissing(session_id.to_string()));
        }
        Ok(())
    }

    pub fn with_connection<T>(
        &self,
        session_id: &str,
        operation: impl FnOnce(&Connection) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        let sessions = self.lock()?;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| EngineError::SessionMissing(session_id.to_string()))?;
        operation(&session.connection)
    }

    pub fn clone_connection(&self, session_id: &str) -> Result<Connection, EngineError> {
        self.with_connection(session_id, |connection| Ok(connection.try_clone()?))
    }

    fn lock(&self) -> Result<MutexGuard<'_, HashMap<String, Session>>, EngineError> {
        self.sessions
            .lock()
            .map_err(|_| EngineError::RegistryPoisoned)
    }
}
