use rusqlite::{Connection, TransactionBehavior};

use super::{MetadataError, LATEST_SCHEMA_VERSION};

struct Migration {
    version: u32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "metadata_foundation",
        sql: include_str!("../../migrations/0001_metadata_foundation.sql"),
    },
    Migration {
        version: 2,
        name: "query_sessions",
        sql: include_str!("../../migrations/0002_query_sessions.sql"),
    },
    Migration {
        version: 3,
        name: "saved_queries_history",
        sql: include_str!("../../migrations/0003_saved_queries_history.sql"),
    },
    Migration {
        version: 4,
        name: "sources_exports",
        sql: include_str!("../../migrations/0004_sources_exports.sql"),
    },
    Migration {
        version: 5,
        name: "project_ownership",
        sql: include_str!("../../migrations/0005_project_ownership.sql"),
    },
    Migration {
        version: 6,
        name: "engine_locator",
        sql: include_str!("../../migrations/0006_engine_locator.sql"),
    },
    Migration {
        version: 7,
        name: "export_lifecycle",
        sql: include_str!("../../migrations/0007_export_lifecycle.sql"),
    },
    Migration {
        version: 8,
        name: "quality_checks",
        sql: include_str!("../../migrations/0008_quality_checks.sql"),
    },
];

pub(super) fn migrate(connection: &mut Connection) -> Result<(), MetadataError> {
    let current = schema_version(connection)?;
    if current > LATEST_SCHEMA_VERSION {
        return Err(MetadataError::IncompatibleVersion {
            found: current,
            supported: LATEST_SCHEMA_VERSION,
        });
    }

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current)
    {
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(migration.sql)?;
        transaction.pragma_update(None, "user_version", migration.version)?;
        transaction
            .commit()
            .map_err(|source| MetadataError::Migration {
                version: migration.version,
                name: migration.name,
                source,
            })?;
    }

    Ok(())
}

fn schema_version(connection: &Connection) -> Result<u32, MetadataError> {
    Ok(connection.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_reaches_latest_version() {
        let mut connection = Connection::open_in_memory().expect("open memory database");

        migrate(&mut connection).expect("migrate database");

        assert_eq!(schema_version(&connection).unwrap(), LATEST_SCHEMA_VERSION);
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'settings'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let quality_tables: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name LIKE 'quality_check%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(quality_tables, 3);
    }

    #[test]
    fn schema_seven_upgrades_transactionally_to_quality_schema_eight() {
        let mut connection = Connection::open_in_memory().expect("open memory database");
        for migration in MIGRATIONS.iter().filter(|migration| migration.version <= 7) {
            connection.execute_batch(migration.sql).unwrap();
            connection
                .pragma_update(None, "user_version", migration.version)
                .unwrap();
        }
        assert_eq!(schema_version(&connection).unwrap(), 7);

        migrate(&mut connection).expect("upgrade schema seven");

        assert_eq!(schema_version(&connection).unwrap(), 8);
        for table in [
            "quality_checks",
            "quality_check_revisions",
            "quality_check_runs",
        ] {
            let exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing migrated table {table}");
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let mut connection = Connection::open_in_memory().expect("open memory database");

        migrate(&mut connection).expect("first migration");
        migrate(&mut connection).expect("second migration");

        assert_eq!(schema_version(&connection).unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn newer_database_is_rejected() {
        let mut connection = Connection::open_in_memory().expect("open memory database");
        connection
            .pragma_update(None, "user_version", LATEST_SCHEMA_VERSION + 1)
            .unwrap();

        let error = migrate(&mut connection).expect_err("newer database should fail");

        assert!(matches!(error, MetadataError::IncompatibleVersion { .. }));
    }

    #[test]
    fn failed_batch_rolls_back_all_statements() {
        let mut connection = Connection::open_in_memory().expect("open memory database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();

        let result = transaction.execute_batch(
            "CREATE TABLE survives_only_if_committed(id INTEGER); INSERT INTO missing_table VALUES (1);",
        );
        assert!(result.is_err());
        drop(transaction);

        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'survives_only_if_committed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
