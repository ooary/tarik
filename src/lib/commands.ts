import { invoke } from "@tauri-apps/api/core";

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

export type InvokeCommand = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export function getRuntimeInfo(invokeCommand: InvokeCommand = invoke): Promise<RuntimeInfo> {
  return invokeCommand<RuntimeInfo>("get_runtime_info");
}

export function getAppDirectories(invokeCommand: InvokeCommand = invoke): Promise<AppDirectories> {
  return invokeCommand<AppDirectories>("get_app_directories");
}
