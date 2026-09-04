import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { getResultPage } from "../../lib/commands";
import { ResultGrid } from "./ResultGrid";

vi.mock("../../lib/commands", () => ({ getResultPage: vi.fn() }));

const page = {
  resultId: "result-1",
  offset: 0,
  rowTotal: 3,
  rowTotalExact: true,
  columns: [
    { name: "name", logicalType: "string", nativeType: "VARCHAR", nullable: false },
    { name: "value", logicalType: "string", nativeType: "VARCHAR", nullable: true },
  ],
  rows: [
    ["alpha", 1],
    ["beta", null],
    ["gamma", "tab\tvalue"],
  ],
  truncatedCells: [],
  cached: false,
};

describe("ResultGrid interactions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getResultPage).mockResolvedValue(page);
    Object.assign(navigator, { clipboard: { writeText: vi.fn().mockResolvedValue(undefined) } });
  });

  it("selects one cell, a Shift rectangle, and Ctrl disjoint cells", async () => {
    const { container } = render(<ResultGrid resultId="result-1" rowTotal={3} />);
    await screen.findByText("alpha");
    const cell = (row: number, column: number) =>
      container.querySelector<HTMLElement>(`[data-row='${row}'][data-column='${column}']`)!;

    fireEvent.click(cell(0, 0));
    fireEvent.click(cell(1, 1), { shiftKey: true });
    expect(container.querySelectorAll("[aria-selected='true']")).toHaveLength(4);
    fireEvent.click(cell(2, 0), { ctrlKey: true });
    expect(container.querySelectorAll("[aria-selected='true']")).toHaveLength(5);
  });

  it("copies bounded selected cells and reruns through the supplied snapshot callback", async () => {
    const onRunAgain = vi.fn();
    const { container } = render(
      <ResultGrid onRunAgain={onRunAgain} resultId="result-1" rowTotal={3} />,
    );
    await screen.findByText("alpha");
    const first = container.querySelector<HTMLElement>("[data-row='0'][data-column='0']")!;
    const nullCell = container.querySelector<HTMLElement>("[data-row='1'][data-column='1']")!;
    fireEvent.click(first);
    fireEvent.click(nullCell, { ctrlKey: true });
    fireEvent.contextMenu(nullCell);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Copy selected cells" }));
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalledWith("alpha\nNULL"));

    fireEvent.contextMenu(first);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Run query again" }));
    expect(onRunAgain).toHaveBeenCalledOnce();
  });

  it("does not open a Tarik context menu from column headers", async () => {
    render(<ResultGrid resultId="result-1" rowTotal={3} />);
    const header = await screen.findByText("name");
    fireEvent.contextMenu(header);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
