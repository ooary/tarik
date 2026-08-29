import { describe, expect, it } from "vitest";
import {
  createWorkbenchPreferencesRepository,
  defaultWorkbenchPreferences,
  normalizeWorkbenchPreferences,
} from "./preferences";

describe("workbench preference boundary", () => {
  it("returns safe defaults for missing values", () => {
    expect(normalizeWorkbenchPreferences(undefined)).toEqual(defaultWorkbenchPreferences);
  });

  it("clamps dimensions and rejects invalid variants", () => {
    expect(
      normalizeWorkbenchPreferences({
        theme: "contrast" as never,
        sidebarWidth: 20,
        bottomPanelHeight: 900,
        activeOutputPanel: "messages" as never,
      }),
    ).toMatchObject({
      theme: "system",
      sidebarWidth: 220,
      bottomPanelHeight: 560,
      activeOutputPanel: "results",
    });
  });

  it("normalizes values at the repository boundary", async () => {
    const saved: unknown[] = [];
    const repository = createWorkbenchPreferencesRepository(
      async () => ({ ...defaultWorkbenchPreferences, sidebarWidth: 999 }),
      async (preferences) => {
        saved.push(preferences);
      },
    );

    expect((await repository.load()).sidebarWidth).toBe(360);
    await repository.save({ ...defaultWorkbenchPreferences, bottomPanelHeight: 20 });
    expect(saved).toEqual([{ ...defaultWorkbenchPreferences, bottomPanelHeight: 180 }]);
  });

  it("preserves valid preferences", () => {
    expect(
      normalizeWorkbenchPreferences({
        theme: "dark",
        sidebarWidth: 318,
        bottomPanelHeight: 340,
        sidebarOpen: false,
        bottomPanelOpen: true,
        activeOutputPanel: "flow",
      }),
    ).toEqual({
      theme: "dark",
      sidebarWidth: 318,
      bottomPanelHeight: 340,
      sidebarOpen: false,
      bottomPanelOpen: true,
      activeOutputPanel: "flow",
    });
  });
});
