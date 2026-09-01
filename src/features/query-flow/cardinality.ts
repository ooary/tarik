export type EstimateAccuracyLevel = "good" | "warning" | "poor";
export type EstimateDirection = "high" | "low" | "exact";

export interface EstimateAccuracy {
  factor: number;
  level: EstimateAccuracyLevel;
  direction: EstimateDirection;
  label: string;
  description: string;
}

/**
 * Symmetric cardinality error factor. It catches both over-estimates and
 * under-estimates instead of treating one direction as inherently better.
 */
export function estimateAccuracy(
  estimatedRows: number | null,
  actualRows: number | null,
): EstimateAccuracy | null {
  if (estimatedRows == null || actualRows == null || estimatedRows < 0 || actualRows < 0) {
    return null;
  }
  let factor: number;
  let direction: EstimateDirection;
  if (estimatedRows === actualRows) {
    factor = 1;
    direction = "exact";
  } else if (actualRows === 0 || estimatedRows === 0) {
    factor = Number.POSITIVE_INFINITY;
    direction = estimatedRows > actualRows ? "high" : "low";
  } else if (estimatedRows > actualRows) {
    factor = estimatedRows / actualRows;
    direction = "high";
  } else {
    factor = actualRows / estimatedRows;
    direction = "low";
  }
  const level: EstimateAccuracyLevel = factor < 10 ? "good" : factor <= 100 ? "warning" : "poor";
  const factorLabel = Number.isFinite(factor) ? formatFactor(factor) : "∞×";
  const label =
    direction === "exact"
      ? "Estimate matched"
      : `${direction === "high" ? "Over-estimate" : "Under-estimate"} · ${factorLabel}`;
  const description =
    direction === "exact"
      ? "DuckDB's estimate matched the measured output."
      : `DuckDB estimated ${direction === "high" ? "more" : "fewer"} output rows than Profile measured by a factor of ${factorLabel}.`;
  return { factor, level, direction, label, description };
}

function formatFactor(factor: number): string {
  if (factor >= 100) return `${Math.round(factor).toLocaleString("en-US")}×`;
  if (factor >= 10) return `${factor.toFixed(1)}×`;
  return `${factor.toFixed(1)}×`;
}
