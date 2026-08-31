import { BaseEdge, getSmoothStepPath, type EdgeProps } from "@xyflow/react";
import type { CSSProperties } from "react";

interface TraversalEdgeData extends Record<string, unknown> {
  traversalDelayMs: number;
}

/** A persistent connector with one restrained source-to-result traversal pulse. */
export function FlowEdge({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerEnd,
  data,
}: EdgeProps) {
  const [path] = getSmoothStepPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    borderRadius: 10,
  });
  const delay = (data as TraversalEdgeData | undefined)?.traversalDelayMs ?? 0;
  const pulseStyle = { "--flow-traversal-delay": `${delay}ms` } as CSSProperties;

  return (
    <>
      <BaseEdge id={id} markerEnd={markerEnd} path={path} />
      <path
        aria-hidden="true"
        className="flow-edge-pulse"
        d={path}
        fill="none"
        pathLength={1}
        style={pulseStyle}
      />
    </>
  );
}
