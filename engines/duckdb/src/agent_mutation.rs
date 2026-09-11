use duckdb::Connection;
use tarik_engine_protocol::{AgentMutationResult, AgentSqlDecision};

use crate::{agent_policy, catalog, error::EngineError};

pub fn execute(
    connection: &mut Connection,
    sql: &str,
    expected_catalog_revision: &str,
    registered_sources: &[tarik_engine_protocol::AgentRegisteredSource],
) -> Result<AgentMutationResult, EngineError> {
    let classification = agent_policy::classify(connection, sql, registered_sources)?;
    if !matches!(
        classification.decision,
        AgentSqlDecision::ApprovalRequired | AgentSqlDecision::CriticalConfirmation
    ) || classification.catalog_revision != expected_catalog_revision
    {
        return Err(EngineError::InvalidQuery(
            "approved mutation classification or catalog revision changed",
        ));
    }
    let transaction = connection.transaction()?;
    let rows_affected = transaction.execute(sql, [])? as u64;
    transaction.commit()?;
    let catalog_revision = catalog::inspect(connection)?.revision;
    Ok(AgentMutationResult {
        rows_affected: Some(rows_affected),
        catalog_revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_commits_once_and_errors_roll_back() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE orders(id INTEGER PRIMARY KEY, amount INTEGER);")
            .unwrap();
        let classified =
            agent_policy::classify(&connection, "INSERT INTO orders VALUES (1, 10)", &[]).unwrap();
        execute(
            &mut connection,
            "INSERT INTO orders VALUES (1, 10)",
            &classified.catalog_revision,
            &[],
        )
        .unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM orders", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let classified =
            agent_policy::classify(&connection, "INSERT INTO orders VALUES (1, 20)", &[]).unwrap();
        assert!(execute(
            &mut connection,
            "INSERT INTO orders VALUES (1, 20)",
            &classified.catalog_revision,
            &[],
        )
        .is_err());
        let amount: i64 = connection
            .query_row("SELECT amount FROM orders", [], |row| row.get(0))
            .unwrap();
        assert_eq!(amount, 10);
    }
}
