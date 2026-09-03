export type ThemePreference = "system" | "light" | "dark";
export type OutputPanel = "results" | "flow";

export interface WorkbenchPreferences {
  theme: ThemePreference;
  sidebarWidth: number;
  bottomPanelHeight: number;
  sidebarOpen: boolean;
  bottomPanelOpen: boolean;
  activeOutputPanel: OutputPanel;
}

export const defaultWorkbenchPreferences: WorkbenchPreferences = {
  theme: "system",
  sidebarWidth: 260,
  bottomPanelHeight: 292,
  sidebarOpen: true,
  bottomPanelOpen: true,
  activeOutputPanel: "results",
};

export interface WorkbenchPreferencesRepository {
  load(): Promise<WorkbenchPreferences>;
  save(preferences: WorkbenchPreferences): Promise<void>;
}

export type WorkbenchPreferencesLoader = () => Promise<WorkbenchPreferences | null>;
export type WorkbenchPreferencesSaver = (preferences: WorkbenchPreferences) => Promise<void>;

export function createWorkbenchPreferencesRepository(
  loadPreferences: WorkbenchPreferencesLoader,
  savePreferences: WorkbenchPreferencesSaver,
): WorkbenchPreferencesRepository {
  return {
    async load() {
      return normalizeWorkbenchPreferences(await loadPreferences());
    },
    save(preferences) {
      return savePreferences(normalizeWorkbenchPreferences(preferences));
    },
  };
}

export function normalizeWorkbenchPreferences(
  value: Partial<WorkbenchPreferences> | null | undefined,
): WorkbenchPreferences {
  const theme = value?.theme;
  const activeOutputPanel = value?.activeOutputPanel;

  return {
    theme: theme === "light" || theme === "dark" || theme === "system" ? theme : "system",
    sidebarWidth: clampNumber(value?.sidebarWidth, 220, 360, 260),
    bottomPanelHeight: clampNumber(value?.bottomPanelHeight, 180, 560, 292),
    sidebarOpen: typeof value?.sidebarOpen === "boolean" ? value.sidebarOpen : true,
    bottomPanelOpen: typeof value?.bottomPanelOpen === "boolean" ? value.bottomPanelOpen : true,
    activeOutputPanel:
      activeOutputPanel === "flow" || activeOutputPanel === "results"
        ? activeOutputPanel
        : "results",
  };
}

function clampNumber(value: unknown, minimum: number, maximum: number, fallback: number) {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(minimum, value))
    : fallback;
}
