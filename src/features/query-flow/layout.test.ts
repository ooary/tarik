import { describe, expect, it } from "vitest";
import type { PlanEdge, PlanNode } from "../../lib/commands";
import { layoutPlan, presentationStartNode } from "./layout";

const nodes: PlanNode[] = [
  {
    id: "n0",
    operator: "join",
    nativeName: "HASH_JOIN",
    source: null,
    estimatedRows: 10,
    actualRows: null,
    timingMs: null,
    rowsScanned: null,
    details: {},
  },
  {
    id: "n1",
    operator: "scan",
    nativeName: "SEQ_SCAN",
    source: "fixture.main.orders",
    estimatedRows: 100,
    actualRows: null,
    timingMs: null,
    rowsScanned: null,
    details: {},
  },
  {
    id: "n2",
    operator: "scan",
    nativeName: "SEQ_SCAN",
    source: "fixture.main.customers",
    estimatedRows: 10,
    actualRows: null,
    timingMs: null,
    rowsScanned: null,
    details: {},
  },
];
const edges: PlanEdge[] = [
  { id: "e0", source: "n1", target: "n0" },
  { id: "e1", source: "n2", target: "n0" },
];

describe("layoutPlan", () => {
  it("is deterministic and positions two scan inputs before their join", () => {
    const first = layoutPlan(nodes, edges);
    const second = layoutPlan(nodes, edges);
    expect(first).toEqual(second);
    const join = first.nodes.find((node) => node.id === "n0")!;
    const scans = first.nodes.filter((node) => node.data.planNode.operator === "scan");
    expect(scans).toHaveLength(2);
    expect(scans.every((scan) => scan.position.x < join.position.x)).toBe(true);
    expect(new Set(scans.map((scan) => scan.position.y)).size).toBe(2);
    expect(first.edges.map((edge) => [edge.source, edge.target])).toEqual([
      ["n1", "n0"],
      ["n2", "n0"],
    ]);
    expect(first.edges.every((edge) => edge.type === "traversal")).toBe(true);
    expect(first.edges.every((edge) => edge.markerEnd != null)).toBe(true);
    expect(first.edges.map((edge) => edge.data?.traversalDelayMs)).toEqual([100, 100]);
    expect(presentationStartNode(first.nodes)?.id).toBe("n1");
  });

  it("selects the upper-left source deterministically for presentation", () => {
    const layout = layoutPlan(nodes, edges);
    expect(presentationStartNode([...layout.nodes].reverse())?.id).toBe("n1");
    expect(presentationStartNode([])).toBeNull();
  });
});
