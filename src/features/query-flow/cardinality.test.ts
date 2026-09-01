import { describe, expect, it } from "vitest";
import { estimateAccuracy } from "./cardinality";

describe("estimateAccuracy", () => {
  it("marks differences below 10x green", () => {
    expect(estimateAccuracy(18, 2)).toMatchObject({
      direction: "high",
      factor: 9,
      level: "good",
      label: "Over-estimate · 9.0×",
    });
  });

  it("marks 10x through 100x yellow", () => {
    expect(estimateAccuracy(147, 4)).toMatchObject({
      direction: "high",
      factor: 36.75,
      level: "warning",
      label: "Over-estimate · 36.8×",
    });
    expect(estimateAccuracy(1, 100)).toMatchObject({ level: "warning" });
  });

  it("marks differences above 100x red", () => {
    expect(estimateAccuracy(101, 1)).toMatchObject({
      direction: "high",
      factor: 101,
      level: "poor",
      label: "Over-estimate · 101×",
    });
  });

  it("detects under-estimates symmetrically", () => {
    expect(estimateAccuracy(4, 147)).toMatchObject({
      direction: "low",
      factor: 36.75,
      level: "warning",
      label: "Under-estimate · 36.8×",
    });
  });

  it("handles zero and exact cardinalities without division errors", () => {
    expect(estimateAccuracy(0, 4)).toMatchObject({
      direction: "low",
      factor: Number.POSITIVE_INFINITY,
      level: "poor",
      label: "Under-estimate · ∞×",
    });
    expect(estimateAccuracy(0, 0)).toMatchObject({
      direction: "exact",
      factor: 1,
      level: "good",
      label: "Estimate matched",
    });
  });

  it("does not create a comparison unless both values exist", () => {
    expect(estimateAccuracy(147, null)).toBeNull();
    expect(estimateAccuracy(null, 4)).toBeNull();
  });
});
