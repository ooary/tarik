export interface CellCoordinate {
  row: number;
  column: number;
}

export interface CellSelection {
  anchor: CellCoordinate | null;
  cells: Set<string>;
}

export const emptySelection = (): CellSelection => ({ anchor: null, cells: new Set() });

export function cellKey(cell: CellCoordinate): string {
  return `${cell.row}:${cell.column}`;
}

export function selectCell(
  selection: CellSelection,
  cell: CellCoordinate,
  mode: "replace" | "extend" | "toggle",
): CellSelection {
  if (mode === "toggle") {
    const cells = new Set(selection.cells);
    const key = cellKey(cell);
    if (cells.has(key)) cells.delete(key);
    else cells.add(key);
    return { anchor: cell, cells };
  }
  if (mode === "extend" && selection.anchor) {
    const cells = new Set<string>();
    const firstRow = Math.min(selection.anchor.row, cell.row);
    const lastRow = Math.max(selection.anchor.row, cell.row);
    const firstColumn = Math.min(selection.anchor.column, cell.column);
    const lastColumn = Math.max(selection.anchor.column, cell.column);
    for (let row = firstRow; row <= lastRow; row += 1) {
      for (let column = firstColumn; column <= lastColumn; column += 1) {
        cells.add(cellKey({ row, column }));
      }
    }
    return { anchor: selection.anchor, cells };
  }
  return { anchor: cell, cells: new Set([cellKey(cell)]) };
}

export function selectedCoordinates(selection: CellSelection): CellCoordinate[] {
  return [...selection.cells]
    .map((key) => {
      const [row, column] = key.split(":").map(Number);
      return { row, column };
    })
    .sort((left, right) => left.row - right.row || left.column - right.column);
}

function clipboardValue(value: unknown): string {
  if (value === null || value === undefined) return "NULL";
  const text = String(value);
  return /[\t\n\r"]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

export function selectionToTsv(selection: CellSelection, rows: unknown[][]): string {
  const coordinates = selectedCoordinates(selection);
  const byRow = new Map<number, CellCoordinate[]>();
  for (const coordinate of coordinates) {
    const cells = byRow.get(coordinate.row) ?? [];
    cells.push(coordinate);
    byRow.set(coordinate.row, cells);
  }
  return [...byRow.entries()]
    .sort(([left], [right]) => left - right)
    .map(([row, cells]) =>
      cells
        .sort((left, right) => left.column - right.column)
        .map((cell) => clipboardValue(rows[row]?.[cell.column]))
        .join("\t"),
    )
    .join("\n");
}
