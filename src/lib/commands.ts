import { invoke } from "@tauri-apps/api/core";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { WorkbenchPreferences } from "../app/preferences";

export interface RuntimeInfo {
  appName: string;
  appVersion: string;
  rustTarget: string;
}

export interface AppDirectories {
  dataDir: string;
  cacheDir: string;
  logDir: string;
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
  objects: CatalogObject[];
  columns: CatalogColumn[];
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

export interface SourceMutationResult {
  source: SourceRecord;
  inspection: SourceInspection;
}

export type InvokeCommand = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function getRuntimeInfo(invokeCommand: InvokeCommand = invoke): Promise<RuntimeInfo> {
  return invokeCommand<RuntimeInfo>("get_runtime_info");
}

export function getAppDirectories(invokeCommand: InvokeCommand = invoke): Promise<AppDirectories> {
  return invokeCommand<AppDirectories>("get_app_directories");
}

export function getLogInfo(invokeCommand: InvokeCommand = invoke): Promise<LogInfo> {
  return invokeCommand<LogInfo>("get_log_info");
}

export function revealLogDirectory(directory: string): Promise<void> {
  return revealItemInDir(directory);
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

export interface ExecutionError {
  code: string;
  message: string;
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

export function revealExportPart(path: string): Promise<void> {
  return revealItemInDir(path);
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
