import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { CSSProperties } from "react";
import { estimateAccuracy } from "./cardinality";
import type { PlanNodeData } from "./layout";

export function PlanNodeCard({ data, selected }: NodeProps & { data: PlanNodeData }) {
  const node = data.planNode;
  const accuracy = estimateAccuracy(node.estimatedRows, node.actualRows);
  const metric =
    node.actualRows != null && node.estimatedRows != null
      ? `Est. ~${node.estimatedRows.toLocaleString("en-US")} · Actual ${node.actualRows.toLocaleString("en-US")}`
      : node.actualRows != null
        ? `Actual output · ${node.actualRows.toLocaleString("en-US")} rows`
        : node.estimatedRows != null
          ? `DuckDB estimate · ~${node.estimatedRows.toLocaleString("en-US")} output rows`
          : null;
  const metricTitle =
    node.actualRows != null
      ? "Rows that actually left this operation during Profile"
      : node.estimatedRows != null
        ? "DuckDB's planning guess before execution—not the actual query result"
        : undefined;
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
        {metric && <span title={metricTitle}>{metric}</span>}
        {accuracy && (
          <span
            className={`flow-accuracy-badge flow-accuracy-${accuracy.level}`}
            title={`${accuracy.description} This measures estimate accuracy, not query speed.`}
          >
            {accuracy.label}
          </span>
        )}
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
