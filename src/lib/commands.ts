import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
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

export function removeLinkedSource(
  sourceId: string,
  invokeCommand: InvokeCommand = invoke,
): Promise<boolean> {
  return invokeCommand<boolean>("remove_linked_source", { sourceId });
}
