import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { Node } from "@xyflow/react";
import type { PlanNodeData } from "./layout";
import { PresentFlowButton } from "./PresentFlowButton";

const planNode = (id: string, source: string) => ({
  id,
  operator: "scan",
  nativeName: "SEQ_SCAN",
  source,
  estimatedRows: 10,
  actualRows: null,
  timingMs: null,
  rowsScanned: null,
  details: {},
});

const nodes: Node<PlanNodeData>[] = [
  {
    id: "n2",
    position: { x: 0, y: 160 },
    data: { planNode: planNode("n2", "customers"), traversalDelayMs: 0 },
  },
  {
    id: "n1",
    position: { x: 0, y: 0 },
    data: { planNode: planNode("n1", "orders"), traversalDelayMs: 0 },
  },
];

describe("PresentFlowButton", () => {
  it("selects and smoothly focuses the upper-left source", async () => {
    const onFocus = vi.fn().mockResolvedValue(true);
    const onPresent = vi.fn();
    render(<PresentFlowButton nodes={nodes} onFocus={onFocus} onPresent={onPresent} />);

    fireEvent.click(screen.getByRole("button", { name: "Present flow from start" }));
    expect(onPresent).toHaveBeenCalledWith(expect.objectContaining({ id: "n1" }));
    await waitFor(() =>
      expect(onFocus).toHaveBeenCalledWith({
        nodes: [{ id: "n1" }],
        padding: 0.9,
        minZoom: 1,
        maxZoom: 1.35,
        duration: 650,
        interpolate: "smooth",
      }),
    );
  });
});
