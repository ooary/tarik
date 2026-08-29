use serde::{de::DeserializeOwned, Serialize};

use super::{MetadataDb, MetadataError};

#[derive(Clone)]
pub struct SettingsRepository {
    database: MetadataDb,
}

impl SettingsRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare("SELECT value_json FROM settings WHERE key = ?1")?;
        let mut rows = statement.query([key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let json: String = row.get(0)?;
        serde_json::from_str(&json)
            .map(Some)
            .map_err(|source| MetadataError::InvalidJson {
                key: key.to_owned(),
                source,
            })
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<(), MetadataError> {
        let json = serde_json::to_string(value).map_err(|source| MetadataError::InvalidJson {
            key: key.to_owned(),
            source,
        })?;
        let connection = self.database.connection()?;
        connection.execute(
            "INSERT INTO settings(key, value_json, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            (key, json),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct Preference {
        theme: String,
        width: u16,
    }

    #[test]
    fn typed_setting_round_trip_and_remove() {
        let database = MetadataDb::open_in_memory().unwrap();
        let repository = SettingsRepository::new(database);
        let preference = Preference {
            theme: "dark".into(),
            width: 312,
        };

        assert_eq!(repository.get::<Preference>("workbench").unwrap(), None);
        repository.set("workbench", &preference).unwrap();
        assert_eq!(repository.get("workbench").unwrap(), Some(preference));
    }

    #[test]
    fn invalid_json_is_a_structured_error() {
        let database = MetadataDb::open_in_memory().unwrap();
        database
            .connection()
            .unwrap()
            .execute(
                "INSERT INTO settings(key, value_json, updated_at) VALUES ('bad', '{', datetime('now'))",
                [],
            )
            .unwrap();
        let repository = SettingsRepository::new(database);

        assert!(matches!(
            repository.get::<Preference>("bad"),
            Err(MetadataError::InvalidJson { .. })
        ));
    }
}
