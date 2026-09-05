use std::{
    collections::HashMap,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use duckdb::{types::Value as DuckValue, Connection};
use serde_json::{Number, Value};
use tarik_engine_protocol::{
    catalog_revision, ErrorEnvelope, MetricProvenance, ProfileColumn, ProfileMetric,
    ProfileMetricKind, ProfileMode, ProfileRequest, ProfileSnapshot, ProfileState, ProfileStatus,
    ProfileTarget, MAX_PROFILE_SNAPSHOT_BYTES, MAX_PROFILE_VALUES, MAX_PROFILE_VALUE_BYTES,
};

use crate::{catalog, error::EngineError, sources::quote_identifier};

const TERMINAL_PROFILE_LIMIT: usize = 32;
const PROFILE_DEADLINE_SECS: u64 = 300;
const JS_SAFE_MAX: i128 = 9_007_199_254_740_991;

struct ProfileRecord {
    session_id: String,
    request: ProfileRequest,
    connection: Option<Connection>,
    state: ProfileState,
    queued_at: Instant,
    started_at: Option<Instant>,
    duration_ms: Option<u64>,
    interrupt: Option<Arc<duckdb::InterruptHandle>>,
    cancel_requested: bool,
    snapshot: Option<ProfileSnapshot>,
    error: Option<ErrorEnvelope>,
}

impl ProfileRecord {
    fn status(&self, profile_id: &str) -> ProfileStatus {
        ProfileStatus {
            profile_id: profile_id.to_string(),
            state: self.state,
            duration_ms: self.duration_ms.unwrap_or_else(|| {
                self.started_at
                    .unwrap_or(self.queued_at)
                    .elapsed()
                    .as_millis() as u64
            }),
            snapshot: self.snapshot.clone(),
            error: self.error.clone(),
        }
    }
}

#[derive(Default)]
struct Registry {
    profiles: HashMap<String, ProfileRecord>,
    terminal_order: std::collections::VecDeque<String>,
}

pub struct ProfileRegistry {
    inner: Mutex<Registry>,
}

impl ProfileRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Registry::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Registry>, EngineError> {
        self.inner.lock().map_err(|_| EngineError::RegistryPoisoned)
    }

    pub fn execute(
        self: &Arc<Self>,
        session_id: &str,
        profile_id: &str,
        request: ProfileRequest,
        connection: Connection,
    ) -> Result<(), EngineError> {
        request
            .validate()
            .map_err(|error| EngineError::InvalidProfile(error.to_string()))?;
        let mut inner = self.lock()?;
        if inner.profiles.contains_key(profile_id) {
            return Err(EngineError::ProfileExists(profile_id.to_string()));
        }
        inner.profiles.insert(
            profile_id.to_string(),
            ProfileRecord {
                session_id: session_id.to_string(),
                request,
                connection: Some(connection),
                state: ProfileState::Queued,
                queued_at: Instant::now(),
                started_at: None,
                duration_ms: None,
                interrupt: None,
                cancel_requested: false,
                snapshot: None,
                error: None,
            },
        );
        drop(inner);

        let registry = Arc::clone(self);
        let id = profile_id.to_string();
        if let Err(error) = std::thread::Builder::new()
            .name(format!("tarik-profile-{profile_id}"))
            .spawn(move || run_profile(registry, &id))
        {
            self.lock()?.profiles.remove(profile_id);
            return Err(EngineError::WorkerSpawn(error.to_string()));
        }
        Ok(())
    }

    pub fn status(&self, profile_id: &str) -> Result<ProfileStatus, EngineError> {
        self.lock()?
            .profiles
            .get(profile_id)
            .map(|record| record.status(profile_id))
            .ok_or_else(|| EngineError::ProfileMissing(profile_id.to_string()))
    }

    pub fn cancel(&self, profile_id: &str) -> Result<ProfileStatus, EngineError> {
        let mut remember = false;
        {
            let mut inner = self.lock()?;
            let record = inner
                .profiles
                .get_mut(profile_id)
                .ok_or_else(|| EngineError::ProfileMissing(profile_id.to_string()))?;
            match record.state {
                ProfileState::Queued => {
                    record.cancel_requested = true;
                    record.state = ProfileState::Cancelled;
                    record.duration_ms = Some(record.queued_at.elapsed().as_millis() as u64);
                    record.connection.take();
                    remember = true;
                }
                ProfileState::Running => {
                    record.cancel_requested = true;
                    if let Some(interrupt) = record.interrupt.as_ref() {
                        interrupt.interrupt();
                    }
                }
                ProfileState::Succeeded | ProfileState::Failed | ProfileState::Cancelled => {}
            }
        }
        if remember {
            self.remember_terminal(profile_id);
        }
        self.status(profile_id)
    }

    pub fn cancel_session(&self, session_id: &str) {
        let ids = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .profiles
                    .iter()
                    .filter(|(_, record)| record.session_id == session_id)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.cancel(&id);
        }
    }

    pub fn has_active_session(&self, session_id: &str) -> Result<bool, EngineError> {
        Ok(self.lock()?.profiles.values().any(|record| {
            record.session_id == session_id
                && matches!(record.state, ProfileState::Queued | ProfileState::Running)
        }))
    }

    fn terminal(
        &self,
        profile_id: &str,
        state: ProfileState,
        snapshot: Option<ProfileSnapshot>,
        error: Option<ErrorEnvelope>,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(record) = inner.profiles.get_mut(profile_id) {
                if matches!(
                    record.state,
                    ProfileState::Succeeded | ProfileState::Failed | ProfileState::Cancelled
                ) {
                    return;
                }
                record.duration_ms = Some(
                    record
                        .started_at
                        .unwrap_or(record.queued_at)
                        .elapsed()
                        .as_millis() as u64,
                );
                record.state = state;
                record.snapshot = snapshot;
                record.error = error;
                record.interrupt = None;
                record.connection = None;
            }
        }
        self.remember_terminal(profile_id);
    }

    fn remember_terminal(&self, profile_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            if !inner.terminal_order.iter().any(|id| id == profile_id) {
                inner.terminal_order.push_back(profile_id.to_string());
            }
            while inner.terminal_order.len() > TERMINAL_PROFILE_LIMIT {
                if let Some(oldest) = inner.terminal_order.pop_front() {
                    inner.profiles.remove(&oldest);
                }
            }
        }
    }

    fn cancel_requested(&self, profile_id: &str) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| {
                inner
                    .profiles
                    .get(profile_id)
                    .map(|record| record.cancel_requested)
            })
            .unwrap_or(false)
    }
}

fn run_profile(registry: Arc<ProfileRegistry>, profile_id: &str) {
    let claimed = {
        let mut inner = match registry.lock() {
            Ok(inner) => inner,
            Err(_) => return,
        };
        let Some(record) = inner.profiles.get_mut(profile_id) else {
            return;
        };
        if record.cancel_requested || record.state != ProfileState::Queued {
            return;
        }
        record.state = ProfileState::Running;
        record.started_at = Some(Instant::now());
        let connection = record.connection.take();
        let request = record.request.clone();
        if let Some(connection) = connection.as_ref() {
            record.interrupt = Some(connection.interrupt_handle());
        }
        (connection, request)
    };

    let (Some(connection), request) = claimed else {
        registry.terminal(
            profile_id,
            ProfileState::Failed,
            None,
            Some(ErrorEnvelope::new(
                "profile.rejected",
                "engine lost profile work before it started",
            )),
        );
        return;
    };
    let deadline_handle = connection.interrupt_handle();
    let (deadline_done, deadline_wait) = std::sync::mpsc::channel();
    let deadline_expired = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let deadline_expired_worker = Arc::clone(&deadline_expired);
    let _ = std::thread::Builder::new()
        .name(format!("tarik-profile-deadline-{profile_id}"))
        .spawn(move || {
            if deadline_wait
                .recv_timeout(std::time::Duration::from_secs(PROFILE_DEADLINE_SECS))
                .is_err()
            {
                deadline_expired_worker.store(true, std::sync::atomic::Ordering::Release);
                deadline_handle.interrupt();
            }
        });
    let result = catch_unwind(AssertUnwindSafe(|| execute_profile(&connection, request)));
    let _ = deadline_done.send(());
    match result {
        Ok(Ok(snapshot)) => {
            registry.terminal(profile_id, ProfileState::Succeeded, Some(snapshot), None)
        }
        Ok(Err(error)) => {
            if registry.cancel_requested(profile_id) {
                registry.terminal(profile_id, ProfileState::Cancelled, None, None);
            } else if deadline_expired.load(std::sync::atomic::Ordering::Acquire) {
                let error = EngineError::ProfileDeadline;
                registry.terminal(
                    profile_id,
                    ProfileState::Failed,
                    None,
                    Some(ErrorEnvelope::new(error.code(), error.to_string())),
                );
            } else {
                registry.terminal(
                    profile_id,
                    ProfileState::Failed,
                    None,
                    Some(ErrorEnvelope::new(error.code(), error.to_string())),
                );
            }
        }
        Err(_) if registry.cancel_requested(profile_id) => {
            registry.terminal(profile_id, ProfileState::Cancelled, None, None)
        }
        Err(_) if deadline_expired.load(std::sync::atomic::Ordering::Acquire) => {
            let error = EngineError::ProfileDeadline;
            registry.terminal(
                profile_id,
                ProfileState::Failed,
                None,
                Some(ErrorEnvelope::new(error.code(), error.to_string())),
            )
        }
        Err(_) => registry.terminal(
            profile_id,
            ProfileState::Failed,
            None,
            Some(ErrorEnvelope::new(
                "profile.interrupted",
                "profile fetch was interrupted",
            )),
        ),
    }
}

pub fn execute_profile(
    connection: &Connection,
    request: ProfileRequest,
) -> Result<ProfileSnapshot, EngineError> {
    let current = catalog::inspect(connection)?;
    if current.revision != request.catalog_revision {
        return Err(EngineError::ProfileCatalogStale);
    }
    let object = current.objects.iter().find(|object| {
        object.database == request.target.database
            && object.schema == request.target.schema
            && object.name == request.target.name
            && object.kind == request.target.kind
    });
    if object.is_none() {
        return Err(EngineError::ProfileCatalogStale);
    }
    for requested in &request.columns {
        let matches = current.columns.iter().any(|column| {
            column.database == request.target.database
                && column.schema == request.target.schema
                && column.object == request.target.name
                && column.name == requested.name
                && column.data_type == requested.data_type
        });
        if !matches {
            return Err(EngineError::ProfileCatalogStale);
        }
    }

    let qualified = qualified_target(&request.target)?;
    let mut metrics = vec![ProfileMetric {
        column: None,
        kind: ProfileMetricKind::RowCount,
        value: Some(query_value(
            connection,
            &format!("SELECT count(*) FROM {qualified}"),
        )?),
        provenance: MetricProvenance::Exact,
        unavailable_reason: None,
        truncated: false,
    }];
    let row_count = metric_u64(metrics[0].value.as_ref());

    for column in &request.columns {
        profile_column(
            connection,
            &qualified,
            column,
            request.mode,
            row_count,
            &mut metrics,
        )?;
    }

    enforce_snapshot_budget(&mut metrics)?;

    // Detect a catalog change that occurred while the multi-statement profile ran.
    let after = catalog::inspect(connection)?;
    if catalog_revision(&after.objects, &after.columns) != request.catalog_revision {
        return Err(EngineError::ProfileCatalogStale);
    }
    Ok(ProfileSnapshot {
        project_id: request.project_id,
        target: request.target,
        catalog_revision: request.catalog_revision,
        mode: request.mode,
        observed_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        metrics,
    })
}

fn profile_column(
    connection: &Connection,
    qualified: &str,
    column: &ProfileColumn,
    mode: ProfileMode,
    row_count: u64,
    metrics: &mut Vec<ProfileMetric>,
) -> Result<(), EngineError> {
    let identifier = quote_identifier(&column.name)?;
    let null_value = query_value(
        connection,
        &format!("SELECT count(*) FILTER (WHERE {identifier} IS NULL) FROM {qualified}"),
    )?;
    let null_count = metric_u64(Some(&null_value));
    push_metric(
        metrics,
        column,
        ProfileMetricKind::NullCount,
        Some(null_value),
        MetricProvenance::Exact,
    );
    push_metric(
        metrics,
        column,
        ProfileMetricKind::NullRate,
        Some(if row_count == 0 {
            Value::Null
        } else {
            Value::Number(
                Number::from_f64(null_count as f64 / row_count as f64)
                    .unwrap_or_else(|| Number::from(0)),
            )
        }),
        MetricProvenance::Exact,
    );

    let type_upper = column.data_type.to_ascii_uppercase();
    if supports_distinct(&type_upper) {
        let exact_distinct = mode == ProfileMode::Exact || type_upper == "BOOLEAN";
        let distinct = if exact_distinct {
            format!("count(DISTINCT {identifier})")
        } else {
            format!("approx_count_distinct({identifier})")
        };
        push_metric(
            metrics,
            column,
            ProfileMetricKind::DistinctCount,
            Some(query_value(
                connection,
                &format!("SELECT {distinct} FROM {qualified}"),
            )?),
            if exact_distinct {
                MetricProvenance::Exact
            } else {
                MetricProvenance::Approximate
            },
        );
    } else {
        unavailable(
            metrics,
            column,
            ProfileMetricKind::DistinctCount,
            "Distinct count is unavailable for this column type.",
        );
    }
    if is_numeric(&type_upper) {
        for (kind, expression) in [
            (ProfileMetricKind::Minimum, format!("min({identifier})")),
            (ProfileMetricKind::Maximum, format!("max({identifier})")),
            (ProfileMetricKind::Average, format!("avg({identifier})")),
        ] {
            push_metric(
                metrics,
                column,
                kind,
                Some(query_display(connection, qualified, &expression)?),
                MetricProvenance::Exact,
            );
        }
        unavailable_text_lengths(metrics, column);
    } else if is_text(&type_upper) {
        for (kind, expression) in [
            (
                ProfileMetricKind::TextLengthMinimum,
                format!("min(length({identifier}))"),
            ),
            (
                ProfileMetricKind::TextLengthMaximum,
                format!("max(length({identifier}))"),
            ),
            (
                ProfileMetricKind::TextLengthAverage,
                format!("avg(length({identifier}))"),
            ),
        ] {
            push_metric(
                metrics,
                column,
                kind,
                Some(query_value(
                    connection,
                    &format!("SELECT {expression} FROM {qualified}"),
                )?),
                MetricProvenance::Exact,
            );
        }
        unavailable_range(
            metrics,
            column,
            "Numeric/temporal summaries are unavailable for text columns.",
        );
    } else if is_temporal(&type_upper) {
        for (kind, expression) in [
            (ProfileMetricKind::Minimum, format!("min({identifier})")),
            (ProfileMetricKind::Maximum, format!("max({identifier})")),
        ] {
            push_metric(
                metrics,
                column,
                kind,
                Some(query_display(connection, qualified, &expression)?),
                MetricProvenance::Exact,
            );
        }
        unavailable(
            metrics,
            column,
            ProfileMetricKind::Average,
            "Average is unavailable for temporal columns.",
        );
        unavailable_text_lengths(metrics, column);
    } else {
        unavailable_range(
            metrics,
            column,
            "Numeric/temporal summaries are unavailable for this column type.",
        );
        unavailable_text_lengths(metrics, column);
    }

    if supports_value_lists(&type_upper) {
        let (common, common_truncated) = query_common_values(connection, qualified, &identifier)?;
        push_metric(
            metrics,
            column,
            ProfileMetricKind::CommonValues,
            Some(Value::Array(common)),
            MetricProvenance::Exact,
        );
        if let Some(metric) = metrics.last_mut() {
            metric.truncated = common_truncated;
        }
    } else {
        unavailable(
            metrics,
            column,
            ProfileMetricKind::CommonValues,
            "Common values are unavailable for this column type.",
        );
    }
    let (representative, representative_truncated) =
        query_representative_values(connection, qualified, &identifier)?;
    push_metric(
        metrics,
        column,
        ProfileMetricKind::RepresentativeValues,
        Some(Value::Array(representative)),
        MetricProvenance::Sampled,
    );
    if let Some(metric) = metrics.last_mut() {
        metric.truncated = representative_truncated;
    }
    Ok(())
}

fn enforce_snapshot_budget(metrics: &mut [ProfileMetric]) -> Result<(), EngineError> {
    if serde_json::to_vec(&*metrics)?.len() <= MAX_PROFILE_SNAPSHOT_BYTES {
        return Ok(());
    }
    for index in (0..metrics.len()).rev() {
        if matches!(
            metrics[index].kind,
            ProfileMetricKind::RepresentativeValues | ProfileMetricKind::CommonValues
        ) && metrics[index].value.is_some()
        {
            metrics[index].value = None;
            metrics[index].unavailable_reason = Some(
                "Value list omitted to keep the profile response within its bounded payload."
                    .into(),
            );
            metrics[index].truncated = true;
            if serde_json::to_vec(&*metrics)?.len() <= MAX_PROFILE_SNAPSHOT_BYTES {
                return Ok(());
            }
        }
    }
    Err(EngineError::InvalidProfile(
        "profile summary exceeds the bounded response size".into(),
    ))
}

fn query_common_values(
    connection: &Connection,
    qualified: &str,
    identifier: &str,
) -> Result<(Vec<Value>, bool), EngineError> {
    let safe_chars = MAX_PROFILE_VALUE_BYTES / 4;
    let sql = format!(
        "SELECT left(CAST({identifier} AS VARCHAR), {safe_chars}) AS value, count(*) AS frequency, length(CAST({identifier} AS VARCHAR)) > {safe_chars} AS truncated FROM {qualified} WHERE {identifier} IS NOT NULL GROUP BY {identifier} ORDER BY frequency DESC, value ASC LIMIT {MAX_PROFILE_VALUES}"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        let value: String = row.get(0)?;
        let frequency: i64 = row.get(1)?;
        let truncated: bool = row.get(2)?;
        Ok((
            Value::Object(
                [
                    ("value".into(), Value::String(value)),
                    ("count".into(), Value::from(frequency.max(0))),
                ]
                .into_iter()
                .collect(),
            ),
            truncated,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let truncated = rows.iter().any(|(_, truncated)| *truncated);
    Ok((
        rows.into_iter().map(|(value, _)| value).collect(),
        truncated,
    ))
}

fn query_representative_values(
    connection: &Connection,
    qualified: &str,
    identifier: &str,
) -> Result<(Vec<Value>, bool), EngineError> {
    let safe_chars = MAX_PROFILE_VALUE_BYTES / 4;
    let sql = format!(
        "SELECT left(CAST({identifier} AS VARCHAR), {safe_chars}), length(CAST({identifier} AS VARCHAR)) > {safe_chars} FROM {qualified} WHERE {identifier} IS NOT NULL LIMIT {MAX_PROFILE_VALUES}"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok((
            Value::String(row.get::<_, String>(0)?),
            row.get::<_, bool>(1)?,
        ))
    })?;
    let rows = rows.collect::<Result<Vec<_>, _>>()?;
    let truncated = rows.iter().any(|(_, truncated)| *truncated);
    Ok((
        rows.into_iter().map(|(value, _)| value).collect(),
        truncated,
    ))
}

fn query_display(
    connection: &Connection,
    qualified: &str,
    expression: &str,
) -> Result<Value, EngineError> {
    query_value(
        connection,
        &format!("SELECT CAST({expression} AS VARCHAR) FROM {qualified}"),
    )
}

fn query_value(connection: &Connection, sql: &str) -> Result<Value, EngineError> {
    let value: DuckValue = connection.query_row(sql, [], |row| row.get(0))?;
    Ok(safe_value(value))
}

fn safe_value(value: DuckValue) -> Value {
    match value {
        DuckValue::Null => Value::Null,
        DuckValue::Boolean(value) => Value::Bool(value),
        DuckValue::TinyInt(value) => Value::from(value),
        DuckValue::SmallInt(value) => Value::from(value),
        DuckValue::Int(value) => Value::from(value),
        DuckValue::BigInt(value) => safe_signed(i128::from(value)),
        DuckValue::HugeInt(value) => safe_signed(value),
        DuckValue::UTinyInt(value) => Value::from(value),
        DuckValue::USmallInt(value) => Value::from(value),
        DuckValue::UInt(value) => Value::from(value),
        DuckValue::UBigInt(value) => safe_unsigned(u128::from(value)),
        DuckValue::UHugeInt(value) => safe_unsigned(value),
        DuckValue::Float(value) => safe_float(f64::from(value)),
        DuckValue::Double(value) => safe_float(value),
        DuckValue::Text(value) | DuckValue::Enum(value) => Value::String(truncate(value)),
        other => Value::String(truncate(format!("{other:?}"))),
    }
}

fn safe_signed(value: i128) -> Value {
    if value.abs() > JS_SAFE_MAX {
        Value::String(value.to_string())
    } else {
        Value::from(value as i64)
    }
}

fn safe_unsigned(value: u128) -> Value {
    if value > JS_SAFE_MAX as u128 {
        Value::String(value.to_string())
    } else {
        Value::from(value as u64)
    }
}

fn safe_float(value: f64) -> Value {
    Number::from_f64(value).map_or_else(
        || {
            Value::String(
                if value.is_nan() {
                    "NaN"
                } else if value.is_sign_positive() {
                    "Infinity"
                } else {
                    "-Infinity"
                }
                .into(),
            )
        },
        Value::Number,
    )
}

fn truncate(mut value: String) -> String {
    if value.len() <= MAX_PROFILE_VALUE_BYTES {
        return value;
    }
    let mut end = MAX_PROFILE_VALUE_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value
}

fn push_metric(
    metrics: &mut Vec<ProfileMetric>,
    column: &ProfileColumn,
    kind: ProfileMetricKind,
    value: Option<Value>,
    provenance: MetricProvenance,
) {
    metrics.push(ProfileMetric {
        column: Some(column.name.clone()),
        kind,
        value,
        provenance,
        unavailable_reason: None,
        truncated: false,
    });
}

fn unavailable(
    metrics: &mut Vec<ProfileMetric>,
    column: &ProfileColumn,
    kind: ProfileMetricKind,
    reason: &str,
) {
    metrics.push(ProfileMetric {
        column: Some(column.name.clone()),
        kind,
        value: None,
        provenance: MetricProvenance::Exact,
        unavailable_reason: Some(reason.into()),
        truncated: false,
    });
}

fn unavailable_range(metrics: &mut Vec<ProfileMetric>, column: &ProfileColumn, reason: &str) {
    for kind in [
        ProfileMetricKind::Minimum,
        ProfileMetricKind::Maximum,
        ProfileMetricKind::Average,
    ] {
        unavailable(metrics, column, kind, reason);
    }
}

fn unavailable_text_lengths(metrics: &mut Vec<ProfileMetric>, column: &ProfileColumn) {
    for kind in [
        ProfileMetricKind::TextLengthMinimum,
        ProfileMetricKind::TextLengthMaximum,
        ProfileMetricKind::TextLengthAverage,
    ] {
        unavailable(
            metrics,
            column,
            kind,
            "Text-length metrics are available only for text columns.",
        );
    }
}

fn metric_u64(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(0)
}

fn qualified_target(target: &ProfileTarget) -> Result<String, EngineError> {
    Ok(format!(
        "{}.{}.{}",
        quote_identifier(&target.database)?,
        quote_identifier(&target.schema)?,
        quote_identifier(&target.name)?
    ))
}

fn is_numeric(data_type: &str) -> bool {
    !is_complex(data_type)
        && [
            "TINYINT",
            "SMALLINT",
            "INTEGER",
            "BIGINT",
            "HUGEINT",
            "UTINYINT",
            "USMALLINT",
            "UINTEGER",
            "UBIGINT",
            "UHUGEINT",
            "FLOAT",
            "REAL",
            "DOUBLE",
            "DECIMAL",
            "NUMERIC",
        ]
        .iter()
        .any(|prefix| data_type.starts_with(prefix))
}

fn is_complex(data_type: &str) -> bool {
    data_type.contains('[')
        || data_type.starts_with("LIST")
        || data_type.starts_with("STRUCT")
        || data_type.starts_with("MAP")
        || data_type.starts_with("UNION")
        || data_type.starts_with("BLOB")
        || data_type.starts_with("BIT")
}

fn is_text(data_type: &str) -> bool {
    ["VARCHAR", "CHAR", "TEXT", "STRING"]
        .iter()
        .any(|prefix| data_type.starts_with(prefix))
}

fn is_temporal(data_type: &str) -> bool {
    ["DATE", "TIME", "TIMESTAMP"]
        .iter()
        .any(|prefix| data_type.starts_with(prefix))
}

fn supports_value_lists(data_type: &str) -> bool {
    is_numeric(data_type) || is_text(data_type) || is_temporal(data_type) || data_type == "BOOLEAN"
}

fn supports_distinct(data_type: &str) -> bool {
    supports_value_lists(data_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(revision: String) -> ProfileRequest {
        ProfileRequest {
            project_id: "project-1".into(),
            target: ProfileTarget {
                database: "memory".into(),
                schema: "main".into(),
                name: "odd table".into(),
                kind: "table".into(),
            },
            columns: vec![
                ProfileColumn {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                },
                ProfileColumn {
                    name: "label".into(),
                    data_type: "VARCHAR".into(),
                },
            ],
            catalog_revision: revision,
            mode: ProfileMode::Approximate,
        }
    }

    #[test]
    fn bounded_profile_marks_provenance_and_values() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE \"odd table\"(id INTEGER, label VARCHAR); INSERT INTO \"odd table\" VALUES (1, 'one'), (2, NULL), (2, 'two');").unwrap();
        let revision = catalog::inspect(&connection).unwrap().revision;
        let snapshot = execute_profile(&connection, request(revision)).unwrap();
        assert_eq!(snapshot.metrics[0].value, Some(Value::from(3)));
        let distinct = snapshot
            .metrics
            .iter()
            .find(|metric| {
                metric.column.as_deref() == Some("id")
                    && metric.kind == ProfileMetricKind::DistinctCount
            })
            .unwrap();
        assert_eq!(distinct.provenance, MetricProvenance::Approximate);
        let representatives = snapshot
            .metrics
            .iter()
            .find(|metric| {
                metric.column.as_deref() == Some("label")
                    && metric.kind == ProfileMetricKind::RepresentativeValues
            })
            .unwrap();
        assert_eq!(representatives.provenance, MetricProvenance::Sampled);
        assert!(
            representatives
                .value
                .as_ref()
                .unwrap()
                .as_array()
                .unwrap()
                .len()
                <= MAX_PROFILE_VALUES
        );
    }

    #[test]
    fn complex_types_report_unavailable_metrics_instead_of_zero() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE \"odd table\"(id INTEGER, label INTEGER[]); INSERT INTO \"odd table\" VALUES (1, [1, 2]);")
            .unwrap();
        let revision = catalog::inspect(&connection).unwrap().revision;
        let mut request = request(revision);
        request.columns = vec![ProfileColumn {
            name: "label".into(),
            data_type: "INTEGER[]".into(),
        }];
        let snapshot = execute_profile(&connection, request).unwrap();
        for kind in [
            ProfileMetricKind::DistinctCount,
            ProfileMetricKind::CommonValues,
        ] {
            let metric = snapshot
                .metrics
                .iter()
                .find(|metric| metric.kind == kind)
                .unwrap();
            assert_eq!(metric.value, None);
            assert!(metric.unavailable_reason.is_some());
        }
        let representative = snapshot
            .metrics
            .iter()
            .find(|metric| metric.kind == ProfileMetricKind::RepresentativeValues)
            .unwrap();
        assert_eq!(representative.provenance, MetricProvenance::Sampled);
        assert!(representative.value.is_some());
    }

    #[test]
    fn stale_catalog_and_quoted_identity_are_enforced() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE \"odd table\"(id INTEGER, label VARCHAR);")
            .unwrap();
        let revision = catalog::inspect(&connection).unwrap().revision;
        connection
            .execute_batch("ALTER TABLE \"odd table\" ADD COLUMN changed INTEGER")
            .unwrap();
        assert!(matches!(
            execute_profile(&connection, request(revision)),
            Err(EngineError::ProfileCatalogStale)
        ));
    }
}
