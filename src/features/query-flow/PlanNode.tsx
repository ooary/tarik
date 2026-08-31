import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { CSSProperties } from "react";
import type { PlanNodeData } from "./layout";

export function PlanNodeCard({ data, selected }: NodeProps & { data: PlanNodeData }) {
  const node = data.planNode;
  const metric =
    node.actualRows != null
      ? `${node.actualRows.toLocaleString("en-US")} actual rows`
      : node.estimatedRows != null
        ? `~${node.estimatedRows.toLocaleString("en-US")} estimated rows`
        : null;
  const traversalStyle = {
    "--flow-node-delay": `${data.traversalDelayMs}ms`,
  } as CSSProperties;
  return (
    <div
      className={`flow-node flow-node-traversal ${selected ? "flow-node-selected" : ""}`}
      style={traversalStyle}
    >
      <Handle aria-label="Input" position={Position.Left} type="target" />
      <div className="flow-node-heading">
        <strong>{displayOperator(node.operator, node.nativeName)}</strong>
        <span>{node.nativeName}</span>
      </div>
      {node.source && (
        <span className="flow-node-source" title={node.source}>
          {shortSource(node.source)}
        </span>
      )}
      <div className="flow-node-metrics">
        {metric && <span>{metric}</span>}
        {node.timingMs != null && <span>{formatTime(node.timingMs)}</span>}
      </div>
      <Handle aria-label="Output" position={Position.Right} type="source" />
    </div>
  );
}

function displayOperator(operator: string, nativeName: string): string {
  const labels: Record<string, string> = {
    scan: "Read data",
    join: "Join",
    aggregate: "Group & summarize",
    filter: "Filter rows",
    projection: "Choose columns",
    sort: "Sort",
    limit: "Limit rows",
    union: "Combine results",
    window: "Window calculation",
    result: "Query result",
  };
  return labels[operator] ?? nativeName.replace(/_/g, " ").toLowerCase();
}

function shortSource(source: string): string {
  const parts = source.split(".");
  return (parts[parts.length - 1] ?? source).replace(/"/g, "");
}

function formatTime(milliseconds: number): string {
  if (milliseconds < 0.01) return "<0.01 ms";
  if (milliseconds < 10) return `${milliseconds.toFixed(2)} ms`;
  return `${milliseconds.toFixed(1)} ms`;
}
