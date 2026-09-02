import { PresentationIcon } from "@phosphor-icons/react";
import { useState } from "react";
import type { PlanNode } from "../../lib/commands";
import type { FitViewOptions, Node } from "@xyflow/react";
import type { PlanNodeData } from "./layout";
import { presentationStartNode } from "./layout";

export function PresentFlowButton({
  nodes,
  onFocus,
  onPresent,
}: {
  nodes: Node<PlanNodeData>[];
  onFocus: (options: FitViewOptions<Node<PlanNodeData>>) => Promise<boolean>;
  onPresent: (node: PlanNode) => void;
}) {
  const [focusing, setFocusing] = useState(false);

  const present = async () => {
    const start = presentationStartNode(nodes);
    if (!start || focusing) return;
    const reduceMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    setFocusing(true);
    onPresent(start.data.planNode);
    try {
      await onFocus({
        nodes: [{ id: start.id }],
        padding: 0.9,
        minZoom: 1,
        maxZoom: 1.35,
        duration: reduceMotion ? 0 : 650,
        interpolate: "smooth",
      });
    } finally {
      setFocusing(false);
    }
  };

  return (
    <button
      aria-label="Present flow from start"
      className="flow-present-button"
      disabled={focusing}
      onClick={() => void present()}
      title="Focus the first source operation"
      type="button"
    >
      <PresentationIcon aria-hidden="true" size={15} />
      Present
    </button>
  );
}
