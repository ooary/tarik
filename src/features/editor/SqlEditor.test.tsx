import { fireEvent, render } from "@testing-library/react";
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
        { schema: "analytics", label: "order lines", type: "table", columns: ["order id", "amount"] },
      ]),
    ).toEqual({ analytics: { "order lines": ["order id", "amount"] } });
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
