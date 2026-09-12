import { invoke } from "@tauri-apps/api/core";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import type { WorkbenchPreferences } from "../app/preferences";

export interface RuntimeInfo {
  appName: string;
  appVersion: string;
  rustTarget: string;
}

export interface LogInfo {
  directory: string;
  activeFile: string;
  maxFileBytes: number;
  retainedFiles: number;
  available: boolean;
}

export interface SupportIncident {
  incidentId: string;
  summary: string;
  logDirectory: string;
  loggingSucceeded: boolean;
}

export interface FrontendIncidentInput {
  incidentId: string;
  kind: string;
  message: string;
}

export interface CleanupSummary {
  artifactsRemoved: number;
  bytesRemoved: number;
  exportBackupsRestored: number;
  warnings: string[];
}

export interface AgentProjectGrant {
  projectId: string;
  inspect: boolean;
  analyze: boolean;
  modifyWorkspace: boolean;
  modifyData: boolean;
}

export interface AgentClient {
  id: string;
  displayName: string;
  connected: boolean;
  lastConnectedAt: string | null;
  grants: AgentProjectGrant[];
}

export interface AgentPairingRequest {
  id: string;
  displayName: string;
  expiresInSeconds: number;
}

export interface AgentAccessStatus {
  enabled: boolean;
  endpointReady: boolean;
  pairedClients: AgentClient[];
  pendingPairings: AgentPairingRequest[];
  connectedClients: number;
}

export interface AgentAccessChange {
  enabled: boolean;
  endpointReady: boolean;
}

export type AgentHostKind =
  "claude_desktop" | "claude_code" | "codex" | "pi" | "cursor" | "vs_code" | "generic";

export type AgentSetupMethod = "official_cli" | "managed_json" | "guided";
export type AgentHostState =
  | "not_installed"
  | "unsupported"
  | "not_configured"
  | "configured"
  | "restart_required"
  | "repair_required"
  | "conflict";
export type AgentSetupOperation = "configure" | "repair" | "remove";

export interface AgentHostInstallation {
  kind: AgentHostKind;
  displayName: string;
  setupMethod: AgentSetupMethod;
  state: AgentHostState;
  version: string | null;
  detail: string;
  canConfigure: boolean;
  canRemove: boolean;
}

export interface AgentSetupStatus {
  platform: string;
  packagedServerReady: boolean;
  hosts: AgentHostInstallation[];
}

export interface AgentSetupPlan {
  planId: string;
  hostKind: AgentHostKind;
  hostName: string;
  operation: AgentSetupOperation;
  setupMethod: AgentSetupMethod;
  summary: string;
  commandPreview: string | null;
  configTarget: string | null;
  expiresInSeconds: number;
}

export interface AgentSetupResult {
  hostKind: AgentHostKind;
  operation: AgentSetupOperation;
  state: AgentHostState;
  message: string;
}

export type AgentSkillHost = "pi";
export type AgentSkillOperation = "install" | "remove";
export type AgentSkillState = "available" | "installed" | "conflict" | "unsupported";

export interface AgentSkillStatus {
  host: AgentSkillHost;
  hostName: string;
  state: AgentSkillState;
  detail: string;
  canInstall: boolean;
  canRemove: boolean;
}

export interface AgentSkillPlan {
  planId: string;
  host: AgentSkillHost;
  hostName: string;
  operation: AgentSkillOperation;
  summary: string;
  target: string;
  expiresInSeconds: number;
}

export interface AgentSkillResult {
  host: AgentSkillHost;
  operation: AgentSkillOperation;
  state: AgentSkillState;
  message: string;
}

export type AgentExportFormat = "csv" | "parquet";

export interface AgentExportDestination {
  destinationId: string;
  label: string;
  formats: AgentExportFormat[];
  maximumRowsPerPart: number;
  maximumTotalBytes: number;
  createNewOnly: true;
  enabled: boolean;
  ready: boolean;
  revision: number;
}

export interface AgentExportDestinationList {
  projectId: string;
  destinations: AgentExportDestination[];
}

export interface AgentExportDestinationPolicy {
  displayLabel: string;
  allowCsv: boolean;
  allowParquet: boolean;
  maximumRowsPerPart: number;
  maximumTotalBytes: number;
}

export interface AgentApproval {
  id: string;
  action: "sql" | "export";
  clientName: string;
  projectId: string;
  projectName: string;
  sql: string;
  decision: "approval_required" | "critical_confirmation";
  reasonCode: string;
  affectedObjects: string[];
  hasTopLevelFilter: boolean | null;
  snapshotHash: string;
  criticalPhrase: string | null;
  expiresInSeconds: number;
}

export interface ShutdownReport {
  phase: "complete";
  queriesCancelled: number;
  exportsCancelled: number;
  /** Added with profile jobs; optional only for legacy test fixtures. */
  profilesCancelled?: number;
  /** Added with quality jobs; optional only for legacy test fixtures. */
  qualityRunsCancelled?: number;
  resultsReleased: number;
  metadataCheckpointed: boolean;
  warnings: string[];
}

export interface ActiveProject {
  id: string;
  name: string;
  duckdbPath: string;
}

export type ProjectOwnership = "managed" | "external";

export interface RecentProject extends ActiveProject {
  ownership: ProjectOwnership;
  createdAt: string;
  lastOpenedAt: string;
}

export type ProjectRemoval = "deleted" | "forgotten";

export type CatalogObjectKind = "table" | "view";

export interface CatalogObject {
  database: string;
  schema: string;
  name: string;
  kind: CatalogObjectKind;
  estimatedRowCount: number | null;
}

export interface CatalogColumn {
  database: string;
  schema: string;
  object: string;
  name: string;
  dataType: string;
  position: number;
  nullable: boolean;
}

export interface ProjectCatalog {
  /** Always supplied by the backend; optional only for legacy component fixtures. */
  revision?: string;
  objects: CatalogObject[];
  columns: CatalogColumn[];
}

export type ProfileMode = "approximate" | "exact";
export type MetricProvenance = "exact" | "approximate" | "sampled";
export type ProfileState = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface ProfileTarget {
  database: string;
  schema: string;
  name: string;
  kind: CatalogObjectKind;
}

export interface ProfileColumn {
  name: string;
  dataType: string;
}

export interface ProfileRequest {
  projectId: string;
  target: ProfileTarget;
  columns: ProfileColumn[];
  catalogRevision: string;
  mode: ProfileMode;
}

export type ProfileMetricKind =
  | "row_count"
  | "null_count"
  | "null_rate"
  | "distinct_count"
  | "minimum"
  | "maximum"
  | "average"
  | "text_length_minimum"
  | "text_length_maximum"
  | "text_length_average"
  | "common_values"
  | "representative_values";

export interface ProfileMetric {
  column: string | null;
  kind: ProfileMetricKind;
  value: unknown | null;
  provenance: MetricProvenance;
  unavailableReason: string | null;
  truncated: boolean;
}

export interface ProfileSqlEvidence {
  columns: string[];
  metricKinds: ProfileMetricKind[];
  sql: string;
}

export interface ProfileSnapshot {
  projectId: string;
  target: ProfileTarget;
  catalogRevision: string;
  mode: ProfileMode;
  observedAtUnixMs: number;
  metrics: ProfileMetric[];
  statements: ProfileSqlEvidence[];
}

export interface ProfileStatus {
  profileId: string;
  state: ProfileState;
  durationMs: number;
  snapshot: ProfileSnapshot | null;
  error: { code: string; message: string } | null;
}

export type CreateTableColumnType =
  "BOOLEAN" | "INTEGER" | "BIGINT" | "DOUBLE" | "DECIMAL" | "VARCHAR" | "DATE" | "TIMESTAMP";

export interface CreateTableColumn {
  name: string;
  dataType: CreateTableColumnType;
  nullable: boolean;
}

export interface CreateTableDefinition {
  name: string;
  columns: CreateTableColumn[];
}

export type SourceFormat = "csv" | "parquet";
export type SourceKind = "duckdb_table" | "linked_parquet" | "linked_csv";
export type SourceState = "ready" | "missing" | "invalid_schema";

export interface CsvOptions {
  delimiter: string;
  hasHeader: boolean;
  nullValue: string | null;
  allVarchar: boolean;
}

export interface SourceColumn {
  name: string;
  dataType: string;
  nullable: boolean;
}

export interface SourceInspection {
  path: string;
  format: SourceFormat;
  suggestedName: string;
  fileSizeBytes: number;
  rowCount: number;
  rowCountExact: boolean;
  columns: SourceColumn[];
  previewRows: unknown[][];
  csvOptions: CsvOptions | null;
  warnings: string[];
}

export interface ColumnOverride {
  column: string;
  dataType: string;
}

export interface ImportOptions {
  tableName: string;
  csv: CsvOptions | null;
  columnOverrides: ColumnOverride[];
}

export interface SourceRecord {
  id: string;
  projectId: string;
  displayName: string;
  kind: SourceKind;
  state: SourceState;
  sourcePath: string | null;
  duckdbName: string;
  options: Record<string, unknown>;
  createdAt: string;
  updatedAt: string;
}

export interface SourceImportStatus {
  importId: string;
  state: "queued" | "running" | "succeeded" | "failed" | "cancelled" | "recovery_required";
  stage: "queued" | "validating" | "reading_and_writing" | "finalizing";
  durationMs: number;
  effectiveResources: { memoryLimitMib: number; threads: number };
  source: SourceRecord | null;
  error: { code: string; message: string } | null;
}

export function getSourceImportStatus(
  invokeCommand: InvokeCommand = invoke,
): Promise<SourceImportStatus | null> {
  return invokeCommand<SourceImportStatus | null>("get_source_import_status");
}

export interface SourceMutationResult {
  source: SourceRecord;
  inspection: SourceInspection;
}

export type InvokeCommand = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function getRuntimeInfo(invokeCommand: InvokeCommand = invoke): Promise<RuntimeInfo> {
  return invokeCommand<RuntimeInfo>("get_runtime_info");
}

export interface StartupStatus {
  ready: boolean;
  phase: "checking_local_storage" | "ready";
}

export function getStartupStatus(invokeCommand: InvokeCommand = invoke): Promise<StartupStatus> {
  return invokeCommand<StartupStatus>("get_startup_status");
}

export function getLogInfo(invokeCommand: InvokeCommand = invoke): Promise<LogInfo> {
  return invokeCommand<LogInfo>("get_log_info");
}

export function revealLogDirectory(invokeCommand: InvokeCommand = invoke): Promise<void> {
  return invokeCommand<void>("reveal_log_directory");
}

export function reportFrontendIncident(
  input: FrontendIncidentInput,
  invokeCommand: InvokeCommand = invoke,
): Promise<SupportIncident> {
  return invokeCommand<SupportIncident>("report_frontend_incident", { input });
}

export function getLastSupportIncident(
  invokeCommand: InvokeCommand = invoke,
): Promise<SupportIncident | null> {
  return invokeCommand<SupportIncident | null>("get_last_support_incident");
}

export function clearCache(invokeCommand: InvokeCommand = invoke): Promise<CleanupSummary> {
  return invokeCommand<CleanupSummary>("clear_cache");
}

export function getAgentAccessStatus(
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentAccessStatus> {
  return invokeCommand<AgentAccessStatus>("get_agent_access_status");
}

export function setAgentAccessEnabled(
  enabled: boolean,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentAccessChange> {
  return invokeCommand<AgentAccessChange>("set_agent_access_enabled", { enabled });
}

export function getAgentSetupStatus(
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentSetupStatus> {
  return invokeCommand<AgentSetupStatus>("get_agent_setup_status");
}

export function planAgentSetup(
  hostKind: AgentHostKind,
  operation: AgentSetupOperation,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentSetupPlan> {
  return invokeCommand<AgentSetupPlan>("plan_agent_setup", { hostKind, operation });
}

export function applyAgentSetup(
  planId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentSetupResult> {
  return invokeCommand<AgentSetupResult>("apply_agent_setup", { planId });
}

export function getAgentSkillStatus(
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentSkillStatus> {
  return invokeCommand<AgentSkillStatus>("get_agent_skill_status");
}

export function planAgentSkill(
  host: AgentSkillHost,
  operation: AgentSkillOperation,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentSkillPlan> {
  return invokeCommand<AgentSkillPlan>("plan_agent_skill", { host, operation });
}

export function applyAgentSkill(
  planId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentSkillResult> {
  return invokeCommand<AgentSkillResult>("apply_agent_skill", { planId });
}

export function chooseAgentExportDirectory(): Promise<string | null> {
  return openFileDialog({
    multiple: false,
    directory: true,
    title: "Choose delegated export folder",
  });
}

export function listAgentExportDestinations(
  clientId: string,
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentExportDestinationList> {
  return invokeCommand<AgentExportDestinationList>("list_agent_export_destinations", {
    clientId,
    projectId,
  });
}

export function createAgentExportDestination(
  clientId: string,
  projectId: string,
  selectedDirectory: string,
  policy: AgentExportDestinationPolicy,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentExportDestination> {
  return invokeCommand<AgentExportDestination>("create_agent_export_destination", {
    clientId,
    projectId,
    selectedDirectory,
    policy,
  });
}

export function updateAgentExportDestination(
  clientId: string,
  projectId: string,
  destinationId: string,
  policy: AgentExportDestinationPolicy,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentExportDestination> {
  return invokeCommand<AgentExportDestination>("update_agent_export_destination", {
    clientId,
    projectId,
    destinationId,
    policy,
  });
}

export function setAgentExportDestinationEnabled(
  clientId: string,
  projectId: string,
  destinationId: string,
  enabled: boolean,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentExportDestination> {
  return invokeCommand<AgentExportDestination>("set_agent_export_destination_enabled", {
    clientId,
    projectId,
    destinationId,
    enabled,
  });
}

export function repairAgentExportDestination(
  clientId: string,
  projectId: string,
  destinationId: string,
  selectedDirectory: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentExportDestination> {
  return invokeCommand<AgentExportDestination>("repair_agent_export_destination", {
    clientId,
    projectId,
    destinationId,
    selectedDirectory,
  });
}

export function revokeAgentExportDestination(
  clientId: string,
  projectId: string,
  destinationId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("revoke_agent_export_destination", {
    clientId,
    projectId,
    destinationId,
  });
}

export function approveAgentPairing(
  pairingId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<string> {
  return invokeCommand<string>("approve_agent_pairing", { pairingId });
}

export function denyAgentPairing(
  pairingId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("deny_agent_pairing", { pairingId });
}

export function setAgentProjectGrant(
  clientId: string,
  grant: AgentProjectGrant,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("set_agent_project_grant", { clientId, grant });
}

export function revokeAgentClient(
  clientId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("revoke_agent_client", { clientId });
}

export function listAgentApprovals(
  invokeCommand: InvokeCommand = invoke,
): Promise<AgentApproval[]> {
  return invokeCommand<AgentApproval[]>("list_agent_approvals");
}

export function decideAgentApproval(
  approvalId: string,
  approve: boolean,
  typedPhrase: string | null,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("decide_agent_approval", {
    approvalId,
    approve,
    typedPhrase,
  });
}

export function registerShutdownReady(invokeCommand: InvokeCommand = invoke): Promise<void> {
  return invokeCommand<void>("register_shutdown_ready");
}

export function completeShutdown(
  skipDraft = false,
  invokeCommand: InvokeCommand = invoke,
): Promise<ShutdownReport> {
  return invokeCommand<ShutdownReport>("complete_shutdown", { skipDraft });
}

export function getWorkbenchPreferences(
  invokeCommand: InvokeCommand = invoke,
): Promise<WorkbenchPreferences | null> {
  return invokeCommand<WorkbenchPreferences | null>("get_workbench_preferences");
}

export function setWorkbenchPreferences(
  preferences: WorkbenchPreferences,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("set_workbench_preferences", { preferences });
}

export function createProject(
  name: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ActiveProject> {
  return invokeCommand<ActiveProject>("create_project", { name });
}

export function openProject(
  name: string,
  duckdbPath: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ActiveProject> {
  return invokeCommand<ActiveProject>("open_project", { name, duckdbPath });
}

export function reopenRecentProject(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ActiveProject> {
  return invokeCommand<ActiveProject>("reopen_recent_project", { projectId });
}

export function listRecentProjects(
  invokeCommand: InvokeCommand = invoke,
): Promise<RecentProject[]> {
  return invokeCommand<RecentProject[]>("list_recent_projects");
}

export function renameProject(
  projectId: string,
  newName: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<RecentProject> {
  return invokeCommand<RecentProject>("rename_project", { projectId, newName });
}

export function removeProject(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ProjectRemoval> {
  return invokeCommand<ProjectRemoval>("remove_project", { projectId });
}

export function createTable(
  projectId: string,
  definition: CreateTableDefinition,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("create_table", { projectId, definition });
}

export function chooseSourceFile(): Promise<string | null> {
  return openFileDialog({
    multiple: false,
    directory: false,
    title: "Choose CSV or Parquet source",
    filters: [
      { name: "Data source", extensions: ["csv", "parquet", "pq"] },
      { name: "CSV", extensions: ["csv"] },
      { name: "Parquet", extensions: ["parquet", "pq"] },
    ],
  });
}

export function chooseParquetFile(
  title = "Choose replacement Parquet file",
): Promise<string | null> {
  return openFileDialog({
    multiple: false,
    directory: false,
    title,
    filters: [{ name: "Parquet", extensions: ["parquet", "pq"] }],
  });
}

export function chooseDuckDbFile(): Promise<string | null> {
  return openFileDialog({
    multiple: false,
    directory: false,
    title: "Open DuckDB project",
    filters: [{ name: "DuckDB database", extensions: ["duckdb", "ddb", "db"] }],
  });
}

export function closeProject(invokeCommand: InvokeCommand = invoke): Promise<boolean> {
  return invokeCommand<boolean>("close_project");
}

export function getActiveProject(
  invokeCommand: InvokeCommand = invoke,
): Promise<ActiveProject | null> {
  return invokeCommand<ActiveProject | null>("get_active_project");
}

export function inspectProjectCatalog(
  invokeCommand: InvokeCommand = invoke,
): Promise<ProjectCatalog> {
  return invokeCommand<ProjectCatalog>("inspect_project_catalog");
}

export function executeProfile(
  request: ProfileRequest,
  invokeCommand: InvokeCommand = invoke,
): Promise<ProfileStatus> {
  return invokeCommand<ProfileStatus>("execute_profile", { request });
}

export function getProfileStatus(
  profileId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ProfileStatus | null> {
  return invokeCommand<ProfileStatus | null>("get_profile_status", { profileId });
}

export function cancelProfile(
  profileId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ProfileStatus | null> {
  return invokeCommand<ProfileStatus | null>("cancel_profile", { profileId });
}

export function inspectSourceFile(
  path: string,
  csv: CsvOptions | null = null,
  invokeCommand: InvokeCommand = invoke,
): Promise<SourceInspection> {
  return invokeCommand<SourceInspection>("inspect_source_file", { path, csv });
}

export function linkParquetSource(
  path: string,
  viewName: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<SourceMutationResult> {
  return invokeCommand<SourceMutationResult>("link_parquet_source", { path, viewName });
}

export function importSourceTable(
  path: string,
  options: ImportOptions,
  invokeCommand: InvokeCommand = invoke,
): Promise<SourceMutationResult> {
  return invokeCommand<SourceMutationResult>("import_source_table", { path, options });
}

export function listSources(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<SourceRecord[]> {
  return invokeCommand<SourceRecord[]>("list_sources", { projectId });
}

export function cancelSourceOperation(invokeCommand: InvokeCommand = invoke): Promise<boolean> {
  return invokeCommand<boolean>("cancel_source_operation");
}

export function repairLinkedSource(
  sourceId: string,
  replacement: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<SourceMutationResult> {
  return invokeCommand<SourceMutationResult>("repair_linked_source", { sourceId, replacement });
}

export function dropCatalogObject(
  projectId: string,
  databaseName: string,
  schema: string,
  name: string,
  kind: CatalogObjectKind,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("drop_catalog_object", {
    projectId,
    databaseName,
    schema,
    name,
    kind,
  });
}

export function removeLinkedSource(
  sourceId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("remove_linked_source", { sourceId });
}

export interface QueryTabSnapshot {
  id: string;
  title: string;
  sqlText: string;
  position: number;
  isActive: boolean;
}

export interface QuerySessionSnapshot {
  id: string;
  projectId: string;
  tabs: QueryTabSnapshot[];
}

export type ExecutionState = "queued" | "running" | "succeeded" | "failed" | "cancelled";
export type EngineRuntimeState = "stopped" | "standby" | "connected" | "failed";

export interface EngineStatus {
  state: EngineRuntimeState;
  processId: number | null;
}

export type EngineResourcePreset = "low_memory" | "balanced" | "fast" | "custom";

export interface EngineResourceSettings {
  preset: EngineResourcePreset;
  memoryLimitMib: number;
  threads: number;
}

export interface EffectiveEngineResources extends EngineResourceSettings {
  memoryLimitDisplay: string;
}

export interface EngineResourceStatus {
  requested: EngineResourceSettings;
  effective: EffectiveEngineResources | null;
  state: "pending" | "effective";
  logicalCpuCount: number | null;
  physicalMemoryMib: number | null;
  minimumMemoryMib: number;
  maximumMemoryMib: number;
  minimumThreads: number;
  maximumThreads: number;
}

export function getEngineStatus(invokeCommand: InvokeCommand = invoke): Promise<EngineStatus> {
  return invokeCommand<EngineStatus>("get_engine_status");
}

export function getEngineResources(
  invokeCommand: InvokeCommand = invoke,
): Promise<EngineResourceStatus> {
  return invokeCommand<EngineResourceStatus>("get_engine_resources");
}

export function setEngineResources(
  requested: EngineResourceSettings,
  invokeCommand: InvokeCommand = invoke,
): Promise<EngineResourceStatus> {
  return invokeCommand<EngineResourceStatus>("set_engine_resources", { requested });
}

export interface ExecutionError {
  code: string;
  message: string;
}

export interface EngineExecutionStatus {
  executionId: string;
  state: ExecutionState;
  durationMs: number;
  rowsProduced: number | null;
  rowsAffected: number | null;
  error: ExecutionError | null;
  result: {
    resultId: string;
    rowCount: number;
    rowCountExact: boolean;
  } | null;
}

export type PlanMode = "explain" | "profile";

export interface PlanNode {
  id: string;
  operator: string;
  nativeName: string;
  semantic?: {
    title: string;
    summary: string;
    inputLabel: string;
    outputLabel: string;
    sqlRange: { from: number; to: number } | null;
    conceptOnly: boolean;
  };
  source: string | null;
  estimatedRows: number | null;
  actualRows: number | null;
  timingMs: number | null;
  rowsScanned: number | null;
  details: Record<string, unknown>;
  presentationNote?: string;
}

export interface PlanEdge {
  id: string;
  source: string;
  target: string;
}

export interface QueryPlan {
  mode: PlanMode;
  nodes: PlanNode[];
  edges: PlanEdge[];
  rootIds: string[];
  rawPlan: string;
  fallbackReason: string | null;
}

export function explainQueryPlan(
  projectId: string,
  sql: string,
  mode: PlanMode,
  invokeCommand: InvokeCommand = invoke,
): Promise<QueryPlan> {
  return invokeCommand<QueryPlan>("explain_query_plan", { projectId, sql, mode });
}

export type SqlDiagnosticSeverity = "error" | "warning";

export interface SqlDiagnostic {
  code: string;
  message: string;
  severity: SqlDiagnosticSeverity;
  from: number | null;
  to: number | null;
}

export interface SqlValidation {
  revision: number;
  diagnostics: SqlDiagnostic[];
}

export function validateQuery(
  projectId: string,
  sql: string,
  revision: number,
  invokeCommand: InvokeCommand = invoke,
): Promise<SqlValidation> {
  return invokeCommand<SqlValidation>("validate_query", { projectId, sql, revision });
}

export function executeQuery(
  projectId: string,
  tabId: string,
  sql: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExecutionView> {
  return invokeCommand<ExecutionView>("execute_query", { projectId, tabId, sql });
}

export function getQueryStatus(
  executionId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExecutionView | null> {
  return invokeCommand<ExecutionView | null>("get_query_status", { executionId });
}

export function getTabExecution(
  projectId: string,
  tabId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExecutionView | null> {
  return invokeCommand<ExecutionView | null>("get_tab_execution", { projectId, tabId });
}

export function cancelQuery(
  executionId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExecutionView> {
  return invokeCommand<ExecutionView>("cancel_query", { executionId });
}

export interface ExecutionView {
  executionId: string;
  projectId: string;
  tabId: string;
  sql: string;
  state: ExecutionState;
  durationMs: number;
  rowsProduced: number | null;
  rowsAffected: number | null;
  error: ExecutionError | null;
  resultId: string | null;
  rowTotal: number | null;
}

export interface ResultPageView {
  resultId: string;
  offset: number;
  rowTotal: number;
  rowTotalExact: boolean;
  columns: unknown;
  rows: unknown[][];
  truncatedCells: [number, number][];
  cached: boolean;
}

export function forgetTabExecution(
  tabId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("forget_tab_execution", { tabId });
}

export function getResultPage(
  resultId: string,
  offset: number,
  invokeCommand: InvokeCommand = invoke,
): Promise<ResultPageView> {
  return invokeCommand<ResultPageView>("get_result_page", { resultId, offset });
}

export function releaseResult(
  resultId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("release_result", { resultId });
}

export function releaseAllResults(invokeCommand: InvokeCommand = invoke): Promise<void> {
  return invokeCommand<void>("release_all_results");
}

export function saveQuerySession(
  snapshot: QuerySessionSnapshot,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("save_query_session", { snapshot });
}

export function loadQuerySession(
  sessionId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QuerySessionSnapshot | null> {
  return invokeCommand<QuerySessionSnapshot | null>("load_query_session", { sessionId });
}

export interface SavedQuery {
  id: string;
  projectId: string;
  folderId: string | null;
  name: string;
  sqlText: string;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

export interface SavedQueryDraft {
  projectId: string;
  folderId: string | null;
  name: string;
  sqlText: string;
  tags: string[];
}

export interface QueryFolder {
  id: string;
  projectId: string;
  name: string;
  createdAt: string;
}

export function createSavedQuery(
  draft: SavedQueryDraft,
  invokeCommand: InvokeCommand = invoke,
): Promise<SavedQuery> {
  return invokeCommand<SavedQuery>("create_saved_query", { draft });
}

export function updateSavedQuery(
  id: string,
  draft: SavedQueryDraft,
  invokeCommand: InvokeCommand = invoke,
): Promise<SavedQuery> {
  return invokeCommand<SavedQuery>("update_saved_query", { id, draft });
}

export function listSavedQueries(
  projectId: string,
  search: string | null = null,
  invokeCommand: InvokeCommand = invoke,
): Promise<SavedQuery[]> {
  return invokeCommand<SavedQuery[]>("list_saved_queries", { projectId, search });
}

export function deleteSavedQuery(
  projectId: string,
  id: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("delete_saved_query", { projectId, id });
}

export function createQueryFolder(
  projectId: string,
  name: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QueryFolder> {
  return invokeCommand<QueryFolder>("create_query_folder", { projectId, name });
}

export function renameQueryFolder(
  projectId: string,
  id: string,
  name: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QueryFolder> {
  return invokeCommand<QueryFolder>("rename_query_folder", { projectId, id, name });
}

export function listQueryFolders(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QueryFolder[]> {
  return invokeCommand<QueryFolder[]>("list_query_folders", { projectId });
}

export function deleteQueryFolder(
  projectId: string,
  id: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("delete_query_folder", { projectId, id });
}

export type HistoryStatus = "succeeded" | "failed" | "cancelled";

export interface QueryHistoryEntry {
  id: string;
  projectId: string;
  sqlText: string;
  status: HistoryStatus;
  durationMs: number | null;
  returnedRows: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  executedAt: string;
}

export interface QueryHistoryFilter {
  status: HistoryStatus | null;
  search: string | null;
  executedFrom: string | null;
  executedTo: string | null;
  offset: number;
  limit: number;
}

export interface QueryHistoryPage {
  entries: QueryHistoryEntry[];
  offset: number;
  nextOffset: number | null;
}

export function listQueryHistoryPage(
  projectId: string,
  filter: QueryHistoryFilter,
  invokeCommand: InvokeCommand = invoke,
): Promise<QueryHistoryPage> {
  return invokeCommand<QueryHistoryPage>("list_query_history_page", { projectId, filter });
}

export interface HistoryRetentionPolicy {
  maxCount: number | null;
  maxAgeDays: number | null;
}

export interface HistoryPruneSummary {
  deleted: number;
  remaining: number;
}

export function applyQueryHistoryRetention(
  projectId: string,
  policy: HistoryRetentionPolicy,
  invokeCommand: InvokeCommand = invoke,
): Promise<HistoryPruneSummary> {
  return invokeCommand<HistoryPruneSummary>("apply_query_history_retention", {
    projectId,
    policy,
  });
}

export function clearQueryHistory(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<HistoryPruneSummary> {
  return invokeCommand<HistoryPruneSummary>("clear_query_history", { projectId });
}

export type QualityCheckType =
  | "not_empty"
  | "not_null"
  | "unique"
  | "accepted_values"
  | "range"
  | "relationship"
  | "freshness"
  | "custom_sql";
export type QualityNullPolicy = "fail_on_null" | "pass_on_null";
export type QualitySeverity = "info" | "warning" | "critical";
export type QualityOutcome = "pass" | "fail" | "error" | "cancelled";

export interface QualityTarget {
  database: string;
  schema: string;
  object: string;
  columns: string[];
}

export type QualityCheckOptions =
  | { kind: "not_empty" }
  | { kind: "not_null" }
  | { kind: "unique" }
  | { kind: "accepted_values"; values: unknown[] }
  | {
      kind: "range";
      minimum: unknown | null;
      maximum: unknown | null;
      inclusiveMinimum: boolean;
      inclusiveMaximum: boolean;
    }
  | { kind: "relationship"; parent: QualityTarget; parentColumns: string[] }
  | { kind: "freshness"; maximumAgeSeconds: number }
  | { kind: "custom_sql"; sql: string };

export interface QualityCheckDraft {
  projectId: string;
  name: string;
  target: QualityTarget;
  options: QualityCheckOptions;
  nullPolicy: QualityNullPolicy;
  severity: QualitySeverity;
  enabled: boolean;
}

export interface QualityCheckDefinition extends QualityCheckDraft {
  id: string;
  checkType: QualityCheckType;
  latestRevisionId: string;
  revisionNumber: number;
  createdAt: string;
  updatedAt: string;
}

export interface QualityCheckRun {
  id: string;
  projectId: string;
  checkId: string;
  revisionId: string;
  outcome: QualityOutcome;
  failureCount: number | null;
  durationMs: number;
  observedAt: string;
  errorCode: string | null;
  createdAt: string;
}

export interface QualityCheckHistoryPage {
  entries: QualityCheckRun[];
  offset: number;
  nextOffset: number | null;
}

export interface QualityPruneSummary {
  deleted: number;
  remaining: number;
}

export type QualityExecutionState =
  "queued" | "running" | "passed" | "failed" | "error" | "cancelled";

export interface CompiledQualityCheckPreview {
  countSql: string;
  failureSql: string;
  custom: boolean;
}

export interface QualityExecutionView {
  runId: string;
  projectId: string;
  checkId: string;
  revisionId: string;
  state: QualityExecutionState;
  failureCount: number | null;
  durationMs: number;
  sql: string;
  error: ExecutionError | null;
  observationScope: "per_check";
}

export interface QualityRunDetail {
  run: QualityCheckRun;
  checkName: string;
  revisionNumber: number;
  currentRevisionNumber: number;
  definition: QualityCheckDraft;
  countSql: string;
  failureSql: string;
  custom: boolean;
  isLatestRevision: boolean;
}

export interface QualityFailurePreview {
  resultId: string;
  projectId: string;
  revisionId: string;
  sql: string;
  state: ExecutionState;
}

export function previewQualityCheckSql(
  draft: QualityCheckDraft,
  invokeCommand: InvokeCommand = invoke,
): Promise<CompiledQualityCheckPreview> {
  return invokeCommand<CompiledQualityCheckPreview>("preview_quality_check_sql", { draft });
}

export function createQualityCheck(
  draft: QualityCheckDraft,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityCheckDefinition> {
  return invokeCommand<QualityCheckDefinition>("create_quality_check", { draft });
}

export function updateQualityCheck(
  id: string,
  draft: QualityCheckDraft,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityCheckDefinition> {
  return invokeCommand<QualityCheckDefinition>("update_quality_check", { id, draft });
}

export function listQualityChecks(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityCheckDefinition[]> {
  return invokeCommand<QualityCheckDefinition[]>("list_quality_checks", { projectId });
}

export function deleteQualityCheck(
  projectId: string,
  id: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("delete_quality_check", { projectId, id });
}

export function getQualityCheckHistory(
  projectId: string,
  checkId: string | null,
  offset: number,
  limit: number,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityCheckHistoryPage> {
  return invokeCommand<QualityCheckHistoryPage>("get_quality_check_history", {
    projectId,
    checkId,
    offset,
    limit,
  });
}

export function listLatestQualityRuns(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityCheckRun[]> {
  return invokeCommand<QualityCheckRun[]>("list_latest_quality_runs", { projectId });
}

export function clearQualityCheckHistory(
  projectId: string,
  checkId: string | null,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityPruneSummary> {
  return invokeCommand<QualityPruneSummary>("clear_quality_check_history", {
    projectId,
    checkId,
  });
}

export function runQualityCheck(
  projectId: string,
  checkId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityExecutionView> {
  return invokeCommand<QualityExecutionView>("run_quality_check", { projectId, checkId });
}

export function getQualityRunDetail(
  projectId: string,
  runId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityRunDetail> {
  return invokeCommand<QualityRunDetail>("get_quality_run_detail", { projectId, runId });
}

export function rerunQualityRevision(
  projectId: string,
  runId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityExecutionView> {
  return invokeCommand<QualityExecutionView>("rerun_quality_revision", { projectId, runId });
}

export function runQualitySuite(
  projectId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityExecutionView[]> {
  return invokeCommand<QualityExecutionView[]>("run_quality_suite", { projectId });
}

export function getQualityRunStatus(
  runId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityExecutionView | null> {
  return invokeCommand<QualityExecutionView | null>("get_quality_run_status", { runId });
}

export function cancelQualityRun(
  runId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityExecutionView> {
  return invokeCommand<QualityExecutionView>("cancel_quality_run", { runId });
}

export function startQualityFailurePreview(
  projectId: string,
  runId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<QualityFailurePreview> {
  return invokeCommand<QualityFailurePreview>("start_quality_failure_preview", {
    projectId,
    runId,
  });
}

export function releaseQualityFailurePreview(
  resultId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("release_quality_failure_preview", { resultId });
}

export function getQualityFailurePreviewStatus(
  resultId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<EngineExecutionStatus | null> {
  return invokeCommand<EngineExecutionStatus | null>("get_quality_failure_preview_status", {
    resultId,
  });
}

export function cancelQualityFailurePreview(
  resultId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<EngineExecutionStatus | null> {
  return invokeCommand<EngineExecutionStatus | null>("cancel_quality_failure_preview", {
    resultId,
  });
}

export type ExportFormat = "csv" | "parquet";
export type ExportOverwritePolicy = "fail_if_exists" | "replace";
export type ParquetCompression = "uncompressed" | "snappy" | "gzip" | "zstd";
export type ExportState = "queued" | "running" | "succeeded" | "failed" | "cancelled";

export interface ExportOptions {
  format: ExportFormat;
  outputDirectory: string;
  baseName: string;
  rowsPerPart: number;
  overwrite: ExportOverwritePolicy;
  csv: { delimiter: string; includeHeader: boolean } | null;
  parquet: { compression: ParquetCompression } | null;
}

export interface ExportPartSummary {
  partNumber: number;
  path: string;
  rows: number;
  bytes: number;
}

export interface ExportView {
  exportId: string;
  projectId: string;
  state: ExportState;
  durationMs: number;
  rowsWritten: number;
  filesWritten: number;
  bytesWritten: number;
  currentPart: number | null;
  completedParts: ExportPartSummary[];
  error: ExecutionError | null;
}

export function chooseExportDirectory(): Promise<string | null> {
  return openFileDialog({
    multiple: false,
    directory: true,
    title: "Choose export folder",
  });
}

export function revealExportPart(
  exportId: string,
  partNumber: number,
  invokeCommand: InvokeCommand = invoke,
): Promise<void> {
  return invokeCommand<void>("reveal_export_part", { exportId, partNumber });
}

export function executeExport(
  projectId: string,
  sql: string,
  options: ExportOptions,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExportView> {
  return invokeCommand<ExportView>("execute_export", { projectId, sql, options });
}

export function getExportStatus(
  exportId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExportView | null> {
  return invokeCommand<ExportView | null>("get_export_status", { exportId });
}

export function cancelExport(
  exportId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<ExportView> {
  return invokeCommand<ExportView>("cancel_export", { exportId });
}
