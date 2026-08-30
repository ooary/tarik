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
