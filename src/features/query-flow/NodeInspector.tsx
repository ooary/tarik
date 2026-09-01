import type { PlanNode, PlanMode } from "../../lib/commands";
import { estimateAccuracy } from "./cardinality";
import { explainOperator, importantDetails } from "./explanations";

export function NodeInspector({ node, mode }: { node: PlanNode | null; mode: PlanMode }) {
  if (!node) {
    return (
      <aside className="flow-inspector flow-inspector-empty" aria-label="Plan node inspector">
        <strong>Select an operation</strong>
        <span>Choose a node to see what it does and which rows it processes.</span>
      </aside>
    );
  }
  const explanation = explainOperator(node);
  const details = importantDetails(node);
  const accuracy = estimateAccuracy(node.estimatedRows, node.actualRows);
  return (
    <aside className="flow-inspector" aria-label="Plan node inspector">
      <header>
        <span>{mode === "profile" ? "Actual operation" : "Estimated operation"}</span>
        <h3>{explanation.title}</h3>
        <code>{node.nativeName}</code>
      </header>
      <p>{explanation.summary}</p>
      {node.presentationNote && (
        <div className="flow-interpretation-note" role="note">
          <strong>Beginner presentation</strong>
          <span>{node.presentationNote}</span>
        </div>
      )}
      {mode === "explain" && node.estimatedRows != null && (
        <div className="flow-estimate-note" role="note">
          <strong>Planning estimate, not result count</strong>
          <span>
            DuckDB guessed that about {node.estimatedRows.toLocaleString("en-US")} rows would leave
            this operation before running the query. This guess helps choose an execution strategy;
            it does not filter, limit, or describe the actual result.
          </span>
          {node.operator === "projection" && (
            <span>
              Return columns normally keeps the same row count as its input because it changes
              columns, not which rows match.
            </span>
          )}
        </div>
      )}
      {mode === "profile" && node.actualRows != null && (
        <div className="flow-actual-note" role="note">
          <strong>Measured during execution</strong>
          <span>
            {node.actualRows.toLocaleString("en-US")} rows actually left this operation during
            Profile.
          </span>
          {accuracy && (
            <div className="flow-accuracy-summary">
              <span className={`flow-accuracy-badge flow-accuracy-${accuracy.level}`}>
                {accuracy.label}
              </span>
              <span>{accuracy.description}</span>
              <small>
                This badge measures estimate accuracy, not whether the query is fast or efficient.
                Check operator time and rows scanned for performance.
              </small>
            </div>
          )}
        </div>
      )}
      <dl className="flow-inspector-io">
        <div>
          <dt>Input</dt>
          <dd>{explanation.inputLabel}</dd>
        </div>
        <div>
          <dt>Output</dt>
          <dd>{explanation.outputLabel}</dd>
        </div>
      </dl>
      <dl className="flow-inspector-metrics">
        {node.source && (
          <div>
            <dt>Source</dt>
            <dd>{node.source}</dd>
          </div>
        )}
        {node.estimatedRows != null && (
          <div>
            <dt>DuckDB estimated output</dt>
            <dd>~{node.estimatedRows.toLocaleString("en-US")} rows</dd>
          </div>
        )}
        {node.actualRows != null && (
          <div>
            <dt>Actual output</dt>
            <dd>{node.actualRows.toLocaleString("en-US")} rows</dd>
          </div>
        )}
        {node.timingMs != null && (
          <div>
            <dt>Operator time</dt>
            <dd>{formatMilliseconds(node.timingMs)}</dd>
          </div>
        )}
        {node.rowsScanned != null && (
          <div>
            <dt>Rows scanned</dt>
            <dd>{node.rowsScanned.toLocaleString("en-US")}</dd>
          </div>
        )}
      </dl>
      {details.length > 0 && (
        <section>
          <h4>DuckDB details</h4>
          <dl className="flow-inspector-details">
            {details.map((detail) => (
              <div key={detail.label}>
                <dt>{detail.label}</dt>
                <dd>{detail.value}</dd>
              </div>
            ))}
          </dl>
        </section>
      )}
      {!explanation.known && (
        <p className="flow-inspector-note">
          No verified beginner explanation is available for this operator.
        </p>
      )}
    </aside>
  );
}

function formatMilliseconds(value: number): string {
  if (value < 0.01) return "<0.01 ms";
  if (value < 10) return `${value.toFixed(2)} ms`;
  return `${value.toFixed(1)} ms`;
}
