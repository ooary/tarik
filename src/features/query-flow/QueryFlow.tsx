import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  ReactFlowProvider,
  type NodeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useMemo } from "react";
import type { QueryPlan } from "../../lib/commands";
import { layoutPlan } from "./layout";
import { PlanNodeCard } from "./PlanNode";

const nodeTypes: NodeTypes = { plan: PlanNodeCard };

export function QueryFlow({ plan }: { plan: QueryPlan }) {
  const layout = useMemo(() => layoutPlan(plan.nodes, plan.edges), [plan]);

  if (plan.fallbackReason || plan.nodes.length === 0) {
    return (
      <div className="flow-fallback">
        <strong>Structured graph unavailable</strong>
        <span>{plan.fallbackReason ?? "This plan contains no operators."}</span>
        <details>
          <summary>Show raw plan</summary>
          <pre>{plan.rawPlan}</pre>
        </details>
      </div>
    );
  }

  return (
    <div className="query-flow" data-mode={plan.mode}>
      <ReactFlowProvider>
        <ReactFlow
          aria-label={`${plan.mode === "profile" ? "Profile" : "Explain"} query flow`}
          edges={layout.edges}
          elementsSelectable
          fitView
          fitViewOptions={{ padding: 0.18, maxZoom: 1.1 }}
          maxZoom={1.6}
          minZoom={0.25}
          nodes={layout.nodes}
          nodesConnectable={false}
          nodesDraggable={false}
          nodeTypes={nodeTypes}
          panOnDrag
          proOptions={{ hideAttribution: true }}
          zoomOnDoubleClick={false}
        >
          <Background
            color="var(--border-subtle)"
            gap={18}
            size={1}
            variant={BackgroundVariant.Dots}
          />
          <Controls position="bottom-right" showInteractive={false} />
        </ReactFlow>
      </ReactFlowProvider>
      <span className="flow-mode-label">
        {plan.mode === "profile" ? "Actual execution profile" : "Estimated execution plan"}
      </span>
    </div>
  );
}
