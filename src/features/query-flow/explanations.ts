import type { PlanNode } from "../../lib/commands";

export interface OperatorExplanation {
  title: string;
  summary: string;
  inputLabel: string;
  outputLabel: string;
  known: boolean;
}

const explanations: Record<string, Omit<OperatorExplanation, "known">> = {
  scan: {
    title: "Read data",
    summary: "DuckDB reads rows from a table, view, file, or generated source.",
    inputLabel: "Stored or generated data",
    outputLabel: "Rows read from the source",
  },
  filter: {
    title: "Filter rows",
    summary: "Rows that do not match the condition are removed.",
    inputLabel: "All input rows",
    outputLabel: "Rows matching the filter",
  },
  projection: {
    title: "Choose columns",
    summary: "DuckDB selects or calculates the columns needed by later steps.",
    inputLabel: "Input columns",
    outputLabel: "Selected or calculated columns",
  },
  join: {
    title: "Join",
    summary: "Rows from two inputs are matched using the join condition.",
    inputLabel: "Two row sets",
    outputLabel: "Matched rows",
  },
  aggregate: {
    title: "Group and summarize",
    summary: "Rows are grouped and aggregate functions calculate summary values.",
    inputLabel: "Detailed rows",
    outputLabel: "One summary row per group",
  },
  sort: {
    title: "Sort",
    summary: "Rows are ordered by one or more expressions.",
    inputLabel: "Unordered rows",
    outputLabel: "Ordered rows",
  },
  limit: {
    title: "Limit rows",
    summary: "Only the requested number of rows continues to the result.",
    inputLabel: "All input rows",
    outputLabel: "A bounded row set",
  },
  union: {
    title: "Combine results",
    summary: "Rows from multiple compatible inputs are combined into one stream.",
    inputLabel: "Multiple row sets",
    outputLabel: "Combined rows",
  },
  window: {
    title: "Window calculation",
    summary: "A value is calculated across related rows without collapsing them into groups.",
    inputLabel: "Detailed rows",
    outputLabel: "Original rows plus window values",
  },
  result: {
    title: "Query result",
    summary: "DuckDB returns the final rows and execution totals.",
    inputLabel: "Final operator output",
    outputLabel: "Rows returned to the query",
  },
};

export function explainOperator(node: PlanNode): OperatorExplanation {
  const known = explanations[node.operator];
  if (known) return { ...known, known: true };
  return {
    title: node.nativeName,
    summary:
      "DuckDB reported this operator, but Tarik does not have a verified beginner explanation for it yet. Native details are shown below.",
    inputLabel: "Operator input",
    outputLabel: "Operator output",
    known: false,
  };
}

export function importantDetails(node: PlanNode): Array<{ label: string; value: string }> {
  const preferred = [
    "Join Type",
    "Conditions",
    "Filters",
    "Groups",
    "Aggregates",
    "Projections",
    "Order By",
    "Top",
    "Table",
    "Type",
  ];
  const entries = Object.entries(node.details);
  const ordered = [
    ...preferred.flatMap((key) => {
      const entry = entries.find(([candidate]) => candidate === key);
      return entry ? [entry] : [];
    }),
    ...entries.filter(([key]) => !preferred.includes(key)),
  ];
  return ordered.map(([label, value]) => ({ label, value: displayDetail(value) }));
}

function displayDetail(value: unknown): string {
  if (Array.isArray(value)) return value.map(displayDetail).join(", ");
  if (value === null || value === undefined) return "Not reported";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}
