import { describe, expect, it } from "vitest";
import { defaultWorkbenchPreferences, normalizeWorkbenchPreferences } from "./preferences";

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
