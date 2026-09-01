import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  ReactFlowProvider,
  type EdgeTypes,
  type NodeTypes,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useMemo, useState } from "react";
import type { PlanNode, QueryPlan } from "../../lib/commands";
import { FlowEdge } from "./FlowEdge";
import { layoutPlan } from "./layout";
import { NodeInspector } from "./NodeInspector";
import { PlanNodeCard } from "./PlanNode";

const nodeTypes: NodeTypes = { plan: PlanNodeCard };
const edgeTypes: EdgeTypes = { traversal: FlowEdge };

export function QueryFlow({
  onSelectNode,
  plan,
}: {
  onSelectNode?: (node: PlanNode | null) => void;
  plan: QueryPlan;
}) {
  const layout = useMemo(() => layoutPlan(plan.nodes, plan.edges), [plan]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = plan.nodes.find((node) => node.id === selectedId) ?? null;

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
    <div className="query-flow-shell">
      <div className="query-flow" data-mode={plan.mode}>
        <ReactFlowProvider>
          <ReactFlow
            aria-label={`${plan.mode === "profile" ? "Profile" : "Explain"} query flow`}
            edges={layout.edges}
            edgeTypes={edgeTypes}
            elementsSelectable
            fitView
            fitViewOptions={{ padding: 0.18, maxZoom: 1.1 }}
            maxZoom={1.6}
            minZoom={0.25}
            nodes={layout.nodes}
            nodesConnectable={false}
            nodesDraggable={false}
            nodeTypes={nodeTypes}
            onNodeClick={(_event, node) => {
              setSelectedId(node.id);
              onSelectNode?.(plan.nodes.find((candidate) => candidate.id === node.id) ?? null);
            }}
            onPaneClick={() => {
              setSelectedId(null);
              onSelectNode?.(null);
            }}
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
        <div className="flow-mode-label">
          <strong>
            {plan.mode === "profile" ? "Actual execution profile" : "Estimated execution plan"}
          </strong>
          <span>
            {plan.mode === "profile"
              ? "Row counts show what happened during execution."
              : "Row counts are DuckDB planning guesses, not query results."}
          </span>
        </div>
      </div>
      <NodeInspector mode={plan.mode} node={selected} />
    </div>
  );
}
