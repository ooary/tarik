import { createRef } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadQuerySession, saveQuerySession } from "../../lib/commands";
import { QueryWorkspace, type QueryWorkspaceHandle } from "./QueryWorkspace";

vi.mock("../../lib/commands", () => ({
  loadQuerySession: vi.fn(),
  saveQuerySession: vi.fn(),
}));

const catalog = {
  objects: [
    {
      database: "local",
      schema: "main",
      name: "orders",
      kind: "table" as const,
      estimatedRowCount: 100,
    },
  ],
  columns: [],
};

describe("QueryWorkspace", () => {
  beforeEach(() => {
    vi.mocked(loadQuerySession).mockResolvedValue(null);
    vi.mocked(saveQuerySession).mockResolvedValue(undefined);
  });

  it("adds tabs and exposes SQL insertion/preview actions", async () => {
    const ref = createRef<QueryWorkspaceHandle>();
    const { container } = render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
        ref={ref}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New query tab" }));
    expect(screen.getAllByRole("tab", { name: /Untitled/ })).toHaveLength(2);

    act(() => ref.current?.openPreview('SELECT * FROM "main"."orders" LIMIT 100;'));
    expect(screen.getAllByRole("tab", { name: /Untitled/ })).toHaveLength(3);
    expect(container.querySelector(".cm-content")).toHaveTextContent("SELECT *");

    act(() => ref.current?.insertSql('"main"."orders"'));
    expect(container.querySelector(".cm-content")).toHaveTextContent('"main"."orders"');
  });

  it("supports tab rename, duplicate, move, and close from context menu", async () => {
    vi.spyOn(window, "prompt").mockReturnValue("Revenue query");
    render(
      <QueryWorkspace
        activePanel="results"
        bottomOpen
        bottomPanelHeight={292}
        catalog={catalog}
        onSetBottomHeight={vi.fn()}
        onToggleBottom={vi.fn()}
        onUpdatePanel={vi.fn()}
        projectId="p1"
      />,
    );
    const tab = screen.getByRole("tab", { name: /Untitled/ });

    fireEvent.contextMenu(tab);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Rename" }));
    expect(screen.getByRole("tab", { name: /Revenue query/ })).toBeInTheDocument();

    fireEvent.contextMenu(screen.getByRole("tab", { name: /Revenue query/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Duplicate" }));
    expect(screen.getAllByRole("tab")).toHaveLength(5); // 2 query tabs + 3 output tabs

    fireEvent.contextMenu(screen.getByRole("tab", { name: /Revenue query copy/ }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Close" }));
    expect(screen.queryByRole("tab", { name: /Revenue query copy/ })).not.toBeInTheDocument();
  });
});
