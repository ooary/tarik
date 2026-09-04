import { fireEvent, render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { buildSqlCompletionSchema } from "./sqlCompletion";
import { SqlEditor } from "./SqlEditor";

describe("SqlEditor", () => {
  it("renders editable SQL with CodeMirror and line numbers", () => {
    const { container } = render(
      <SqlEditor
        onChange={vi.fn()}
        value="SELECT 1;"
        tables={[{ schema: "main", label: "orders", columns: ["id", "amount"] }]}
      />,
    );

    expect(container.querySelector(".cm-editor")).toBeInTheDocument();
    expect(container.querySelector(".cm-gutters")).toBeInTheDocument();
    expect(container.querySelector(".cm-content")).toHaveTextContent("SELECT 1;");
    expect(container.querySelector(".cm-content")).toHaveAttribute("contenteditable", "true");
  });

  it("builds schema, table, and column completion data", () => {
    expect(
      buildSqlCompletionSchema([
        {
          schema: "analytics",
          label: "order lines",
          type: "table",
          columns: ["order id", "amount"],
        },
      ]),
    ).toEqual({ analytics: { "order lines": ["order id", "amount"] } });
  });

  it("opens project relation completion with Ctrl+Space", async () => {
    const { container } = render(
      <SqlEditor
        onChange={vi.fn()}
        value="SELECT * FROM data_"
        tables={[{ schema: "main", label: "data_2021", type: "table", columns: ["commodity"] }]}
      />,
    );
    const content = container.querySelector<HTMLElement>(".cm-content")!;
    content.focus();
    fireEvent.keyDown(content, { key: "End", code: "End" });
    fireEvent.keyDown(content, { key: " ", code: "Space", ctrlKey: true });
    await waitFor(() =>
      expect(document.querySelector(".cm-tooltip-autocomplete")).toHaveTextContent("data_2021"),
    );
  });

  it("refreshes completion catalog without replacing editor text", async () => {
    const { container, rerender } = render(
      <SqlEditor
        onChange={vi.fn()}
        value="SELECT * FROM fresh_"
        tables={[{ schema: "main", label: "orders", type: "table", columns: ["id"] }]}
      />,
    );
    rerender(
      <SqlEditor
        onChange={vi.fn()}
        value="SELECT * FROM fresh_"
        tables={[
          { schema: "main", label: "orders", type: "table", columns: ["id"] },
          { schema: "main", label: "fresh_table", type: "view", columns: ["id"] },
        ]}
      />,
    );
    const content = container.querySelector<HTMLElement>(".cm-content")!;
    expect(content).toHaveTextContent("SELECT * FROM fresh_");
    content.focus();
    fireEvent.keyDown(content, { key: "End", code: "End" });
    fireEvent.keyDown(content, { key: " ", code: "Space", ctrlKey: true });
    await waitFor(() =>
      expect(document.querySelector(".cm-tooltip-autocomplete")).toHaveTextContent("fresh_table"),
    );
  });

  it("renders only reliable DuckDB diagnostic ranges as lint markers", async () => {
    const sql = "SELECT * FROM missing";
    const from = sql.indexOf("missing");
    const { container, rerender } = render(
      <SqlEditor
        diagnostics={[
          {
            code: "sql.catalog",
            message: "Table missing does not exist",
            severity: "error",
            from,
            to: from + "missing".length,
          },
        ]}
        onChange={vi.fn()}
        value={sql}
      />,
    );
    await waitFor(() => expect(container.querySelector(".cm-lintRange-error")).toBeInTheDocument());
    expect(container.querySelector(".cm-lint-marker-error")).toBeInTheDocument();

    rerender(
      <SqlEditor
        diagnostics={[
          {
            code: "sql.syntax",
            message: "syntax error at end of input",
            severity: "error",
            from: null,
            to: null,
          },
        ]}
        onChange={vi.fn()}
        value={sql}
      />,
    );
    await waitFor(() => expect(container.querySelector(".cm-lintRange-error")).toBeNull());
    expect(container.querySelector(".cm-lint-marker-error")).toBeNull();
  });

  it("reconfigures Dracula from effective theme without replacing editor state", async () => {
    const { container, rerender } = render(
      <SqlEditor effectiveTheme="light" onChange={vi.fn()} value="SELECT 1;" />,
    );
    const content = container.querySelector<HTMLElement>(".cm-content")!;
    content.focus();
    const editor = container.querySelector<HTMLElement>(".cm-editor")!;
    expect(editor).not.toHaveAttribute("data-theme", "dark");

    rerender(<SqlEditor effectiveTheme="dark" onChange={vi.fn()} value="SELECT 1;" />);

    await waitFor(() => expect(getComputedStyle(editor).backgroundColor).toBe("rgb(40, 42, 54)"));
    expect(content).toHaveTextContent("SELECT 1;");
    expect(document.activeElement).toBe(content);
  });

  it("handles Ctrl+Enter through the run callback", () => {
    const onRun = vi.fn();
    const { container } = render(
      <SqlEditor onChange={vi.fn()} onRun={onRun} value="SELECT * FROM orders;" />,
    );
    const content = container.querySelector<HTMLElement>(".cm-content")!;

    content.focus();
    fireEvent.keyDown(content, { key: "Enter", code: "Enter", ctrlKey: true });

    expect(onRun).toHaveBeenCalledWith("SELECT * FROM orders;");
  });
});
