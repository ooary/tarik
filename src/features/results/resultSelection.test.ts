import { describe, expect, it } from "vitest";
import { emptySelection, selectCell, selectedCoordinates, selectionToTsv } from "./resultSelection";

describe("bounded result selection", () => {
  it("replaces, extends a rectangle, and toggles disjoint cells", () => {
    let selection = selectCell(emptySelection(), { row: 1, column: 1 }, "replace");
    selection = selectCell(selection, { row: 2, column: 2 }, "extend");
    expect(selectedCoordinates(selection)).toEqual([
      { row: 1, column: 1 },
      { row: 1, column: 2 },
      { row: 2, column: 1 },
      { row: 2, column: 2 },
    ]);
    selection = selectCell(selection, { row: 0, column: 0 }, "toggle");
    expect(selectedCoordinates(selection)[0]).toEqual({ row: 0, column: 0 });
    selection = selectCell(selection, { row: 2, column: 1 }, "toggle");
    expect(selectedCoordinates(selection)).not.toContainEqual({ row: 2, column: 1 });
  });

  it("serializes selected coordinates in stable TSV order with explicit NULL and escaping", () => {
    let selection = selectCell(emptySelection(), { row: 1, column: 1 }, "toggle");
    selection = selectCell(selection, { row: 0, column: 0 }, "toggle");
    selection = selectCell(selection, { row: 0, column: 1 }, "toggle");
    expect(
      selectionToTsv(selection, [
        [null, "a\tb"],
        ["skip", "line\nvalue"],
      ]),
    ).toBe('NULL\t"a\tb"\n"line\nvalue"');
  });
});
