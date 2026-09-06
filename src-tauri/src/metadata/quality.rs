use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

use super::{MetadataDb, MetadataError};

pub const MAX_CHECKS_PER_PROJECT: u32 = 200;
pub const MAX_CHECK_NAME_BYTES: usize = 64;
pub const MAX_COMPOSITE_COLUMNS: usize = 16;
pub const MAX_ACCEPTED_VALUES: usize = 100;
pub const MAX_ACCEPTED_VALUE_BYTES: usize = 32 * 1024;
pub const MAX_CUSTOM_SQL_BYTES: usize = 256 * 1024;
#[allow(dead_code)] // consumed by the internal T3 run finalizer, never a frontend command
pub const MAX_RUNS_PER_CHECK: u32 = 100;
#[allow(dead_code)] // consumed by the internal T3 run finalizer, never a frontend command
pub const MAX_RUNS_PER_PROJECT: u32 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    NotEmpty,
    NotNull,
    Unique,
    AcceptedValues,
    Range,
    Relationship,
    Freshness,
    CustomSql,
}

impl CheckType {
    fn as_str(self) -> &'static str {
        match self {
            Self::NotEmpty => "not_empty",
            Self::NotNull => "not_null",
            Self::Unique => "unique",
            Self::AcceptedValues => "accepted_values",
            Self::Range => "range",
            Self::Relationship => "relationship",
            Self::Freshness => "freshness",
            Self::CustomSql => "custom_sql",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "not_empty" => Ok(Self::NotEmpty),
            "not_null" => Ok(Self::NotNull),
            "unique" => Ok(Self::Unique),
            "accepted_values" => Ok(Self::AcceptedValues),
            "range" => Ok(Self::Range),
            "relationship" => Ok(Self::Relationship),
            "freshness" => Ok(Self::Freshness),
            "custom_sql" => Ok(Self::CustomSql),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NullPolicy {
    FailOnNull,
    PassOnNull,
}

impl NullPolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::FailOnNull => "fail_on_null",
            Self::PassOnNull => "pass_on_null",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "fail_on_null" => Ok(Self::FailOnNull),
            "pass_on_null" => Ok(Self::PassOnNull),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    Info,
    Warning,
    Critical,
}

impl CheckSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "info" => Ok(Self::Info),
            "warning" => Ok(Self::Warning),
            "critical" => Ok(Self::Critical),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityTarget {
    pub database: String,
    pub schema: String,
    pub object: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckOptions {
    NotEmpty,
    NotNull,
    Unique,
    AcceptedValues {
        values: Vec<serde_json::Value>,
    },
    Range {
        minimum: Option<serde_json::Value>,
        maximum: Option<serde_json::Value>,
        inclusive_minimum: bool,
        inclusive_maximum: bool,
    },
    Relationship {
        parent: QualityTarget,
        parent_columns: Vec<String>,
    },
    Freshness {
        maximum_age_seconds: u64,
    },
    CustomSql {
        sql: String,
    },
}

impl CheckOptions {
    fn check_type(&self) -> CheckType {
        match self {
            Self::NotEmpty => CheckType::NotEmpty,
            Self::NotNull => CheckType::NotNull,
            Self::Unique => CheckType::Unique,
            Self::AcceptedValues { .. } => CheckType::AcceptedValues,
            Self::Range { .. } => CheckType::Range,
            Self::Relationship { .. } => CheckType::Relationship,
            Self::Freshness { .. } => CheckType::Freshness,
            Self::CustomSql { .. } => CheckType::CustomSql,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityCheckDraft {
    pub project_id: String,
    pub name: String,
    pub target: QualityTarget,
    pub options: CheckOptions,
    pub null_policy: NullPolicy,
    pub severity: CheckSeverity,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityCheckDefinition {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub check_type: CheckType,
    pub target: QualityTarget,
    pub options: CheckOptions,
    pub null_policy: NullPolicy,
    pub severity: CheckSeverity,
    pub enabled: bool,
    pub latest_revision_id: String,
    pub revision_number: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // immutable revisions are resolved by the T3 execution coordinator
pub struct CheckRevision {
    pub id: String,
    pub check_id: String,
    pub revision_number: u32,
    pub definition: QualityCheckDraft,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckOutcome {
    Pass,
    Fail,
    Error,
    Cancelled,
}

impl CheckOutcome {
    #[allow(dead_code)] // used only when the T3 coordinator finalizes a run
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Error => "error",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> rusqlite::Result<Self> {
        match value {
            "pass" => Ok(Self::Pass),
            "fail" => Ok(Self::Fail),
            "error" => Ok(Self::Error),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckRun {
    pub id: String,
    pub project_id: String,
    pub check_id: String,
    pub revision_id: String,
    pub outcome: CheckOutcome,
    pub failure_count: Option<u64>,
    pub duration_ms: u64,
    pub observed_at: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // internal trusted input for T3; deliberately not exposed over Tauri
pub struct CheckRunDraft {
    pub id: String,
    pub project_id: String,
    pub check_id: String,
    pub revision_id: String,
    pub outcome: CheckOutcome,
    pub failure_count: Option<u64>,
    pub duration_ms: u64,
    pub observed_at: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckHistoryPage {
    pub entries: Vec<CheckRun>,
    pub offset: u32,
    pub next_offset: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityPruneSummary {
    pub deleted: u64,
    pub remaining: u64,
}

#[derive(Clone)]
pub struct QualityRepository {
    database: MetadataDb,
}

impl QualityRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn create(
        &self,
        draft: &QualityCheckDraft,
    ) -> Result<QualityCheckDefinition, MetadataError> {
        let draft = validate_draft(draft)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: u32 = transaction.query_row(
            "SELECT count(*) FROM quality_checks WHERE project_id = ?1",
            [&draft.project_id],
            |row| row.get(0),
        )?;
        if count >= MAX_CHECKS_PER_PROJECT {
            return Err(invalid("maximum checks per project reached"));
        }
        if name_exists(&transaction, &draft.project_id, &draft.name, None)? {
            return Err(MetadataError::QualityCheckConflict(draft.name));
        }
        let id = uuid::Uuid::new_v4().to_string();
        let revision_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let target_json = encode(&format!("quality-check:{id}:target"), &draft.target)?;
        let definition_json = encode(&format!("quality-check:{id}:revision"), &draft)?;
        transaction.execute(
            "INSERT INTO quality_checks(id, project_id, name, check_type, target_json, null_policy, severity, enabled, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
            (
                &id,
                &draft.project_id,
                &draft.name,
                draft.options.check_type().as_str(),
                target_json,
                draft.null_policy.as_str(),
                draft.severity.as_str(),
                draft.enabled,
                &now,
            ),
        )?;
        transaction.execute(
            "INSERT INTO quality_check_revisions(id, check_id, revision_number, definition_json, created_at)
             VALUES (?1, ?2, 1, ?3, ?4)",
            (&revision_id, &id, definition_json, &now),
        )?;
        transaction.commit()?;
        drop(connection);
        self.get(&draft.project_id, &id)?
            .ok_or(MetadataError::QualityCheckMissing)
    }

    pub fn update(
        &self,
        id: &str,
        draft: &QualityCheckDraft,
    ) -> Result<QualityCheckDefinition, MetadataError> {
        let draft = validate_draft(draft)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: Option<u32> = transaction
            .query_row(
                "SELECT max(revision_number) FROM quality_check_revisions WHERE check_id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let current = current.ok_or(MetadataError::QualityCheckMissing)?;
        if name_exists(&transaction, &draft.project_id, &draft.name, Some(id))? {
            return Err(MetadataError::QualityCheckConflict(draft.name));
        }
        let target_json = encode(&format!("quality-check:{id}:target"), &draft.target)?;
        let definition_json = encode(&format!("quality-check:{id}:revision"), &draft)?;
        let now = chrono::Utc::now().to_rfc3339();
        let updated = transaction.execute(
            "UPDATE quality_checks SET name = ?1, check_type = ?2, target_json = ?3,
             null_policy = ?4, severity = ?5, enabled = ?6, updated_at = ?7
             WHERE id = ?8 AND project_id = ?9",
            (
                &draft.name,
                draft.options.check_type().as_str(),
                target_json,
                draft.null_policy.as_str(),
                draft.severity.as_str(),
                draft.enabled,
                &now,
                id,
                &draft.project_id,
            ),
        )?;
        if updated == 0 {
            return Err(MetadataError::QualityCheckMissing);
        }
        transaction.execute(
            "INSERT INTO quality_check_revisions(id, check_id, revision_number, definition_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                uuid::Uuid::new_v4().to_string(),
                id,
                current + 1,
                definition_json,
                &now,
            ),
        )?;
        transaction.commit()?;
        drop(connection);
        self.get(&draft.project_id, id)?
            .ok_or(MetadataError::QualityCheckMissing)
    }

    pub fn get(
        &self,
        project_id: &str,
        id: &str,
    ) -> Result<Option<QualityCheckDefinition>, MetadataError> {
        let connection = self.database.connection()?;
        connection
            .query_row(
                &definition_select("WHERE q.project_id = ?1 AND q.id = ?2"),
                (project_id, id),
                read_definition,
            )
            .optional()
            .map_err(MetadataError::from)
    }

    pub fn list(&self, project_id: &str) -> Result<Vec<QualityCheckDefinition>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(&definition_select(
            "WHERE q.project_id = ?1 ORDER BY q.updated_at DESC, q.name, q.id",
        ))?;
        let definitions = statement
            .query_map([project_id], read_definition)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(definitions)
    }

    pub fn delete(&self, project_id: &str, id: &str) -> Result<bool, MetadataError> {
        let connection = self.database.connection()?;
        let has_history: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM quality_check_runs WHERE project_id = ?1 AND check_id = ?2)",
            (project_id, id),
            |row| row.get(0),
        )?;
        if has_history {
            return Err(MetadataError::QualityCheckHasHistory);
        }
        Ok(connection.execute(
            "DELETE FROM quality_checks WHERE project_id = ?1 AND id = ?2",
            (project_id, id),
        )? > 0)
    }

    #[allow(dead_code)] // used by the T3 immutable execution coordinator
    pub fn revision(
        &self,
        project_id: &str,
        revision_id: &str,
    ) -> Result<Option<CheckRevision>, MetadataError> {
        let connection = self.database.connection()?;
        connection
            .query_row(
                "SELECT r.id, r.check_id, r.revision_number, r.definition_json, r.created_at
                 FROM quality_check_revisions r JOIN quality_checks q ON q.id = r.check_id
                 WHERE q.project_id = ?1 AND r.id = ?2",
                (project_id, revision_id),
                read_revision,
            )
            .optional()
            .map_err(MetadataError::from)
    }

    #[allow(dead_code)] // used by the T3 exactly-once terminal finalizer
    pub fn add_run(&self, draft: &CheckRunDraft) -> Result<CheckRun, MetadataError> {
        validate_run(draft)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let owns_revision: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM quality_checks q
             JOIN quality_check_revisions r ON r.check_id = q.id
             WHERE q.project_id = ?1 AND q.id = ?2 AND r.id = ?3)",
            (&draft.project_id, &draft.check_id, &draft.revision_id),
            |row| row.get(0),
        )?;
        if !owns_revision {
            return Err(MetadataError::QualityRevisionMissing);
        }
        let created_at = chrono::Utc::now().to_rfc3339();
        transaction.execute(
            "INSERT INTO quality_check_runs(id, project_id, check_id, revision_id, outcome,
             failure_count, duration_ms, observed_at, error_code, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO NOTHING",
            (
                &draft.id,
                &draft.project_id,
                &draft.check_id,
                &draft.revision_id,
                draft.outcome.as_str(),
                draft.failure_count,
                draft.duration_ms,
                &draft.observed_at,
                &draft.error_code,
                &created_at,
            ),
        )?;
        let stored = transaction.query_row(
            "SELECT id, project_id, check_id, revision_id, outcome, failure_count,
             duration_ms, observed_at, error_code, created_at FROM quality_check_runs
             WHERE project_id = ?1 AND id = ?2",
            (&draft.project_id, &draft.id),
            read_run,
        )?;
        if stored.check_id != draft.check_id
            || stored.revision_id != draft.revision_id
            || stored.outcome != draft.outcome
            || stored.failure_count != draft.failure_count
            || stored.duration_ms != draft.duration_ms
            || stored.observed_at != draft.observed_at
            || stored.error_code != draft.error_code
        {
            return Err(invalid(
                "quality run id already identifies different evidence",
            ));
        }
        prune_runs(&transaction, &draft.project_id, &draft.check_id)?;
        transaction.commit()?;
        Ok(stored)
    }

    pub fn history(
        &self,
        project_id: &str,
        check_id: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<CheckHistoryPage, MetadataError> {
        let limit = limit.clamp(1, 100);
        let fetch = limit + 1;
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, check_id, revision_id, outcome, failure_count,
             duration_ms, observed_at, error_code, created_at FROM quality_check_runs
             WHERE project_id = ?1 AND (?2 IS NULL OR check_id = ?2)
             ORDER BY observed_at DESC, id DESC LIMIT ?3 OFFSET ?4",
        )?;
        let mut entries = statement
            .query_map((project_id, check_id, fetch, offset), read_run)?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = entries.len() > limit as usize;
        entries.truncate(limit as usize);
        Ok(CheckHistoryPage {
            entries,
            offset,
            next_offset: has_more.then_some(offset.saturating_add(limit)),
        })
    }

    pub fn latest_runs(&self, project_id: &str) -> Result<Vec<CheckRun>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, check_id, revision_id, outcome, failure_count,
             duration_ms, observed_at, error_code, created_at FROM (
               SELECT id, project_id, check_id, revision_id, outcome, failure_count,
                 duration_ms, observed_at, error_code, created_at,
                 row_number() OVER (
                   PARTITION BY check_id ORDER BY observed_at DESC, id DESC
                 ) AS latest_rank
               FROM quality_check_runs WHERE project_id = ?1
             ) WHERE latest_rank = 1
             ORDER BY observed_at DESC, id DESC LIMIT 200",
        )?;
        let entries = statement
            .query_map([project_id], read_run)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn clear_history(
        &self,
        project_id: &str,
        check_id: Option<&str>,
    ) -> Result<QualityPruneSummary, MetadataError> {
        let connection = self.database.connection()?;
        let deleted = connection.execute(
            "DELETE FROM quality_check_runs WHERE project_id = ?1 AND (?2 IS NULL OR check_id = ?2)",
            (project_id, check_id),
        )?;
        let remaining = connection.query_row(
            "SELECT count(*) FROM quality_check_runs WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )?;
        Ok(QualityPruneSummary {
            deleted: deleted as u64,
            remaining,
        })
    }

    #[allow(dead_code)]
    fn run(&self, project_id: &str, id: &str) -> Result<Option<CheckRun>, MetadataError> {
        self.database
            .connection()?
            .query_row(
                "SELECT id, project_id, check_id, revision_id, outcome, failure_count,
                 duration_ms, observed_at, error_code, created_at FROM quality_check_runs
                 WHERE project_id = ?1 AND id = ?2",
                (project_id, id),
                read_run,
            )
            .optional()
            .map_err(MetadataError::from)
    }
}

pub(crate) fn validate_draft(
    draft: &QualityCheckDraft,
) -> Result<QualityCheckDraft, MetadataError> {
    let mut draft = draft.clone();
    draft.project_id = required(&draft.project_id, "project id is empty")?;
    draft.name = required(&draft.name, "check name is empty")?;
    if draft.name.len() > MAX_CHECK_NAME_BYTES {
        return Err(invalid("check name is too long"));
    }
    normalize_target(&mut draft.target)?;
    if draft.options.check_type() == CheckType::NotEmpty && !draft.target.columns.is_empty() {
        return Err(invalid("not-empty check cannot target columns"));
    }
    match &mut draft.options {
        CheckOptions::NotEmpty => {}
        CheckOptions::NotNull | CheckOptions::Range { .. } | CheckOptions::Freshness { .. }
            if draft.target.columns.len() != 1 =>
        {
            return Err(invalid("check requires exactly one target column"));
        }
        CheckOptions::Unique if draft.target.columns.is_empty() => {
            return Err(invalid("unique check requires target columns"));
        }
        CheckOptions::AcceptedValues { values } => {
            if draft.target.columns.len() != 1 {
                return Err(invalid("accepted-values check requires one target column"));
            }
            if values.is_empty() || values.len() > MAX_ACCEPTED_VALUES {
                return Err(invalid("accepted-values entry count is out of range"));
            }
            if values.iter().any(|value| {
                value.is_null()
                    || matches!(
                        value,
                        serde_json::Value::Array(_) | serde_json::Value::Object(_)
                    )
            }) {
                return Err(invalid(
                    "accepted values must be non-NULL boolean, number, or text scalars",
                ));
            }
            let bytes =
                serde_json::to_vec(values).map_err(|source| MetadataError::InvalidJson {
                    key: "quality-check:accepted-values".into(),
                    source,
                })?;
            if bytes.len() > MAX_ACCEPTED_VALUE_BYTES {
                return Err(invalid("accepted values are too large"));
            }
        }
        CheckOptions::Range {
            minimum, maximum, ..
        } => {
            if minimum.is_none() && maximum.is_none() {
                return Err(invalid("range requires a minimum, maximum, or both"));
            }
            if minimum.iter().chain(maximum.iter()).any(|value| {
                value.is_null()
                    || matches!(
                        value,
                        serde_json::Value::Array(_) | serde_json::Value::Object(_)
                    )
            }) {
                return Err(invalid(
                    "range bounds must be non-NULL boolean, number, or text scalars",
                ));
            }
        }
        CheckOptions::Relationship {
            parent,
            parent_columns,
        } => {
            normalize_target(parent)?;
            *parent_columns = normalize_columns(parent_columns)?;
            if draft.target.columns.is_empty() || draft.target.columns.len() != parent_columns.len()
            {
                return Err(invalid("relationship key arity does not match"));
            }
        }
        CheckOptions::Freshness {
            maximum_age_seconds,
        } if *maximum_age_seconds == 0 => {
            return Err(invalid("freshness age must be positive"));
        }
        CheckOptions::CustomSql { sql } => {
            *sql = sql.trim().to_string();
            if sql.is_empty() || sql.len() > MAX_CUSTOM_SQL_BYTES {
                return Err(invalid("custom SQL size is out of range"));
            }
            validate_custom_sql(sql)?;
        }
        _ => {}
    }
    Ok(draft)
}

#[allow(dead_code)]
fn validate_custom_sql(sql: &str) -> Result<(), MetadataError> {
    let statements = split_sql_statements(sql);
    if statements.len() != 1 {
        return Err(invalid("custom SQL must contain exactly one statement"));
    }
    let tokens = sql_tokens(&statements[0]);
    if !matches!(tokens.first().map(String::as_str), Some("select" | "with")) {
        return Err(invalid("custom SQL must be a read-only SELECT"));
    }
    const FORBIDDEN: &[&str] = &[
        "insert",
        "update",
        "delete",
        "merge",
        "create",
        "alter",
        "drop",
        "truncate",
        "copy",
        "attach",
        "detach",
        "install",
        "load",
        "pragma",
        "call",
        "set",
        "reset",
        "vacuum",
        "checkpoint",
        "export",
        "import",
        "read_csv",
        "read_csv_auto",
        "read_parquet",
        "parquet_scan",
        "sqlite_scan",
        "postgres_scan",
        "httpfs",
    ];
    if tokens
        .iter()
        .any(|token| FORBIDDEN.contains(&token.as_str()))
    {
        return Err(invalid(
            "custom SQL contains a mutating or external operation",
        ));
    }
    Ok(())
}

fn split_sql_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(character) = chars.next() {
        if let Some(active) = quote {
            current.push(character);
            if character == active {
                if chars.peek() == Some(&active) {
                    current.push(chars.next().unwrap_or(active));
                } else {
                    quote = None;
                }
            }
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                current.push(character);
            }
            '-' if chars.peek() == Some(&'-') => {
                current.push(character);
                current.push(chars.next().unwrap_or('-'));
                for next in chars.by_ref() {
                    current.push(next);
                    if next == '\n' {
                        break;
                    }
                }
            }
            ';' => {
                if !current.trim().is_empty() {
                    statements.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if !current.trim().is_empty() {
        statements.push(current.trim().to_string());
    }
    statements
}

fn sql_tokens(sql: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut chars = sql.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '\'' | '"' => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
                let quote = character;
                while let Some(next) = chars.next() {
                    if next == quote {
                        if chars.peek() == Some(&quote) {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
            }
            '-' if chars.peek() == Some(&'-') => {
                chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            character if character.is_ascii_alphanumeric() || character == '_' => {
                token.push(character.to_ascii_lowercase());
            }
            _ if !token.is_empty() => tokens.push(std::mem::take(&mut token)),
            _ => {}
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

#[allow(dead_code)]
fn validate_run(run: &CheckRunDraft) -> Result<(), MetadataError> {
    if run.id.trim().is_empty()
        || run.project_id.trim().is_empty()
        || run.check_id.trim().is_empty()
        || run.revision_id.trim().is_empty()
        || run.observed_at.trim().is_empty()
    {
        return Err(invalid("quality run identity is incomplete"));
    }
    if run.outcome == CheckOutcome::Fail && run.failure_count.is_none() {
        return Err(invalid("failed quality run requires a failure count"));
    }
    if run.outcome == CheckOutcome::Pass && run.failure_count != Some(0) {
        return Err(invalid("passed quality run requires zero failures"));
    }
    Ok(())
}

fn normalize_target(target: &mut QualityTarget) -> Result<(), MetadataError> {
    target.database = required(&target.database, "target database is empty")?;
    target.schema = required(&target.schema, "target schema is empty")?;
    target.object = required(&target.object, "target object is empty")?;
    target.columns = normalize_columns(&target.columns)?;
    Ok(())
}

fn normalize_columns(columns: &[String]) -> Result<Vec<String>, MetadataError> {
    if columns.len() > MAX_COMPOSITE_COLUMNS {
        return Err(invalid("too many composite key columns"));
    }
    let normalized = columns
        .iter()
        .map(|column| required(column, "target column is empty"))
        .collect::<Result<Vec<_>, _>>()?;
    for (index, column) in normalized.iter().enumerate() {
        if normalized[..index]
            .iter()
            .any(|previous| previous.eq_ignore_ascii_case(column))
        {
            return Err(invalid("target columns must be unique"));
        }
    }
    Ok(normalized)
}

fn required(value: &str, message: &str) -> Result<String, MetadataError> {
    let value = value.trim();
    if value.is_empty() {
        Err(invalid(message))
    } else {
        Ok(value.to_string())
    }
}

fn invalid(message: &str) -> MetadataError {
    MetadataError::InvalidQualityCheck(message.into())
}

fn encode<T: Serialize>(key: &str, value: &T) -> Result<String, MetadataError> {
    serde_json::to_string(value).map_err(|source| MetadataError::InvalidJson {
        key: key.into(),
        source,
    })
}

fn decode<T: for<'de> Deserialize<'de>>(key: &str, value: &str) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            value.len(),
            rusqlite::types::Type::Text,
            Box::new(MetadataError::InvalidJson {
                key: key.into(),
                source: error,
            }),
        )
    })
}

fn definition_select(clause: &str) -> String {
    format!(
        "SELECT q.id, q.project_id, q.name, q.check_type, q.target_json, q.null_policy,
         q.severity, q.enabled, r.id, r.revision_number, r.definition_json,
         q.created_at, q.updated_at FROM quality_checks q
         JOIN quality_check_revisions r ON r.check_id = q.id
          AND r.revision_number = (SELECT max(x.revision_number) FROM quality_check_revisions x WHERE x.check_id = q.id)
         {clause}"
    )
}

fn read_definition(row: &rusqlite::Row<'_>) -> rusqlite::Result<QualityCheckDefinition> {
    let id: String = row.get(0)?;
    let draft: QualityCheckDraft = decode(
        &format!("quality-check:{id}:revision"),
        &row.get::<_, String>(10)?,
    )?;
    let stored_type = CheckType::parse(&row.get::<_, String>(3)?)?;
    if stored_type != draft.options.check_type() {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(QualityCheckDefinition {
        id,
        project_id: row.get(1)?,
        name: row.get(2)?,
        check_type: stored_type,
        target: decode("quality-check:target", &row.get::<_, String>(4)?)?,
        options: draft.options,
        null_policy: NullPolicy::parse(&row.get::<_, String>(5)?)?,
        severity: CheckSeverity::parse(&row.get::<_, String>(6)?)?,
        enabled: row.get(7)?,
        latest_revision_id: row.get(8)?,
        revision_number: row.get(9)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

#[allow(dead_code)]
fn read_revision(row: &rusqlite::Row<'_>) -> rusqlite::Result<CheckRevision> {
    let id: String = row.get(0)?;
    Ok(CheckRevision {
        id: id.clone(),
        check_id: row.get(1)?,
        revision_number: row.get(2)?,
        definition: decode(&format!("quality-revision:{id}"), &row.get::<_, String>(3)?)?,
        created_at: row.get(4)?,
    })
}

fn read_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<CheckRun> {
    Ok(CheckRun {
        id: row.get(0)?,
        project_id: row.get(1)?,
        check_id: row.get(2)?,
        revision_id: row.get(3)?,
        outcome: CheckOutcome::parse(&row.get::<_, String>(4)?)?,
        failure_count: row.get(5)?,
        duration_ms: row.get(6)?,
        observed_at: row.get(7)?,
        error_code: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn name_exists(
    connection: &rusqlite::Connection,
    project_id: &str,
    name: &str,
    except_id: Option<&str>,
) -> Result<bool, MetadataError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM quality_checks WHERE project_id = ?1
         AND name = ?2 COLLATE NOCASE AND (?3 IS NULL OR id <> ?3))",
        (project_id, name, except_id),
        |row| row.get(0),
    )?)
}

#[allow(dead_code)]
fn prune_runs(
    transaction: &rusqlite::Transaction<'_>,
    project_id: &str,
    check_id: &str,
) -> Result<(), MetadataError> {
    transaction.execute(
        "DELETE FROM quality_check_runs WHERE check_id = ?1 AND id NOT IN (
          SELECT id FROM quality_check_runs WHERE check_id = ?1 ORDER BY observed_at DESC, id DESC LIMIT ?2
         )",
        (check_id, MAX_RUNS_PER_CHECK),
    )?;
    transaction.execute(
        "DELETE FROM quality_check_runs WHERE project_id = ?1 AND id NOT IN (
          SELECT id FROM quality_check_runs WHERE project_id = ?1 ORDER BY observed_at DESC, id DESC LIMIT ?2
         )",
        (project_id, MAX_RUNS_PER_PROJECT),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::metadata::projects::{ProjectOwnership, ProjectsRepository};

    fn project(database: &MetadataDb, name: &str) -> String {
        ProjectsRepository::new(database.clone())
            .upsert(
                name,
                Path::new(&format!("/data/{name}.duckdb")),
                ProjectOwnership::External,
            )
            .unwrap()
            .id
    }

    fn draft(project_id: &str, name: &str) -> QualityCheckDraft {
        QualityCheckDraft {
            project_id: project_id.into(),
            name: name.into(),
            target: QualityTarget {
                database: "project".into(),
                schema: "main".into(),
                object: "orders".into(),
                columns: vec!["order_id".into()],
            },
            options: CheckOptions::NotNull,
            null_policy: NullPolicy::FailOnNull,
            severity: CheckSeverity::Warning,
            enabled: true,
        }
    }

    #[test]
    fn crud_creates_immutable_revisions_and_isolates_projects() {
        let database = MetadataDb::open_in_memory().unwrap();
        let first = project(&database, "first");
        let second = project(&database, "second");
        let repository = QualityRepository::new(database.clone());
        let created = repository
            .create(&draft(&first, "Order id required"))
            .unwrap();
        assert_eq!(created.revision_number, 1);
        assert!(repository.get(&second, &created.id).unwrap().is_none());
        let revision_one = created.latest_revision_id.clone();
        let mut update = draft(&first, "Order id required");
        update.severity = CheckSeverity::Critical;
        let updated = repository.update(&created.id, &update).unwrap();
        assert_eq!(updated.revision_number, 2);
        assert_eq!(
            repository
                .revision(&first, &revision_one)
                .unwrap()
                .unwrap()
                .definition
                .severity,
            CheckSeverity::Warning
        );
        assert!(repository.delete(&first, &created.id).unwrap());
        assert!(repository
            .revision(&first, &revision_one)
            .unwrap()
            .is_none());
    }

    #[test]
    fn limits_conflicts_and_malformed_json_fail_closed() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project_id = project(&database, "limits");
        let repository = QualityRepository::new(database.clone());
        let created = repository.create(&draft(&project_id, "Required")).unwrap();
        assert!(matches!(
            repository.create(&draft(&project_id, "required")),
            Err(MetadataError::QualityCheckConflict(_))
        ));
        let mut oversized = draft(&project_id, "Accepted");
        oversized.options = CheckOptions::AcceptedValues {
            values: (0..=MAX_ACCEPTED_VALUES)
                .map(serde_json::Value::from)
                .collect(),
        };
        assert!(matches!(
            repository.create(&oversized),
            Err(MetadataError::InvalidQualityCheck(_))
        ));
        database
            .connection()
            .unwrap()
            .execute(
                "UPDATE quality_check_revisions SET definition_json = '{broken'",
                [],
            )
            .unwrap_err();
        database
            .connection()
            .unwrap()
            .execute(
                "UPDATE quality_check_revisions SET definition_json = '{}' WHERE id = ?1",
                [&created.latest_revision_id],
            )
            .unwrap();
        assert!(repository.get(&project_id, &created.id).is_err());
    }

    #[test]
    fn definitions_and_revisions_survive_disk_reopen() {
        let root =
            std::env::temp_dir().join(format!("tarik-quality-reopen-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("quality.sqlite");
        let (project_id, check_id, revision_id) = {
            let database = MetadataDb::open(&path).unwrap();
            let project_id = project(&database, "reopen");
            let check = QualityRepository::new(database)
                .create(&draft(&project_id, "Required 日本語"))
                .unwrap();
            (project_id, check.id, check.latest_revision_id)
        };
        let reopened = MetadataDb::open(&path).unwrap();
        let repository = QualityRepository::new(reopened);
        let check = repository.get(&project_id, &check_id).unwrap().unwrap();
        assert_eq!(check.name, "Required 日本語");
        assert_eq!(
            repository
                .revision(&project_id, &revision_id)
                .unwrap()
                .unwrap()
                .definition
                .name,
            "Required 日本語"
        );
        drop(repository);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_sql_rejects_multiple_mutating_and_external_statements() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project_id = project(&database, "custom");
        let repository = QualityRepository::new(database);
        for sql in [
            "SELECT 1; SELECT 2",
            "DELETE FROM orders",
            "WITH changed AS (DELETE FROM orders RETURNING *) SELECT * FROM changed",
            "SELECT * FROM read_parquet('secret.parquet')",
            "COPY orders TO 'outside.csv'",
        ] {
            let mut candidate = draft(&project_id, "Custom");
            candidate.target.columns.clear();
            candidate.options = CheckOptions::CustomSql { sql: sql.into() };
            assert!(matches!(
                repository.create(&candidate),
                Err(MetadataError::InvalidQualityCheck(_))
            ));
        }
        let mut accepted = draft(&project_id, "Visible SQL");
        accepted.target.columns.clear();
        accepted.options = CheckOptions::CustomSql {
            sql: "WITH failures AS (SELECT 1 AS id WHERE false) SELECT * FROM failures".into(),
        };
        assert!(repository.create(&accepted).is_ok());
    }

    #[test]
    fn run_history_references_revision_is_idempotent_bounded_and_clear_isolated() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project_id = project(&database, "runs");
        let repository = QualityRepository::new(database.clone());
        let check = repository.create(&draft(&project_id, "Required")).unwrap();
        for index in 0..=MAX_RUNS_PER_CHECK {
            let run = CheckRunDraft {
                id: format!("run-{index:03}"),
                project_id: project_id.clone(),
                check_id: check.id.clone(),
                revision_id: check.latest_revision_id.clone(),
                outcome: CheckOutcome::Fail,
                failure_count: Some(index as u64 + 1),
                duration_ms: 10,
                observed_at: format!("2026-01-01T00:{:02}:00Z", index % 60),
                error_code: None,
            };
            repository.add_run(&run).unwrap();
            repository.add_run(&run).unwrap();
            if index == 0 {
                let mut conflicting = run.clone();
                conflicting.failure_count = Some(999);
                assert!(matches!(
                    repository.add_run(&conflicting),
                    Err(MetadataError::InvalidQualityCheck(_))
                ));
            }
        }
        let page = repository
            .history(&project_id, Some(&check.id), 0, 100)
            .unwrap();
        assert_eq!(page.entries.len(), MAX_RUNS_PER_CHECK as usize);
        assert!(matches!(
            repository.delete(&project_id, &check.id),
            Err(MetadataError::QualityCheckHasHistory)
        ));
        let summary = repository
            .clear_history(&project_id, Some(&check.id))
            .unwrap();
        assert_eq!(summary.deleted, MAX_RUNS_PER_CHECK as u64);
        assert_eq!(repository.list(&project_id).unwrap().len(), 1);
        assert!(repository.delete(&project_id, &check.id).unwrap());
    }

    #[test]
    fn deleting_project_cascades_definitions_revisions_and_runs() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project_id = project(&database, "cascade");
        let repository = QualityRepository::new(database.clone());
        let check = repository.create(&draft(&project_id, "Required")).unwrap();
        repository
            .add_run(&CheckRunDraft {
                id: "run".into(),
                project_id: project_id.clone(),
                check_id: check.id,
                revision_id: check.latest_revision_id,
                outcome: CheckOutcome::Pass,
                failure_count: Some(0),
                duration_ms: 1,
                observed_at: "2026-01-01T00:00:00Z".into(),
                error_code: None,
            })
            .unwrap();
        ProjectsRepository::new(database.clone())
            .remove(&project_id)
            .unwrap();
        let connection = database.connection().unwrap();
        for table in [
            "quality_checks",
            "quality_check_revisions",
            "quality_check_runs",
        ] {
            let count: u64 = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
        }
    }
}
