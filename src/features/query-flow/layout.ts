import { MarkerType, type Edge, type Node } from "@xyflow/react";
import type { PlanEdge, PlanNode } from "../../lib/commands";

const NODE_WIDTH = 220;
const NODE_HEIGHT = 92;
const COLUMN_GAP = 110;
const ROW_GAP = 34;

export interface PlanNodeData extends Record<string, unknown> {
  planNode: PlanNode;
  traversalDelayMs: number;
}

/**
 * Deterministic source-to-result layout. Edges already flow child -> parent;
 * node depth is the longest distance from an input/source. Every depth column
 * is ordered by normalized preorder id, so the same graph always lands in the
 * same place without async layout engines or unstable measurements.
 */
export function layoutPlan(
  nodes: PlanNode[],
  edges: PlanEdge[],
): { nodes: Node<PlanNodeData>[]; edges: Edge[] } {
  const incoming = new Map<string, string[]>();
  for (const edge of edges) {
    incoming.set(edge.target, [...(incoming.get(edge.target) ?? []), edge.source]);
  }
  const memo = new Map<string, number>();
  const visiting = new Set<string>();
  const depth = (id: string): number => {
    if (memo.has(id)) return memo.get(id)!;
    if (visiting.has(id)) return 0; // malformed cycle: stable fallback, parser still preserves graph
    visiting.add(id);
    const inputs = incoming.get(id) ?? [];
    const value = inputs.length === 0 ? 0 : Math.max(...inputs.map(depth)) + 1;
    visiting.delete(id);
    memo.set(id, value);
    return value;
  };

  const byDepth = new Map<number, PlanNode[]>();
  for (const node of nodes) {
    const level = depth(node.id);
    byDepth.set(level, [...(byDepth.get(level) ?? []), node]);
  }
  for (const level of byDepth.values()) {
    level.sort((left, right) => numericId(left.id) - numericId(right.id));
  }
  const maxRows = Math.max(1, ...[...byDepth.values()].map((level) => level.length));
  const totalHeight = maxRows * NODE_HEIGHT + (maxRows - 1) * ROW_GAP;

  const positioned = nodes.map((node): Node<PlanNodeData> => {
    const level = depth(node.id);
    const siblings = byDepth.get(level) ?? [];
    const index = siblings.findIndex((sibling) => sibling.id === node.id);
    const levelHeight = siblings.length * NODE_HEIGHT + Math.max(0, siblings.length - 1) * ROW_GAP;
    return {
      id: node.id,
      type: "plan",
      data: { planNode: node, traversalDelayMs: level * 620 },
      position: {
        x: level * (NODE_WIDTH + COLUMN_GAP),
        y: (totalHeight - levelHeight) / 2 + index * (NODE_HEIGHT + ROW_GAP),
      },
      draggable: false,
      selectable: true,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
    };
  });
  const flowEdges: Edge[] = edges.map((edge) => ({
    id: edge.id,
    source: edge.source,
    target: edge.target,
    type: "traversal",
    data: { traversalDelayMs: depth(edge.source) * 620 + 100 },
    markerEnd: { type: MarkerType.ArrowClosed },
    animated: false,
  }));
  return { nodes: positioned, edges: flowEdges };
}

function numericId(id: string): number {
  const parsed = Number.parseInt(id.replace(/^\D+/, ""), 10);
  return Number.isFinite(parsed) ? parsed : Number.MAX_SAFE_INTEGER;
}
