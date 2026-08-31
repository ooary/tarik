import { Position, type EdgeProps } from "@xyflow/react";
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FlowEdge } from "./FlowEdge";

describe("FlowEdge", () => {
  it("renders a persistent connector and a directional traversal pulse", () => {
    const props = {
      id: "e0",
      source: "n1",
      target: "n0",
      sourceX: 220,
      sourceY: 46,
      targetX: 330,
      targetY: 46,
      sourcePosition: Position.Right,
      targetPosition: Position.Left,
      markerEnd: "url(#arrow)",
      data: { traversalDelayMs: 100 },
    } as EdgeProps;
    const { container } = render(
      <svg>
        <FlowEdge {...props} />
      </svg>,
    );

    const base = container.querySelector<SVGPathElement>("#e0");
    const pulse = container.querySelector<SVGPathElement>(".flow-edge-pulse");
    expect(base).not.toBeNull();
    expect(pulse).not.toBeNull();
    expect(base!.getAttribute("d")).not.toBe("");
    expect(pulse!.getAttribute("d")).toBe(base!.getAttribute("d"));
    expect(base).toHaveAttribute("marker-end", "url(#arrow)");
    expect(pulse!.getAttribute("style")).toContain("--flow-traversal-delay: 100ms");
  });
});
