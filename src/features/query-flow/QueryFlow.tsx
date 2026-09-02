import {
  Background,
  BackgroundVariant,
  Controls,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
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
import { PresentFlowButton } from "./PresentFlowButton";

const nodeTypes: NodeTypes = { plan: PlanNodeCard };
const edgeTypes: EdgeTypes = { traversal: FlowEdge };

function QueryFlowCanvas({
  layout,
  onSelectNode,
  plan,
}: {
  layout: ReturnType<typeof layoutPlan>;
  onSelectNode?: (node: PlanNode | null) => void;
  plan: QueryPlan;
}) {
  const flow = useReactFlow();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selected = plan.nodes.find((node) => node.id === selectedId) ?? null;
  const visibleNodes = useMemo(
    () => layout.nodes.map((node) => ({ ...node, selected: node.id === selectedId })),
    [layout.nodes, selectedId],
  );
  const selectNode = (node: PlanNode | null) => {
    setSelectedId(node?.id ?? null);
    onSelectNode?.(node);
  };

  return (
    <div className="query-flow-shell">
      <div className="query-flow" data-mode={plan.mode}>
        <ReactFlow
          aria-label={`${plan.mode === "profile" ? "Actual" : "Estimated"} query flow`}
          edges={layout.edges}
          edgeTypes={edgeTypes}
          elementsSelectable
          fitView
          fitViewOptions={{ padding: 0.18, maxZoom: 1.1 }}
          maxZoom={1.6}
          minZoom={0.25}
          nodes={visibleNodes}
          nodesConnectable={false}
          nodesDraggable={false}
          nodeTypes={nodeTypes}
          onNodeClick={(_event, node) => {
            selectNode(plan.nodes.find((candidate) => candidate.id === node.id) ?? null);
          }}
          onPaneClick={() => selectNode(null)}
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
          <PresentFlowButton
            nodes={layout.nodes}
            onFocus={(options) => flow.fitView(options)}
            onPresent={selectNode}
          />
        </ReactFlow>
        <div className="flow-mode-label">
          <strong>
            {plan.mode === "profile" ? "Actual Flow · DuckDB Profile" : "Estimate · DuckDB Explain"}
          </strong>
          <span>
            {plan.mode === "profile"
              ? "Measured rows, rows scanned, and operator time from executing this SQL."
              : "Planned operations and row-count guesses—not query results."}
          </span>
        </div>
      </div>
      <NodeInspector mode={plan.mode} node={selected} />
    </div>
  );
}

export function QueryFlow({
  onSelectNode,
  plan,
}: {
  onSelectNode?: (node: PlanNode | null) => void;
  plan: QueryPlan;
}) {
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
    <ReactFlowProvider>
      <QueryFlowCanvas layout={layout} onSelectNode={onSelectNode} plan={plan} />
    </ReactFlowProvider>
  );
}
