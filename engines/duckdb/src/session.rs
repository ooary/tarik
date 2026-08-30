use std::collections::HashMap;

use duckdb::Connection;

use crate::error::EngineError;

pub struct SessionManager {
    sessions: HashMap<String, Connection>,
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

    pub fn open(&mut self, session_id: String, path: &str) -> Result<(), EngineError> {
        if self.sessions.contains_key(&session_id) {
            return Err(EngineError::SessionExists(session_id));
        }
        let connection = Connection::open(path)?;
        self.sessions.insert(session_id, connection);
        Ok(())
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
            .ok_or_else(|| EngineError::SessionMissing(session_id.to_string()))
    }
}
